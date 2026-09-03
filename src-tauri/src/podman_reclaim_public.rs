use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Read;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

#[path = "podman_reclaim.rs"]
mod implementation;
#[cfg(windows)]
#[path = "windows_process_tree.rs"]
mod windows_process_tree;

pub use implementation::{
    probe_podman_reclaim, GuestFilesystemEvidence, PodmanDanglingImagePruneExecution,
    PodmanMachineEvidence, PodmanReclaimAssessment, PodmanReclaimPlan, PodmanRecommendedAction,
    PodmanRecommendedActionKind, PodmanStoreEvidence, PodmanSystemDfCategoryEvidence,
    PodmanSystemDfEvidence, PodmanUnusedImageEvidence, RawImageEvidence, DEFAULT_PODMAN_MACHINE,
    DEFAULT_PROBE_TIMEOUT, PODMAN_RECLAIM_SCHEMA_KIND,
};

const MAX_CAPTURE_BYTES: usize = 1_048_576;
pub(super) const MAX_EXACT_DELETE_IDS: usize = 256;
const PODMAN_PRUNE_TIMEOUT: Duration = Duration::from_secs(30);
const PODMAN_PRUNE_SCHEMA_VERSION: u32 = 1;
const OUTPUT_DRAIN_TIMEOUT: Duration = Duration::from_secs(1);

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

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct PodmanVolumeRecord {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Mountpoint")]
    mountpoint: String,
    #[serde(rename = "MountCount")]
    mount_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PodmanEmptyVolumePlan {
    pub schema_version: u32,
    pub ontology_class: &'static str,
    pub candidate_count: usize,
    pub candidate_set_sha256: String,
    pub exact_approval_phrase: Option<String>,
    pub evidence_complete: bool,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PodmanEmptyVolumeExecution {
    pub schema_version: u32,
    pub ontology_class: &'static str,
    pub candidate_count: usize,
    pub candidate_set_sha256: String,
    pub executed: bool,
    pub status_code: i32,
    pub rationale: String,
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

fn spawn_bounded_reader<R: Read + Send + 'static>(
    reader: R,
) -> Receiver<std::io::Result<Vec<u8>>> {
    let (sender, receiver) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let _ = sender.send(drain_bounded(reader));
    });
    receiver
}

fn receive_bounded_reader(
    receiver: Receiver<std::io::Result<Vec<u8>>>,
    label: &str,
    stream: &str,
) -> Result<Vec<u8>, String> {
    match receiver.recv_timeout(OUTPUT_DRAIN_TIMEOUT) {
        Ok(Ok(bytes)) => Ok(bytes),
        Ok(Err(_)) => Err(format!("{label}-output-too-large")),
        Err(RecvTimeoutError::Timeout) => Err(format!("{label}-{stream}-reader-timeout")),
        Err(RecvTimeoutError::Disconnected) => Err(format!("{label}-{stream}-reader-panicked")),
    }
}

#[cfg(unix)]
fn kill_process_group(process_id: u32) {
    let Ok(process_id) = i32::try_from(process_id) else {
        return;
    };
    // SAFETY: run_bounded starts the child in a private process group, so the negative PID targets
    // only this Podman invocation and descendants that inherited its group.
    let _ = unsafe { libc::kill(-process_id, libc::SIGKILL) };
}

#[cfg(unix)]
fn terminate_command_tree(child: &mut Child) {
    kill_process_group(child.id());
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(all(not(unix), not(windows)))]
fn terminate_command_tree(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(windows)]
fn terminate_windows_command_tree(
    child: &mut Child,
    process_tree: &mut Option<windows_process_tree::ProcessTreeGuard>,
) {
    process_tree.take();
    let _ = child.kill();
    let _ = child.wait();
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
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    command.process_group(0);
    #[cfg(windows)]
    windows_process_tree::ProcessTreeGuard::prepare_suspended(&mut command);
    let mut child = command.spawn().map_err(|_| format!("{label}-spawn"))?;
    #[cfg(unix)]
    let process_id = child.id();
    #[cfg(windows)]
    let mut process_tree = match windows_process_tree::ProcessTreeGuard::attach_and_resume(&child) {
        Ok(guard) => Some(guard),
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("{label}-process-tree-control-unavailable"));
        }
    };
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| format!("{label}-stdout-pipe-unavailable"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| format!("{label}-stderr-pipe-unavailable"))?;
    let stdout_reader = spawn_bounded_reader(stdout);
    let stderr_reader = spawn_bounded_reader(stderr);
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                #[cfg(unix)]
                kill_process_group(process_id);
                #[cfg(windows)]
                {
                    process_tree.take();
                }
                break status;
            }
            Ok(None) if started.elapsed() >= timeout => {
                #[cfg(windows)]
                terminate_windows_command_tree(&mut child, &mut process_tree);
                #[cfg(not(windows))]
                terminate_command_tree(&mut child);
                return Err(format!("{label}-timeout"));
            }
            Ok(None) => thread::sleep(Duration::from_millis(25)),
            Err(_) => {
                #[cfg(windows)]
                terminate_windows_command_tree(&mut child, &mut process_tree);
                #[cfg(not(windows))]
                terminate_command_tree(&mut child);
                return Err(format!("{label}-wait"));
            }
        }
    };
    let stdout = receive_bounded_reader(stdout_reader, label, "stdout")?;
    let _stderr = receive_bounded_reader(stderr_reader, label, "stderr")?;
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
    let mut images: BTreeMap<String, (Vec<String>, u64, bool)> = BTreeMap::new();
    for mut record in records {
        if record.id.len() != 64
            || !record
                .id
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err("podman-images-invalid-id".to_string());
        }
        let mut tags = record.repo_tags.take().unwrap_or_default();
        tags.extend(record.repo_digests.take().unwrap_or_default());
        tags.extend(record.names.take().unwrap_or_default());
        tags.sort();
        tags.dedup();
        let image = images
            .entry(record.id)
            .or_insert_with(|| (Vec::new(), record.size_bytes, false));
        if image.1 != record.size_bytes {
            return Err("podman-images-duplicate-size-mismatch".to_string());
        }
        image.0.extend(tags);
        image.0.sort();
        image.0.dedup();
        image.2 |= record.containers > 0;
    }
    Ok(images
        .into_iter()
        .filter(|(_, image)| !image.2)
        .map(|(id, (tags, size_bytes, _))| ExactCandidate {
            id,
            tags,
            size_bytes,
        })
        .collect())
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

fn valid_volume_component(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 255
        && !value.starts_with('-')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn valid_volume_mountpoint(value: &str) -> bool {
    value.starts_with("/var/home/core/.local/share/containers/storage/volumes/")
        && value.ends_with("/_data")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'_' | b'.'))
}

fn volume_set_sha256(volumes: &[PodmanVolumeRecord]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"disksage.podman-empty-dangling-volumes.v1");
    for volume in volumes {
        hash_frame(&mut hasher, volume.name.as_bytes());
        hash_frame(&mut hasher, volume.mountpoint.as_bytes());
    }
    lower_hex(&hasher.finalize())
}

fn list_dangling_volumes(
    podman_bin: &Path,
    requested_machine: &str,
) -> Result<Vec<PodmanVolumeRecord>, String> {
    let output = run_text(
        podman_bin,
        &[
            "--connection",
            requested_machine,
            "volume",
            "ls",
            "--filter",
            "dangling=true",
            "--format",
            "json",
        ],
        PODMAN_PRUNE_TIMEOUT,
        "podman-dangling-volumes",
    )?;
    let mut volumes: Vec<PodmanVolumeRecord> =
        serde_json::from_str(&output).map_err(|_| "invalid-podman-volumes-json".to_string())?;
    if volumes.iter().any(|volume| {
        !valid_volume_component(&volume.name)
            || !valid_volume_mountpoint(&volume.mountpoint)
            || volume.mountpoint
                != format!(
                    "/var/home/core/.local/share/containers/storage/volumes/{}/_data",
                    volume.name
                )
            || volume.mount_count != 0
    }) {
        return Err("podman-volume-evidence-invalid".into());
    }
    volumes.sort_by(|left, right| left.name.cmp(&right.name));
    volumes.dedup_by(|left, right| left.name == right.name && left.mountpoint == right.mountpoint);
    Ok(volumes)
}

fn collect_empty_volume_probe_results(
    results: Vec<Result<Option<PodmanVolumeRecord>, String>>,
) -> Result<Vec<PodmanVolumeRecord>, String> {
    let mut empty = Vec::new();
    for result in results {
        if let Some(volume) = result? {
            empty.push(volume);
        }
    }
    empty.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(empty)
}

fn empty_dangling_volumes(
    podman_bin: &Path,
    requested_machine: &str,
) -> Result<Vec<PodmanVolumeRecord>, String> {
    let volumes = list_dangling_volumes(podman_bin, requested_machine)?;
    let executable = podman_bin.to_path_buf();
    let machine = requested_machine.to_string();
    let probes = crate::stale_git_clone::bounded_parallel_map(
        volumes
            .into_iter()
            .map(|volume| std::path::PathBuf::from(volume.name))
            .collect(),
        8,
        move |name| {
            let name = name.to_string_lossy().into_owned();
            let mountpoint =
                format!("/var/home/core/.local/share/containers/storage/volumes/{name}/_data");
            let command = format!("sudo find {mountpoint} -mindepth 1 -print -quit");
            let output = run_text(
                &executable,
                &["machine", "ssh", &machine, &command],
                PODMAN_PRUNE_TIMEOUT,
                "podman-volume-empty-probe",
            )?;
            Ok(output.trim().is_empty().then(|| PodmanVolumeRecord {
                name,
                mountpoint,
                mount_count: 0,
            }))
        },
    );
    collect_empty_volume_probe_results(probes)
}

pub fn plan_empty_dangling_volumes(
    podman_bin: &Path,
    requested_machine: &str,
) -> Result<PodmanEmptyVolumePlan, String> {
    if !valid_machine_name(requested_machine) {
        return Err("unsafe-requested-machine-name".into());
    }
    let candidates = empty_dangling_volumes(podman_bin, requested_machine)?;
    let fingerprint = volume_set_sha256(&candidates);
    Ok(PodmanEmptyVolumePlan {
        schema_version: PODMAN_PRUNE_SCHEMA_VERSION,
        ontology_class: "https://disksage.app/ontology#ContainerVolume",
        candidate_count: candidates.len(),
        candidate_set_sha256: fingerprint,
        exact_approval_phrase: None,
        evidence_complete: true,
        issues: vec!["podman-empty-volume-atomic-removal-unavailable".into()],
    })
}

pub fn prune_empty_dangling_volumes(
    podman_bin: &Path,
    requested_machine: &str,
    confirmation_phrase: &str,
    rationale: &str,
) -> Result<PodmanEmptyVolumeExecution, String> {
    if rationale.trim().is_empty() || rationale != rationale.trim() {
        return Err("podman-volume-prune-rationale-invalid".into());
    }
    if !valid_machine_name(requested_machine) {
        return Err("unsafe-requested-machine-name".into());
    }
    let _ = (podman_bin, confirmation_phrase);
    Err("podman-empty-volume-atomic-removal-unavailable".into())
}

fn dangling_candidates(candidates: &[ExactCandidate]) -> Vec<&ExactCandidate> {
    candidates
        .iter()
        .filter(|candidate| candidate.tags.is_empty())
        .collect()
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
    let approved_dangling = dangling_candidates(&approved_candidates);
    if approved_dangling.is_empty() || approved_dangling.len() > MAX_EXACT_DELETE_IDS {
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
    {
        return Err("podman-prune-candidate-set-changed".into());
    }
    let revalidated_dangling = dangling_candidates(&revalidated_candidates);

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
        revalidated_dangling
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

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn bounded_timeout_kills_descendants_holding_output_pipes() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().expect("fixture directory should be creatable");
        let script = root.path().join("podman-descendant-fixture");
        fs::write(&script, "#!/bin/sh\n(sleep 30) &\nsleep 30\n")
            .expect("fixture script should be writable");
        let mut permissions = fs::metadata(&script)
            .expect("fixture metadata should be readable")
            .permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&script, permissions).expect("fixture script should be executable");

        let started = Instant::now();
        assert_eq!(
            run_bounded(&script, &[], Duration::from_millis(100), "podman-descendant-fixture")
                .unwrap_err(),
            "podman-descendant-fixture-timeout"
        );
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "timeout must include descendant output-pipe cleanup"
        );
    }

    #[test]
    fn exact_candidates_merge_alias_records_and_keep_referenced_images() {
        let id = "b".repeat(64);
        let output = format!(
            r#"[{{"Id":"{id}","RepoTags":["one:latest"],"Containers":0,"Size":200}},{{"Id":"{id}","RepoTags":["two:latest"],"Containers":1,"Size":200}}]"#
        );
        assert!(exact_candidates(&output).unwrap().is_empty());

        let mismatch = format!(
            r#"[{{"Id":"{id}","RepoTags":[],"Containers":0,"Size":200}},{{"Id":"{id}","RepoTags":[],"Containers":0,"Size":201}}]"#
        );
        assert_eq!(
            exact_candidates(&mismatch).unwrap_err(),
            "podman-images-duplicate-size-mismatch"
        );
    }

    #[test]
    fn dangling_candidates_exclude_tagged_unused_images() {
        let candidates = vec![
            ExactCandidate {
                id: "a".repeat(64),
                tags: Vec::new(),
                size_bytes: 1,
            },
            ExactCandidate {
                id: "b".repeat(64),
                tags: vec!["keep:latest".into()],
                size_bytes: 2,
            },
        ];
        let dangling = dangling_candidates(&candidates);
        assert_eq!(dangling.len(), 1);
        assert_eq!(dangling[0].id, "a".repeat(64));
    }

    #[test]
    fn volume_identity_binds_name_to_expected_store_mountpoint() {
        let volume = PodmanVolumeRecord {
            name: "project_data".into(),
            mountpoint: "/var/home/core/.local/share/containers/storage/volumes/project_data/_data"
                .into(),
            mount_count: 0,
        };
        assert!(valid_volume_component(&volume.name));
        assert!(valid_volume_mountpoint(&volume.mountpoint));
        assert_eq!(
            volume_set_sha256(&[volume.clone()]),
            volume_set_sha256(&[volume])
        );
        assert!(!valid_volume_component("../escape"));
        assert!(!valid_volume_mountpoint("/tmp/project_data/_data"));
    }

    #[test]
    fn failed_volume_probe_makes_evidence_incomplete() {
        assert_eq!(
            collect_empty_volume_probe_results(vec![Err("podman-volume-empty-probe-failed".into())])
                .unwrap_err(),
            "podman-volume-empty-probe-failed"
        );
    }
}
