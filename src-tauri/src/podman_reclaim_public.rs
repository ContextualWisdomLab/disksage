use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[path = "podman_reclaim.rs"]
mod implementation;

pub use implementation::{
    probe_podman_reclaim, GuestFilesystemEvidence, PodmanDanglingImagePruneExecution,
    PodmanMachineEvidence, PodmanReclaimAssessment, PodmanReclaimPlan, PodmanRecommendedAction,
    PodmanRecommendedActionKind, PodmanStoreEvidence, PodmanSystemDfCategoryEvidence,
    PodmanSystemDfEvidence, PodmanUnusedImageEvidence, RawImageEvidence, DEFAULT_PODMAN_MACHINE,
    DEFAULT_PROBE_TIMEOUT, PODMAN_RECLAIM_SCHEMA_KIND,
};

const MAX_CAPTURE_BYTES: usize = 1_048_576;
const MAX_EXACT_DELETE_IDS: usize = 256;
const PODMAN_PRUNE_TIMEOUT: Duration = Duration::from_secs(30);
const PODMAN_PRUNE_SCHEMA_VERSION: u32 = 1;

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
    let mut child = Command::new(executable)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| format!("{label}-spawn"))?;
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
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(format!("{label}-timeout"));
            }
            Ok(None) => thread::sleep(Duration::from_millis(25)),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(format!("{label}-wait"));
            }
        }
    };
    let stdout = stdout_reader
        .join()
        .map_err(|_| format!("{label}-stdout-reader-panicked"))?
        .map_err(|_| format!("{label}-output-too-large"))?;
    let _stderr = stderr_reader
        .join()
        .map_err(|_| format!("{label}-stderr-reader-panicked"))?
        .map_err(|_| format!("{label}-output-too-large"))?;
    Ok(BoundedOutput {
        status_code: status.code().unwrap_or(-1),
        stdout: String::from_utf8(stdout).map_err(|_| format!("{label}-stdout-not-utf8"))?,
    })
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

fn list_exact_candidates(podman_bin: &Path, requested_machine: &str) -> Result<Vec<ExactCandidate>, String> {
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
        || approved_candidates.iter().any(|candidate| !candidate.tags.is_empty())
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
        || revalidated_candidates.iter().any(|candidate| !candidate.tags.is_empty())
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
    remove_args.extend(revalidated_candidates.iter().map(|candidate| candidate.id.clone()));
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
