use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[path = "podman_reclaim.rs"]
mod implementation;

pub use implementation::{
    inspect_raw_image_evidence, probe_podman_reclaim, GuestFilesystemEvidence,
    PodmanDanglingImagePruneExecution, PodmanHostCompactionPlan, PodmanMachineEvidence, PodmanReclaimAssessment,
    PodmanReclaimPlan, PodmanRecommendedAction, PodmanRecommendedActionKind,
    PodmanStorageCheckPlan, PodmanStorageRepairExecution, PodmanStoreEvidence,
    PodmanSystemDfCategoryEvidence, PodmanSystemDfEvidence, PodmanUnusedImageEvidence,
    RawImageEvidence, DEFAULT_PODMAN_MACHINE, DEFAULT_PROBE_TIMEOUT, PODMAN_RECLAIM_SCHEMA_KIND,
};

const MAX_CAPTURE_BYTES: usize = 1_048_576;
const MAX_EXACT_DELETE_IDS: usize = 256;
const PODMAN_PRUNE_TIMEOUT: Duration = Duration::from_secs(30);
const PODMAN_PRUNE_SCHEMA_VERSION: u32 = 1;
const PODMAN_STORAGE_CHECK_TIMEOUT: Duration = Duration::from_secs(120);
const MUTATION_TIMEOUT_STATUS_CODE: i32 = -124;
const MUTATION_WAIT_STATUS_CODE: i32 = -125;
const MUTATION_CAPTURE_STATUS_CODE: i32 = -126;
const MUTATION_UTF8_STATUS_CODE: i32 = -127;

#[derive(Debug, Deserialize)]
struct MachineInspectRecord {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "State")]
    state: String,
}

#[derive(Debug, Deserialize)]
struct PodmanImageRecord {
    #[serde(rename = "Id")]
    id: String,
    #[serde(rename = "RepoTags")]
    repo_tags: Option<Vec<String>>,
    #[serde(rename = "RepoDigests")]
    repo_digests: Option<Vec<String>>,
    #[serde(rename = "Names")]
    names: Option<Vec<String>>,
    #[serde(rename = "Containers")]
    containers: u64,
    #[serde(rename = "Size")]
    size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ExactCandidate {
    id: String,
    tags: Vec<String>,
    size_bytes: u64,
}

#[derive(Debug)]
struct BoundedOutput {
    status_code: i32,
    stdout: String,
    stderr: String,
}

fn valid_machine_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value != "."
        && value != ".."
        && !value.starts_with('-')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn drain_bounded<R: Read>(mut reader: R) -> std::io::Result<Vec<u8>> {
    let mut captured = Vec::with_capacity(MAX_CAPTURE_BYTES.min(64 * 1024));
    let mut buffer = [0u8; 16 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        let remaining = MAX_CAPTURE_BYTES.saturating_sub(captured.len());
        if read > remaining {
            return Err(std::io::Error::other("output-too-large"));
        }
        captured.extend_from_slice(&buffer[..read]);
    }
    Ok(captured)
}

fn run_bounded(
    executable: &Path,
    args: &[&str],
    timeout: Duration,
    label: &str,
) -> Result<BoundedOutput, String> {
    let mut command = Command::new(executable);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command.spawn().map_err(|_| format!("{label}-spawn"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| format!("{label}-stdout-pipe-unavailable"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| format!("{label}-stderr-pipe-unavailable"))?;
    let stdout_reader = thread::spawn(move || drain_bounded(stdout));
    let stderr_reader = thread::spawn(move || drain_bounded(stderr));
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() >= timeout => {
                terminate_readonly_process_tree(&mut child);
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(format!("{label}-timeout"));
            }
            Ok(None) => thread::sleep(Duration::from_millis(25)),
            Err(_) => {
                terminate_readonly_process_tree(&mut child);
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(format!("{label}-wait"));
            }
        }
    };
    // A successful direct child may still leave descendants holding inherited capture pipes.
    // Terminate the private group before joining readers so completion remains truly bounded.
    terminate_readonly_process_tree(&mut child);
    let stdout = stdout_reader
        .join()
        .map_err(|_| format!("{label}-stdout-reader-panicked"))?
        .map_err(|_| format!("{label}-output-too-large"))?;
    let stderr = stderr_reader
        .join()
        .map_err(|_| format!("{label}-stderr-reader-panicked"))?
        .map_err(|_| format!("{label}-output-too-large"))?;
    Ok(BoundedOutput {
        status_code: status.code().unwrap_or(-1),
        stdout: String::from_utf8(stdout).map_err(|_| format!("{label}-stdout-not-utf8"))?,
        stderr: String::from_utf8(stderr).map_err(|_| format!("{label}-stderr-not-utf8"))?,
    })
}

fn terminate_readonly_process_tree(child: &mut std::process::Child) {
    #[cfg(unix)]
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Execute a mutating command without losing the fact that it was spawned.
///
/// Spawn failure is still an error because no mutation could have started. Once the child exists,
/// every timeout/wait/capture/UTF-8 failure is converted into a stable negative status sentinel so
/// the caller can run a fresh postcheck and emit a receipt instead of erasing a possibly completed
/// mutation from the audit trail.
fn run_mutation_bounded(
    executable: &Path,
    args: &[&str],
    timeout: Duration,
    label: &str,
) -> Result<BoundedOutput, String> {
    let mut command = Command::new(executable);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command.spawn().map_err(|_| format!("{label}-spawn"))?;

    let Some(stdout) = child.stdout.take() else {
        terminate_mutation_process_tree(&mut child);
        return Ok(BoundedOutput {
            status_code: MUTATION_CAPTURE_STATUS_CODE,
            stdout: String::new(),
            stderr: String::new(),
        });
    };
    let Some(stderr) = child.stderr.take() else {
        terminate_mutation_process_tree(&mut child);
        return Ok(BoundedOutput {
            status_code: MUTATION_CAPTURE_STATUS_CODE,
            stdout: String::new(),
            stderr: String::new(),
        });
    };

    let stdout_reader = thread::spawn(move || drain_bounded(stdout));
    let stderr_reader = thread::spawn(move || drain_bounded(stderr));
    let started = Instant::now();
    let status_code = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.code().unwrap_or(-1),
            Ok(None) if started.elapsed() >= timeout => {
                terminate_mutation_process_tree(&mut child);
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Ok(BoundedOutput {
                    status_code: MUTATION_TIMEOUT_STATUS_CODE,
                    stdout: String::new(),
                    stderr: String::new(),
                });
            }
            Ok(None) => thread::sleep(Duration::from_millis(25)),
            Err(_) => {
                terminate_mutation_process_tree(&mut child);
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Ok(BoundedOutput {
                    status_code: MUTATION_WAIT_STATUS_CODE,
                    stdout: String::new(),
                    stderr: String::new(),
                });
            }
        }
    };

    // A completed leader does not imply that inherited capture pipes are closed: a descendant
    // may still own them. The command was isolated in a private process group, so terminate any
    // remaining descendants before joining the readers and preserve the leader's exit status.
    terminate_mutation_process_tree(&mut child);

    let stdout = match stdout_reader.join() {
        Ok(Ok(value)) => value,
        Ok(Err(_)) | Err(_) => {
            let _ = stderr_reader.join();
            return Ok(BoundedOutput {
                status_code: MUTATION_CAPTURE_STATUS_CODE,
                stdout: String::new(),
                stderr: String::new(),
            });
        }
    };
    let stderr = match stderr_reader.join() {
        Ok(Ok(value)) => value,
        Ok(Err(_)) | Err(_) => {
            return Ok(BoundedOutput {
                status_code: MUTATION_CAPTURE_STATUS_CODE,
                stdout: String::new(),
                stderr: String::new(),
            });
        }
    };
    let Ok(stdout) = String::from_utf8(stdout) else {
        return Ok(BoundedOutput {
            status_code: MUTATION_UTF8_STATUS_CODE,
            stdout: String::new(),
            stderr: String::new(),
        });
    };
    let Ok(stderr) = String::from_utf8(stderr) else {
        return Ok(BoundedOutput {
            status_code: MUTATION_UTF8_STATUS_CODE,
            stdout: String::new(),
            stderr: String::new(),
        });
    };

    Ok(BoundedOutput {
        status_code,
        stdout,
        stderr,
    })
}

fn terminate_mutation_process_tree(child: &mut std::process::Child) {
    #[cfg(unix)]
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn run_text(
    executable: &Path,
    args: &[&str],
    timeout: Duration,
    label: &str,
) -> Result<String, String> {
    let output = run_bounded(executable, args, timeout, label)?;
    if output.status_code != 0 {
        return Err(format!("{label}-failed"));
    }
    Ok(output.stdout)
}

fn hash_frame(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
}

fn lower_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        encoded.push(char::from(DIGITS[usize::from(byte >> 4)]));
        encoded.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn damaged_layer_ids(output: &str) -> Result<Vec<String>, String> {
    let mut ids = output
        .lines()
        .filter_map(|line| line.strip_prefix("Damaged layer "))
        .filter_map(|line| line.strip_suffix(':'))
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>();
    if ids
        .iter()
        .any(|id| id.len() != 64 || !id.bytes().all(|byte| byte.is_ascii_hexdigit()))
    {
        return Err("podman-storage-check-invalid-layer-id".into());
    }
    ids.sort_unstable();
    ids.dedup();
    Ok(ids)
}

fn storage_check_fingerprint(ids: &[String]) -> String {
    let mut digest = Sha256::new();
    digest.update(b"disksage.podman-storage-check.v1\0");
    for id in ids {
        hash_frame(&mut digest, id.as_bytes());
    }
    lower_hex(&digest.finalize())
}

fn storage_repair_scope_fingerprint(machine: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(b"disksage.podman-machine-storage-repair-scope.v1\0");
    hash_frame(&mut digest, machine.as_bytes());
    for token in [
        "podman",
        "--connection",
        machine,
        "system",
        "check",
        "--quick",
        "--repair",
    ] {
        hash_frame(&mut digest, token.as_bytes());
    }
    lower_hex(&digest.finalize())
}

fn storage_check_complete(status_code: i32, output: &str, ids: &[String]) -> bool {
    let expected_damage_completion = output
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .is_some_and(|line| line == "Error: damage detected in local storage");
    (status_code == 0 && ids.is_empty()) || (!ids.is_empty() && expected_damage_completion)
}

fn storage_repair_provider_issue(output: &BoundedOutput) -> Option<String> {
    if output.status_code == 0 {
        return None;
    }
    if output.status_code != 125 {
        return Some("podman-storage-repair-provider-exit-status-unexpected".into());
    }
    let provider_diagnostic = format!("{}\n{}", output.stdout, output.stderr).to_ascii_lowercase();
    let dependent_container = provider_diagnostic.lines().any(|line| {
        line.contains("layer")
            && line.contains("container")
            && ["in use", "used by", "referenced by"]
                .iter()
                .any(|marker| line.contains(marker))
    });
    Some(
        if dependent_container {
            "podman-storage-repair-provider-unable-to-detach-damaged-container"
        } else {
            "podman-storage-repair-provider-refused"
        }
        .into(),
    )
}

fn storage_check_evidence(
    podman_bin: &Path,
    machine: &str,
) -> Result<(PodmanStorageCheckPlan, Vec<String>), String> {
    if !valid_machine_name(machine) {
        return Err("unsafe-requested-machine-name".into());
    }
    let output = run_bounded(
        podman_bin,
        &["--connection", machine, "system", "check", "--quick"],
        PODMAN_STORAGE_CHECK_TIMEOUT,
        "podman-storage-check",
    )?;
    let combined = format!("{}\n{}", output.stdout, output.stderr);
    let ids = damaged_layer_ids(&combined)?;
    let fingerprint = storage_check_fingerprint(&ids);
    let complete = storage_check_complete(output.status_code, &combined, &ids);
    let scope_fingerprint = storage_repair_scope_fingerprint(machine);
    let plan = PodmanStorageCheckPlan {
        schema_version: 1,
        machine: machine.to_string(),
        damaged_layer_records: ids.len() as u64,
        candidate_set_sha256: fingerprint,
        evidence_complete: complete,
        exact_approval_phrase: (complete && !ids.is_empty())
            .then(|| format!("DiskSage Podman machine storage repair 승인 {scope_fingerprint}")),
        issue: (!complete).then(|| "podman-storage-check-evidence-incomplete".into()),
    };
    Ok((plan, ids))
}

/// Capture bounded native Podman storage-check evidence.
///
/// The candidate fingerprint is evidence about the current damaged-layer set. The approval phrase
/// is deliberately bound to the selected machine plus the exact broad native repair command,
/// because `podman system check --repair` cannot be constrained to a caller-supplied layer list.
pub fn plan_podman_storage_repair(
    podman_bin: &Path,
    machine: &str,
) -> Result<PodmanStorageCheckPlan, String> {
    storage_check_evidence(podman_bin, machine).map(|(plan, _)| plan)
}

/// Execute one explicitly machine-scoped native Podman storage repair and retain an auditable
/// receipt even when the mutation command or fresh postcheck cannot be fully observed.
pub fn execute_podman_storage_repair(
    podman_bin: &Path,
    machine: &str,
    confirmation_phrase: &str,
    rationale: &str,
    executed_at_ms: u64,
) -> Result<PodmanStorageRepairExecution, String> {
    if executed_at_ms == 0 || rationale.trim().is_empty() || rationale != rationale.trim() {
        return Err("podman-storage-repair-request-invalid".into());
    }

    let (plan, pre_ids) = storage_check_evidence(podman_bin, machine)?;
    if !plan.evidence_complete || pre_ids.is_empty() {
        return Err("podman-storage-repair-not-required".into());
    }
    if plan.exact_approval_phrase.as_deref() != Some(confirmation_phrase) {
        return Err("podman-storage-repair-confirmation-mismatch".into());
    }

    let output = run_mutation_bounded(
        podman_bin,
        &[
            "--connection",
            machine,
            "system",
            "check",
            "--quick",
            "--repair",
        ],
        PODMAN_STORAGE_CHECK_TIMEOUT,
        "podman-storage-repair",
    )?;

    let (postcheck_complete, repaired_layer_records, remaining_damaged_layer_records) =
        match storage_check_evidence(podman_bin, machine) {
            Ok((postcheck, post_ids)) => {
                let remaining = post_ids.len() as u64;
                let post_set = post_ids.into_iter().collect::<HashSet<_>>();
                let repaired = pre_ids
                    .iter()
                    .filter(|id| !post_set.contains(id.as_str()))
                    .count() as u64;
                (postcheck.evidence_complete, repaired, remaining)
            }
            Err(_) => (false, 0, 0),
        };

    let execution_issue = match output.status_code {
        MUTATION_TIMEOUT_STATUS_CODE => Some("podman-storage-repair-timeout".into()),
        MUTATION_WAIT_STATUS_CODE => Some("podman-storage-repair-wait".into()),
        MUTATION_CAPTURE_STATUS_CODE => Some("podman-storage-repair-output-too-large".into()),
        MUTATION_UTF8_STATUS_CODE => Some("podman-storage-repair-output-not-utf8".into()),
        0 if !postcheck_complete => Some("podman-storage-repair-postcheck-incomplete".into()),
        _ => storage_repair_provider_issue(&output),
    };

    Ok(PodmanStorageRepairExecution {
        schema_version: 1,
        machine: machine.to_string(),
        candidate_set_sha256: plan.candidate_set_sha256,
        command: vec![
            "podman".into(),
            "--connection".into(),
            machine.into(),
            "system".into(),
            "check".into(),
            "--quick".into(),
            "--repair".into(),
        ],
        status_code: output.status_code,
        command_attempted: true,
        execution_issue,
        executed: output.status_code == 0 && postcheck_complete,
        repaired_layer_records,
        remaining_damaged_layer_records,
        postcheck_complete,
        executed_at_ms,
        rationale: rationale.to_string(),
    })
}

fn exact_candidates(output: &str) -> Result<Vec<ExactCandidate>, String> {
    let records: Vec<PodmanImageRecord> =
        serde_json::from_str(output).map_err(|_| "invalid-podman-images-json".to_string())?;
    let mut candidates = Vec::new();
    for mut record in records {
        if record.id.len() != 64
            || !record
                .id
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err("podman-images-invalid-id".to_string());
        }
        if record.containers > 0 {
            continue;
        }
        let mut tags = record.repo_tags.take().unwrap_or_default();
        tags.extend(record.repo_digests.take().unwrap_or_default());
        tags.extend(record.names.take().unwrap_or_default());
        tags.sort();
        tags.dedup();
        candidates.push(ExactCandidate {
            id: record.id,
            tags,
            size_bytes: record.size_bytes,
        });
    }
    candidates.sort_by(|left, right| left.id.cmp(&right.id));
    if candidates.windows(2).any(|pair| pair[0].id == pair[1].id) {
        return Err("podman-images-duplicate-id".to_string());
    }
    Ok(candidates)
}

fn candidate_set_sha256(candidates: &[ExactCandidate]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"disksage.podman-unused-images.v1");
    for candidate in candidates {
        hash_frame(&mut hasher, candidate.id.as_bytes());
        hash_frame(&mut hasher, &candidate.size_bytes.to_be_bytes());
        hash_frame(&mut hasher, &(candidate.tags.len() as u64).to_be_bytes());
        for tag in &candidate.tags {
            hash_frame(&mut hasher, tag.as_bytes());
        }
    }
    lower_hex(&hasher.finalize())
}

fn list_exact_candidates(
    podman_bin: &Path,
    requested_machine: &str,
) -> Result<Vec<ExactCandidate>, String> {
    let output = run_text(
        podman_bin,
        &[
            "--connection",
            requested_machine,
            "images",
            "--all",
            "--format",
            "json",
        ],
        PODMAN_PRUNE_TIMEOUT,
        "podman-prune-images",
    )?;
    exact_candidates(&output)
}

/// Deletes only the immutable image IDs represented by the reviewed candidate fingerprint.
///
/// The public mutation boundary re-lists the candidate set immediately before deletion, rejects
/// any tag/reference/size drift, and uses `image rm --no-prune` without `--force`. This prevents a
/// broad `image prune` from deleting newly dangling images that were never part of the approval.
pub fn prune_dangling_images(
    podman_bin: &Path,
    requested_machine: &str,
    confirmation_phrase: &str,
    rationale: &str,
    executed_at_ms: u64,
) -> Result<PodmanDanglingImagePruneExecution, String> {
    if executed_at_ms == 0 {
        return Err("podman-prune-time-invalid".into());
    }
    if rationale.trim().is_empty()
        || rationale != rationale.trim()
        || rationale.chars().count() > 1_000
        || rationale.chars().any(char::is_control)
    {
        return Err("podman-prune-rationale-invalid".into());
    }
    if !valid_machine_name(requested_machine) {
        return Err("unsafe-requested-machine-name".into());
    }
    let inspect_output = run_text(
        podman_bin,
        &["machine", "inspect", requested_machine],
        PODMAN_PRUNE_TIMEOUT,
        "podman-prune-machine-inspect",
    )?;
    let inspect: Vec<MachineInspectRecord> = serde_json::from_str(&inspect_output)
        .map_err(|_| "invalid-machine-inspect-json".to_string())?;
    if inspect.len() != 1
        || inspect[0].name != requested_machine
        || !inspect[0].state.eq_ignore_ascii_case("running")
    {
        return Err("podman-prune-machine-not-running".into());
    }

    let approved_candidates = list_exact_candidates(podman_bin, requested_machine)?;
    if approved_candidates.is_empty()
        || approved_candidates.len() > MAX_EXACT_DELETE_IDS
        || approved_candidates
            .iter()
            .any(|candidate| !candidate.tags.is_empty())
    {
        return Err("podman-prune-tagged-empty-or-oversized-candidate-set".into());
    }
    let approved_sha = candidate_set_sha256(&approved_candidates);
    let expected_phrase = format!("DiskSage Podman dangling image prune 승인 {approved_sha}");
    if confirmation_phrase != expected_phrase {
        return Err("podman-prune-confirmation-mismatch".into());
    }

    let revalidated_candidates = list_exact_candidates(podman_bin, requested_machine)?;
    if revalidated_candidates != approved_candidates
        || candidate_set_sha256(&revalidated_candidates) != approved_sha
        || revalidated_candidates
            .iter()
            .any(|candidate| !candidate.tags.is_empty())
    {
        return Err("podman-prune-candidate-set-changed".into());
    }

    let before_available_bytes = std::env::current_dir()
        .ok()
        .and_then(|path| crate::volume_pressure::snapshot_volume(&path, executed_at_ms).ok())
        .map(|snapshot| snapshot.available_bytes);

    let mut remove_args = vec![
        "--connection".to_string(),
        requested_machine.to_string(),
        "image".to_string(),
        "rm".to_string(),
        "--no-prune".to_string(),
    ];
    remove_args.extend(
        revalidated_candidates
            .iter()
            .map(|candidate| candidate.id.clone()),
    );
    let remove_arg_refs = remove_args.iter().map(String::as_str).collect::<Vec<_>>();
    let output = run_bounded(
        podman_bin,
        &remove_arg_refs,
        PODMAN_PRUNE_TIMEOUT,
        "podman-prune-approved-images",
    )?;

    let after_observed_at_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(executed_at_ms);
    let after_available_bytes = std::env::current_dir()
        .ok()
        .and_then(|path| crate::volume_pressure::snapshot_volume(&path, after_observed_at_ms).ok())
        .map(|snapshot| snapshot.available_bytes);
    let observed_available_gain_bytes = before_available_bytes
        .zip(after_available_bytes)
        .and_then(|(before, after)| after.checked_sub(before));

    Ok(PodmanDanglingImagePruneExecution {
        schema_version: PODMAN_PRUNE_SCHEMA_VERSION,
        candidate_set_sha256: approved_sha,
        command: vec![
            "podman".into(),
            "--connection".into(),
            requested_machine.into(),
            "image".into(),
            "rm".into(),
            "--no-prune".into(),
            "<approved-image-ids>".into(),
        ],
        status_code: output.status_code,
        stdout: String::new(),
        stderr: String::new(),
        output_truncated: false,
        executed: output.status_code == 0,
        executed_at_ms,
        before_available_bytes,
        after_available_bytes,
        observed_available_gain_bytes,
        rationale: rationale.to_string(),
    })
}

#[cfg(all(test, unix))]
mod mutation_runner_tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn provider_issue_requires_one_coherent_dependency_diagnostic_line() {
        let separate_lines = BoundedOutput {
            status_code: 125,
            stdout: "damaged layer abc\ncontainer inventory follows".into(),
            stderr: "resource is in use".into(),
        };
        assert_eq!(
            storage_repair_provider_issue(&separate_lines).as_deref(),
            Some("podman-storage-repair-provider-refused")
        );
        let coherent = BoundedOutput {
            status_code: 125,
            stdout: "layer abc is used by container def".into(),
            stderr: String::new(),
        };
        assert_eq!(
            storage_repair_provider_issue(&coherent).as_deref(),
            Some("podman-storage-repair-provider-unable-to-detach-damaged-container")
        );
        let untrusted = BoundedOutput {
            status_code: 1,
            stdout: "layer abc is used by container def".into(),
            stderr: String::new(),
        };
        assert_eq!(
            storage_repair_provider_issue(&untrusted).as_deref(),
            Some("podman-storage-repair-provider-exit-status-unexpected")
        );
    }

    #[test]
    fn timed_out_readonly_command_terminates_descendants_holding_output_pipes() {
        let temp = tempfile::tempdir().expect("temporary runtime directory");
        let fake = temp.path().join("readonly");
        fs::write(&fake, "#!/bin/sh\nsleep 5 &\nwait\n").expect("write read-only fixture");
        let mut permissions = fs::metadata(&fake).unwrap().permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&fake, permissions).unwrap();

        let started = Instant::now();
        let error = run_bounded(
            &fake,
            &[],
            Duration::from_millis(50),
            "readonly-timeout-fixture",
        )
        .expect_err("the process-group timeout must fail closed");

        assert_eq!(error, "readonly-timeout-fixture-timeout");
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn completed_readonly_command_terminates_descendants_holding_output_pipes() {
        let temp = tempfile::tempdir().expect("temporary runtime directory");
        let fake = temp.path().join("readonly-completed");
        fs::write(&fake, "#!/bin/sh\nsleep 5 &\nexit 0\n").expect("write read-only fixture");
        let mut permissions = fs::metadata(&fake).unwrap().permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&fake, permissions).unwrap();

        let started = Instant::now();
        let output = run_bounded(
            &fake,
            &[],
            Duration::from_secs(1),
            "readonly-completed-fixture",
        )
        .expect("direct completion must not hang on inherited pipes");

        assert_eq!(output.status_code, 0);
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn timed_out_spawned_mutation_returns_failure_output_instead_of_erasing_execution() {
        let temp = tempfile::tempdir().expect("temporary runtime directory");
        let fake = temp.path().join("mutation");
        fs::write(
            &fake,
            r#"#!/bin/sh
root="$(dirname "$0")"
touch "$root/mutation-ran"
sleep 5 &
wait
"#,
        )
        .expect("write mutation fixture");
        let mut permissions = fs::metadata(&fake).unwrap().permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&fake, permissions).unwrap();

        let started = Instant::now();
        let output = run_mutation_bounded(
            &fake,
            &[],
            Duration::from_millis(50),
            "mutation-timeout-fixture",
        )
        .expect("spawned timeout must remain representable as output evidence");

        assert_eq!(output.status_code, MUTATION_TIMEOUT_STATUS_CODE);
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn completed_mutation_terminates_descendants_holding_output_pipes() {
        let temp = tempfile::tempdir().expect("temporary runtime directory");
        let fake = temp.path().join("mutation-completed");
        fs::write(&fake, "#!/bin/sh\nsleep 5 &\nexit 0\n").expect("write mutation fixture");
        let mut permissions = fs::metadata(&fake).unwrap().permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&fake, permissions).unwrap();

        let started = Instant::now();
        let output = run_mutation_bounded(
            &fake,
            &[],
            Duration::from_secs(1),
            "mutation-completed-fixture",
        )
        .expect("direct completion must not hang on inherited pipes");

        assert_eq!(output.status_code, 0);
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn shared_storage_repair_parser_owns_candidate_identity() {
        let first = "a".repeat(64);
        let second = "b".repeat(64);
        let ids = damaged_layer_ids(&format!(
            "Damaged layer {second}:\nDamaged layer {first}:\nDamaged layer {second}:"
        ))
        .unwrap();
        assert_eq!(ids, vec![first, second]);
        assert_eq!(storage_check_fingerprint(&ids).len(), 64);
    }
}
