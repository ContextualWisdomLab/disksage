use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

pub const PODMAN_RECLAIM_SCHEMA_KIND: &str = "disksage.podman-reclaim-plan";
pub const DEFAULT_PODMAN_MACHINE: &str = "podman-machine-default";
pub const DEFAULT_PROBE_TIMEOUT: Duration = Duration::from_secs(12);
const MAX_CAPTURE_BYTES: usize = 1_048_576;
const GIB: u64 = 1_073_741_824;
const CRITICAL_GUEST_AVAILABLE_BYTES: u64 = 2 * GIB;
const MATERIAL_ALLOCATION_GAP_BYTES: u64 = 512 * 1_048_576;
const PODMAN_PRUNE_SCHEMA_VERSION: u32 = 1;
const PODMAN_PRUNE_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PodmanMachineEvidence {
    pub name: String,
    pub state: String,
    pub configured_disk_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RawImageEvidence {
    pub path: String,
    pub logical_bytes: u64,
    /// st_blocks * 512 on Unix. This is observed host allocation, not reclaim proof.
    pub allocated_bytes: Option<u64>,
    /// Stable hash of the host file identity (device and inode on Unix).
    pub identity_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GuestFilesystemEvidence {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PodmanStoreEvidence {
    pub graph_root: String,
    pub graph_root_allocated_bytes: u64,
    pub graph_root_used_bytes: u64,
    pub images: u64,
    pub containers_total: u64,
    pub containers_running: u64,
    pub containers_stopped: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PodmanSystemDfCategoryEvidence {
    pub total: u64,
    pub active: u64,
    /// Podman-reported logical/shared size. This is not host physical allocation.
    pub size_bytes: u64,
    /// Podman-reported candidate bytes. This is not host physical reclaim proof.
    pub reclaimable_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PodmanSystemDfEvidence {
    pub images: PodmanSystemDfCategoryEvidence,
    pub containers: PodmanSystemDfCategoryEvidence,
    pub local_volumes: PodmanSystemDfCategoryEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PodmanUnusedImageEvidence {
    pub total_records: u64,
    pub referenced_records: u64,
    pub unused_records: u64,
    pub unused_untagged_records: u64,
    pub unused_tagged_records: u64,
    /// Sum of image record sizes. Shared layers mean this is not additive physical reclaim proof.
    pub candidate_record_size_sum: u64,
    /// Binds exact unused image IDs, sorted tags, and sizes without exposing IDs in this report.
    pub candidate_set_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PodmanRecommendedActionKind {
    RestoreGuestHeadroom,
    InvestigateApi,
    ReviewGuestTrim,
    ReviewStoppedContainers,
    ReviewUnusedImages,
    ReviewUnusedVolumes,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PodmanRecommendedAction {
    pub kind: PodmanRecommendedActionKind,
    pub requires_human_approval: bool,
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PodmanReclaimAssessment {
    /// Intentionally unknown until a before/after host free-space observation proves reclaim.
    pub physically_reclaimable_bytes: Option<u64>,
    /// Sum reported by `podman system df`; shared layers and the VM raw image make this non-physical.
    pub podman_reported_reclaimable_bytes: Option<u64>,
    /// Observed allocation gap only; filesystem metadata and sparse extents make it non-proof.
    pub raw_allocated_minus_guest_used_bytes: Option<u64>,
    pub status: String,
    pub reason_codes: Vec<String>,
    pub recommended_actions: Vec<PodmanRecommendedAction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PodmanReclaimPlan {
    pub schema_kind: &'static str,
    pub schema_version: u32,
    pub platform: &'static str,
    pub evidence_complete: bool,
    pub elapsed_ms: u64,
    pub machine: Option<PodmanMachineEvidence>,
    pub raw_image: Option<RawImageEvidence>,
    pub guest_filesystem: Option<GuestFilesystemEvidence>,
    pub store: Option<PodmanStoreEvidence>,
    pub system_df: Option<PodmanSystemDfEvidence>,
    pub unused_images: Option<PodmanUnusedImageEvidence>,
    /// Present only when the fresh evidence contains dangling (untagged, unreferenced) images.
    pub dangling_prune_approval_phrase: Option<String>,
    pub assessment: PodmanReclaimAssessment,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PodmanDanglingImagePruneExecution {
    pub schema_version: u32,
    pub candidate_set_sha256: String,
    pub command: Vec<String>,
    pub status_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub output_truncated: bool,
    pub executed: bool,
    pub executed_at_ms: u64,
    pub before_available_bytes: Option<u64>,
    pub after_available_bytes: Option<u64>,
    /// Only a positive before/after available-space delta is reported; it is still attribution-weak.
    pub observed_available_gain_bytes: Option<u64>,
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PodmanStorageCheckPlan {
    pub schema_version: u32,
    pub machine: String,
    pub damaged_layer_records: u64,
    pub candidate_set_sha256: String,
    pub evidence_complete: bool,
    pub exact_approval_phrase: Option<String>,
    pub issue: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PodmanStorageRepairExecution {
    pub schema_version: u32,
    pub machine: String,
    pub candidate_set_sha256: String,
    pub command: Vec<String>,
    pub status_code: i32,
    pub command_attempted: bool,
    pub execution_issue: Option<String>,
    pub executed: bool,
    pub repaired_layer_records: u64,
    pub remaining_damaged_layer_records: u64,
    pub postcheck_complete: bool,
    pub executed_at_ms: u64,
    pub rationale: String,
}

#[derive(Debug, Deserialize)]
struct PathField {
    #[serde(rename = "Path")]
    path: String,
}

#[derive(Debug, Deserialize)]
struct MachineResources {
    #[serde(rename = "DiskSize")]
    disk_size_gib: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct MachineInspectRecord {
    #[serde(rename = "ConfigDir")]
    config_dir: PathField,
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "State")]
    state: String,
    #[serde(rename = "Resources")]
    resources: MachineResources,
}

#[derive(Debug, Deserialize)]
struct MachineConfig {
    #[serde(rename = "ImagePath")]
    image_path: PathField,
}

#[derive(Debug, Deserialize)]
struct PodmanSystemDfRecord {
    #[serde(rename = "Type")]
    kind: String,
    #[serde(rename = "Total")]
    total: u64,
    #[serde(rename = "Active")]
    active: u64,
    #[serde(rename = "RawSize")]
    size_bytes: u64,
    #[serde(rename = "RawReclaimable")]
    reclaimable_bytes: u64,
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
struct UnusedImageCandidate {
    id: String,
    tags: Vec<String>,
    size_bytes: u64,
}

fn prune_approval_phrase(candidate_set_sha256: &str) -> String {
    format!("DiskSage Podman dangling image prune 승인 {candidate_set_sha256}")
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

fn parse_machine_inspect(output: &str) -> Result<MachineInspectRecord, String> {
    let records: Vec<MachineInspectRecord> = serde_json::from_str(output)
        .map_err(|error| format!("invalid-machine-inspect-json:{error}"))?;
    if records.len() != 1 {
        return Err(format!("unexpected-machine-count:{}", records.len()));
    }
    let record = records.into_iter().next().unwrap();
    if !valid_machine_name(&record.name) {
        return Err("unsafe-machine-name".to_string());
    }
    if !Path::new(&record.config_dir.path).is_absolute() {
        return Err("machine-config-dir-not-absolute".to_string());
    }
    Ok(record)
}

fn parse_machine_config(output: &str) -> Result<PathBuf, String> {
    let config: MachineConfig = serde_json::from_str(output)
        .map_err(|error| format!("invalid-machine-config-json:{error}"))?;
    let path = PathBuf::from(config.image_path.path);
    if !path.is_absolute() {
        return Err("raw-image-path-not-absolute".to_string());
    }
    Ok(path)
}

fn parse_guest_df(output: &str) -> Result<GuestFilesystemEvidence, String> {
    let line = output
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .ok_or_else(|| "guest-df-empty".to_string())?;
    let values = line
        .split_whitespace()
        .map(|value| {
            value
                .parse::<u64>()
                .map_err(|_| "guest-df-invalid".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    if values.len() != 3 {
        return Err("guest-df-field-count".to_string());
    }
    let evidence = GuestFilesystemEvidence {
        total_bytes: values[0],
        used_bytes: values[1],
        available_bytes: values[2],
    };
    if evidence.used_bytes > evidence.total_bytes
        || evidence.available_bytes > evidence.total_bytes
        || evidence.used_bytes.saturating_add(evidence.available_bytes) > evidence.total_bytes
    {
        return Err("guest-df-inconsistent".to_string());
    }
    Ok(evidence)
}

fn json_u64(value: &Value, path: &[&str]) -> Result<u64, String> {
    let mut cursor = value;
    for key in path {
        cursor = cursor
            .get(*key)
            .ok_or_else(|| format!("podman-info-field-missing:{}", path.join(".")))?;
    }
    cursor
        .as_u64()
        .ok_or_else(|| format!("podman-info-field-invalid:{}", path.join(".")))
}

fn parse_podman_info(output: &str) -> Result<PodmanStoreEvidence, String> {
    let value: Value = serde_json::from_str(output)
        .map_err(|error| format!("invalid-podman-info-json:{error}"))?;
    let store = value
        .get("store")
        .ok_or_else(|| "podman-info-field-missing:store".to_string())?;
    let graph_root = store
        .get("graphRoot")
        .and_then(Value::as_str)
        .ok_or_else(|| "podman-info-field-invalid:store.graphRoot".to_string())?
        .to_string();
    Ok(PodmanStoreEvidence {
        graph_root,
        graph_root_allocated_bytes: json_u64(&value, &["store", "graphRootAllocated"])?,
        graph_root_used_bytes: json_u64(&value, &["store", "graphRootUsed"])?,
        images: json_u64(&value, &["store", "imageStore", "number"])?,
        containers_total: json_u64(&value, &["store", "containerStore", "number"])?,
        containers_running: json_u64(&value, &["store", "containerStore", "running"])?,
        containers_stopped: json_u64(&value, &["store", "containerStore", "stopped"])?,
    })
}

fn parse_podman_system_df(output: &str) -> Result<PodmanSystemDfEvidence, String> {
    let records: Vec<PodmanSystemDfRecord> = serde_json::from_str(output)
        .map_err(|error| format!("invalid-podman-system-df-json:{error}"))?;
    let mut images = None;
    let mut containers = None;
    let mut local_volumes = None;
    for record in records {
        if record.active > record.total || record.reclaimable_bytes > record.size_bytes {
            return Err("podman-system-df-inconsistent".to_string());
        }
        let category = PodmanSystemDfCategoryEvidence {
            total: record.total,
            active: record.active,
            size_bytes: record.size_bytes,
            reclaimable_bytes: record.reclaimable_bytes,
        };
        let slot = match record.kind.as_str() {
            "Images" => &mut images,
            "Containers" => &mut containers,
            "Local Volumes" => &mut local_volumes,
            value => return Err(format!("podman-system-df-unknown-type:{value}")),
        };
        if slot.replace(category).is_some() {
            return Err("podman-system-df-duplicate-type".to_string());
        }
    }
    Ok(PodmanSystemDfEvidence {
        images: images.ok_or_else(|| "podman-system-df-missing-images".to_string())?,
        containers: containers.ok_or_else(|| "podman-system-df-missing-containers".to_string())?,
        local_volumes: local_volumes
            .ok_or_else(|| "podman-system-df-missing-local-volumes".to_string())?,
    })
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

fn parse_unused_image_candidates(
    output: &str,
) -> Result<(u64, u64, Vec<UnusedImageCandidate>), String> {
    let records: Vec<PodmanImageRecord> = serde_json::from_str(output)
        .map_err(|error| format!("invalid-podman-images-json:{error}"))?;
    let total_records =
        u64::try_from(records.len()).map_err(|_| "podman-images-count-overflow".to_string())?;
    let mut referenced_records = 0u64;
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
        let mut tags = record.repo_tags.take().unwrap_or_default();
        tags.extend(record.repo_digests.take().unwrap_or_default());
        tags.extend(record.names.take().unwrap_or_default());
        tags.sort();
        tags.dedup();
        if record.containers > 0 {
            referenced_records = referenced_records
                .checked_add(1)
                .ok_or_else(|| "podman-images-count-overflow".to_string())?;
        } else {
            candidates.push(UnusedImageCandidate {
                id: record.id,
                tags,
                size_bytes: record.size_bytes,
            });
        }
    }
    candidates.sort_by(|left, right| left.id.cmp(&right.id));
    if candidates.windows(2).any(|pair| pair[0].id == pair[1].id) {
        return Err("podman-images-duplicate-id".to_string());
    }
    Ok((total_records, referenced_records, candidates))
}

fn summarize_unused_image_candidates(
    total_records: u64,
    referenced_records: u64,
    candidates: &[UnusedImageCandidate],
) -> Result<PodmanUnusedImageEvidence, String> {
    let unused_records =
        u64::try_from(candidates.len()).map_err(|_| "podman-images-count-overflow".to_string())?;
    let unused_untagged_records = u64::try_from(
        candidates
            .iter()
            .filter(|candidate| candidate.tags.is_empty())
            .count(),
    )
    .map_err(|_| "podman-images-count-overflow".to_string())?;
    let candidate_record_size_sum = candidates.iter().try_fold(0u64, |total, candidate| {
        total
            .checked_add(candidate.size_bytes)
            .ok_or_else(|| "podman-images-size-overflow".to_string())
    })?;
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
    Ok(PodmanUnusedImageEvidence {
        total_records,
        referenced_records,
        unused_records,
        unused_untagged_records,
        unused_tagged_records: unused_records.saturating_sub(unused_untagged_records),
        candidate_record_size_sum,
        candidate_set_sha256: lower_hex(&hasher.finalize()),
    })
}

fn parse_podman_images(output: &str) -> Result<PodmanUnusedImageEvidence, String> {
    let (total_records, referenced_records, candidates) = parse_unused_image_candidates(output)?;
    summarize_unused_image_candidates(total_records, referenced_records, &candidates)
}

fn raw_image_evidence(path: &Path) -> Result<RawImageEvidence, String> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| format!("raw-image-metadata:{error}"))?;
    if metadata.file_type().is_symlink() {
        return Err("raw-image-symbolic-link".to_string());
    }
    if !metadata.is_file() {
        return Err("raw-image-not-regular-file".to_string());
    }
    #[cfg(unix)]
    let allocated_bytes = {
        use std::os::unix::fs::MetadataExt;
        Some(metadata.blocks().saturating_mul(512))
    };
    #[cfg(not(unix))]
    let allocated_bytes = None;
    #[cfg(unix)]
    let identity_sha256 = {
        use std::os::unix::fs::MetadataExt;
        let mut hasher = Sha256::new();
        hasher.update(b"disksage-runtime-image-identity-v1\0");
        hasher.update(metadata.dev().to_le_bytes());
        hasher.update(metadata.ino().to_le_bytes());
        Some(lower_hex(&hasher.finalize()))
    };
    #[cfg(not(unix))]
    let identity_sha256 = None;
    Ok(RawImageEvidence {
        path: path.to_string_lossy().into_owned(),
        logical_bytes: metadata.len(),
        allocated_bytes,
        identity_sha256,
    })
}

/// Reads only the configured sparse image and its current host allocation.
fn configured_raw_image_evidence(
    record: &MachineInspectRecord,
) -> Result<RawImageEvidence, String> {
    let config_path = Path::new(&record.config_dir.path).join(format!("{}.json", record.name));
    fs::read_to_string(&config_path)
        .map_err(|error| format!("machine-config-read:{error}"))
        .and_then(|output| parse_machine_config(&output))
        .and_then(|path| raw_image_evidence(&path))
}

/// Reads only the configured sparse image and its current host allocation.
pub fn inspect_raw_image_evidence(
    podman_bin: &Path,
    requested_machine: &str,
    timeout: Duration,
) -> Result<RawImageEvidence, String> {
    if !valid_machine_name(requested_machine) {
        return Err("unsafe-requested-machine-name".into());
    }
    let record = command_text(
        podman_bin,
        &["machine", "inspect", requested_machine],
        timeout,
        "podman-machine-inspect",
    )
    .and_then(|output| parse_machine_inspect(&output))?;
    if record.name != requested_machine {
        return Err("machine-name-mismatch".into());
    }
    configured_raw_image_evidence(&record)
}

fn bounded_detail(value: &str) -> String {
    let flattened = value.replace(['\r', '\n'], " ");
    flattened.chars().take(512).collect()
}

fn drain_bounded<R: Read>(mut reader: R) -> std::io::Result<(Vec<u8>, bool)> {
    let mut captured = Vec::with_capacity(MAX_CAPTURE_BYTES.min(64 * 1024));
    let mut truncated = false;
    let mut buffer = [0u8; 16 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        let remaining = MAX_CAPTURE_BYTES.saturating_sub(captured.len());
        let retained = remaining.min(read);
        captured.extend_from_slice(&buffer[..retained]);
        if retained < read {
            truncated = true;
        }
    }
    Ok((captured, truncated))
}

fn join_capture(
    handle: thread::JoinHandle<std::io::Result<(Vec<u8>, bool)>>,
    label: &str,
    stream: &str,
) -> Result<(Vec<u8>, bool), String> {
    handle
        .join()
        .map_err(|_| format!("{label}-{stream}-reader-panicked"))?
        .map_err(|error| format!("{label}-{stream}:{error}"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CommandCapture {
    status_code: i32,
    stdout: String,
    stderr: String,
    output_truncated: bool,
}

fn command_capture(
    executable: &Path,
    args: &[&str],
    timeout: Duration,
    label: &str,
) -> Result<CommandCapture, String> {
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
    let mut child = command
        .spawn()
        .map_err(|error| format!("{label}-spawn:{error}"))?;
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
                let _ = join_capture(stdout_reader, label, "stdout");
                let _ = join_capture(stderr_reader, label, "stderr");
                return Err(format!("{label}-timeout"));
            }
            Ok(None) => thread::sleep(Duration::from_millis(25)),
            Err(error) => {
                terminate_readonly_process_tree(&mut child);
                let _ = join_capture(stdout_reader, label, "stdout");
                let _ = join_capture(stderr_reader, label, "stderr");
                return Err(format!("{label}-wait:{error}"));
            }
        }
    };

    // A completed CLI may leave descendants holding inherited pipes. Its private process group
    // can be terminated without touching the long-lived Podman VM process.
    terminate_readonly_process_tree(&mut child);
    let (stdout, stdout_truncated) = join_capture(stdout_reader, label, "stdout")?;
    let (stderr, stderr_truncated) = join_capture(stderr_reader, label, "stderr")?;
    if stdout_truncated || stderr_truncated {
        return Err(format!("{label}-output-too-large"));
    }
    Ok(CommandCapture {
        status_code: status.code().unwrap_or(-1),
        stdout: String::from_utf8(stdout).map_err(|_| format!("{label}-stdout-not-utf8"))?,
        stderr: String::from_utf8(stderr).map_err(|_| format!("{label}-stderr-not-utf8"))?,
        output_truncated: false,
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

fn command_text(
    executable: &Path,
    args: &[&str],
    timeout: Duration,
    label: &str,
) -> Result<String, String> {
    let output = command_capture(executable, args, timeout, label)?;
    if output.status_code != 0 {
        let detail = bounded_detail(&output.stderr);
        return Err(format!("{label}-failed:{detail}"));
    }
    Ok(output.stdout)
}

fn assess(
    machine: Option<&PodmanMachineEvidence>,
    raw_image: Option<&RawImageEvidence>,
    guest: Option<&GuestFilesystemEvidence>,
    store: Option<&PodmanStoreEvidence>,
    system_df: Option<&PodmanSystemDfEvidence>,
    unused_images: Option<&PodmanUnusedImageEvidence>,
    issues: &[String],
) -> PodmanReclaimAssessment {
    let mut reason_codes = vec!["host-physical-reclaim-unverified".to_string()];
    let mut recommended_actions = Vec::new();
    let gap = raw_image
        .and_then(|raw| raw.allocated_bytes)
        .zip(guest.map(|value| value.used_bytes))
        .map(|(allocated, used)| allocated.saturating_sub(used));

    if let Some(guest) = guest {
        let critical_ratio = guest
            .available_bytes
            .saturating_mul(100)
            .checked_div(guest.total_bytes.max(1))
            .unwrap_or(0)
            < 2;
        if guest.available_bytes < CRITICAL_GUEST_AVAILABLE_BYTES || critical_ratio {
            reason_codes.push("guest-filesystem-critical".to_string());
            recommended_actions.push(PodmanRecommendedAction {
                kind: PodmanRecommendedActionKind::RestoreGuestHeadroom,
                requires_human_approval: true,
                rationale: "게스트의 재생성 가능 캐시와 오래된 로그를 검토해 API가 시작할 여유를 확보합니다."
                    .to_string(),
            });
        }
    }

    if gap.is_some_and(|bytes| bytes >= MATERIAL_ALLOCATION_GAP_BYTES) {
        reason_codes.push("raw-allocation-exceeds-guest-used".to_string());
        recommended_actions.push(PodmanRecommendedAction {
            kind: PodmanRecommendedActionKind::ReviewGuestTrim,
            requires_human_approval: true,
            rationale: "게스트에서 해제된 블록이 호스트 raw 할당으로 남았는지 TRIM 전후 관측으로 확인합니다."
                .to_string(),
        });
    }

    if let Some(store) = store {
        if store.containers_stopped > 0 {
            recommended_actions.push(PodmanRecommendedAction {
                kind: PodmanRecommendedActionKind::ReviewStoppedContainers,
                requires_human_approval: true,
                rationale: format!(
                    "중지 컨테이너 {}개가 참조하는 이미지와 볼륨을 사람 검토 대상으로 유지합니다.",
                    store.containers_stopped
                ),
            });
        }
    } else if machine.is_some_and(|value| value.state.eq_ignore_ascii_case("running")) {
        reason_codes.push("podman-api-evidence-missing".to_string());
        recommended_actions.push(PodmanRecommendedAction {
            kind: PodmanRecommendedActionKind::InvestigateApi,
            requires_human_approval: false,
            rationale: "머신은 실행 중이지만 API 증거가 없어 소켓과 게스트 여유 공간을 점검합니다."
                .to_string(),
        });
    }

    let podman_reported_reclaimable_bytes = system_df.and_then(|evidence| {
        evidence
            .images
            .reclaimable_bytes
            .checked_add(evidence.containers.reclaimable_bytes)?
            .checked_add(evidence.local_volumes.reclaimable_bytes)
    });
    if let Some(evidence) = system_df {
        if evidence.images.reclaimable_bytes > 0 {
            reason_codes.push("podman-unused-images-reported".to_string());
            let rationale = if let Some(images) = unused_images {
                if images.candidate_record_size_sum != evidence.images.reclaimable_bytes {
                    reason_codes.push("podman-image-record-size-differs-from-df".to_string());
                }
                format!(
                    "Podman system df는 이미지 후보 {}바이트를 보고했고, 참조 0인 exact image record {}개의 size 합계는 {}바이트입니다. 재다운로드 비용과 shared layer 차이를 검토합니다.",
                    evidence.images.reclaimable_bytes,
                    images.unused_records,
                    images.candidate_record_size_sum
                )
            } else {
                format!(
                    "Podman이 미사용 이미지 후보 {}바이트를 보고했습니다. 재다운로드 비용과 참조 상태를 검토합니다.",
                    evidence.images.reclaimable_bytes
                )
            };
            recommended_actions.push(PodmanRecommendedAction {
                kind: PodmanRecommendedActionKind::ReviewUnusedImages,
                requires_human_approval: true,
                rationale,
            });
        }
        if evidence.local_volumes.reclaimable_bytes > 0 {
            reason_codes.push("podman-unused-volumes-reported".to_string());
            recommended_actions.push(PodmanRecommendedAction {
                kind: PodmanRecommendedActionKind::ReviewUnusedVolumes,
                requires_human_approval: true,
                rationale: format!(
                    "Podman이 미사용 volume 후보 {}바이트를 보고했습니다. 데이터 보존 여부를 별도 승인합니다.",
                    evidence.local_volumes.reclaimable_bytes
                ),
            });
        }
    }

    if !issues.is_empty() {
        reason_codes.push("partial-evidence".to_string());
    }
    reason_codes.sort();
    reason_codes.dedup();
    PodmanReclaimAssessment {
        physically_reclaimable_bytes: None,
        podman_reported_reclaimable_bytes,
        raw_allocated_minus_guest_used_bytes: gap,
        status: "unverified".to_string(),
        reason_codes,
        recommended_actions,
    }
}

/// Remove only freshly observed, untagged images with zero container references.
/// Never prunes volumes, tagged images, containers, or the Podman machine itself.
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
    let inspect = command_text(
        podman_bin,
        &["machine", "inspect", requested_machine],
        PODMAN_PRUNE_TIMEOUT,
        "podman-prune-machine-inspect",
    )
    .and_then(|output| parse_machine_inspect(&output))?;
    if inspect.name != requested_machine || !inspect.state.eq_ignore_ascii_case("running") {
        return Err("podman-prune-machine-not-running".into());
    }
    let images_output = command_text(
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
    let (total_records, referenced_records, candidates) =
        parse_unused_image_candidates(&images_output)?;
    let evidence =
        summarize_unused_image_candidates(total_records, referenced_records, &candidates)?;
    let expected_phrase =
        if evidence.unused_untagged_records > 0 && evidence.unused_tagged_records == 0 {
            prune_approval_phrase(&evidence.candidate_set_sha256)
        } else {
            return Err("podman-prune-tagged-or-empty-candidate-set".into());
        };
    if confirmation_phrase != expected_phrase {
        return Err("podman-prune-confirmation-mismatch".into());
    }

    let before_available_bytes = std::env::current_dir()
        .ok()
        .and_then(|path| crate::volume_pressure::snapshot_volume(&path, executed_at_ms).ok())
        .map(|snapshot| snapshot.available_bytes);
    let output = command_capture(
        podman_bin,
        &[
            "--connection",
            requested_machine,
            "image",
            "prune",
            "--force",
        ],
        PODMAN_PRUNE_TIMEOUT,
        "podman-prune-dangling-images",
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
        candidate_set_sha256: evidence.candidate_set_sha256,
        command: vec![
            "podman".into(),
            "--connection".into(),
            requested_machine.into(),
            "image".into(),
            "prune".into(),
            "--force".into(),
        ],
        status_code: output.status_code,
        stdout: output.stdout,
        stderr: output.stderr,
        output_truncated: output.output_truncated,
        executed: output.status_code == 0,
        executed_at_ms,
        before_available_bytes,
        after_available_bytes,
        observed_available_gain_bytes,
        rationale: rationale.to_string(),
    })
}

pub fn probe_podman_reclaim(
    podman_bin: &Path,
    requested_machine: &str,
    timeout: Duration,
) -> PodmanReclaimPlan {
    let started = Instant::now();
    let mut issues = Vec::new();
    if !valid_machine_name(requested_machine) {
        issues.push("unsafe-requested-machine-name".to_string());
    }

    let inspect = if issues.is_empty() {
        command_text(
            podman_bin,
            &["machine", "inspect", requested_machine],
            timeout,
            "podman-machine-inspect",
        )
        .and_then(|output| parse_machine_inspect(&output))
        .and_then(|record| {
            if record.name == requested_machine {
                Ok(record)
            } else {
                Err("machine-name-mismatch".to_string())
            }
        })
        .map_err(|error| issues.push(error))
        .ok()
    } else {
        None
    };

    let machine = inspect.as_ref().map(|record| PodmanMachineEvidence {
        name: record.name.clone(),
        state: record.state.clone(),
        configured_disk_bytes: record
            .resources
            .disk_size_gib
            .and_then(|gib| gib.checked_mul(GIB)),
    });

    let raw_image = inspect.as_ref().and_then(|record| {
        configured_raw_image_evidence(record)
            .map_err(|error| issues.push(error))
            .ok()
    });

    let guest_filesystem = machine
        .as_ref()
        .filter(|value| value.state.eq_ignore_ascii_case("running"))
        .and_then(|_| {
            command_text(
                podman_bin,
                &[
                    "machine",
                    "ssh",
                    requested_machine,
                    "--",
                    "df",
                    "-B1",
                    "--output=size,used,avail",
                    "/",
                ],
                timeout,
                "podman-guest-df",
            )
            .and_then(|output| parse_guest_df(&output))
            .map_err(|error| issues.push(error))
            .ok()
        });

    let store = inspect.as_ref().and_then(|_| {
        command_text(
            podman_bin,
            &[
                "--connection",
                requested_machine,
                "info",
                "--format",
                "json",
            ],
            timeout,
            "podman-info",
        )
        .and_then(|output| parse_podman_info(&output))
        .map_err(|error| issues.push(error))
        .ok()
    });

    let system_df = inspect.as_ref().and_then(|_| {
        command_text(
            podman_bin,
            &[
                "--connection",
                requested_machine,
                "system",
                "df",
                "--format",
                "json",
            ],
            timeout,
            "podman-system-df",
        )
        .and_then(|output| parse_podman_system_df(&output))
        .map_err(|error| issues.push(error))
        .ok()
    });

    let unused_images = inspect.as_ref().and_then(|_| {
        command_text(
            podman_bin,
            &[
                "--connection",
                requested_machine,
                "images",
                "--all",
                "--format",
                "json",
            ],
            timeout,
            "podman-images",
        )
        .and_then(|output| parse_podman_images(&output))
        .map_err(|error| issues.push(error))
        .ok()
    });

    let assessment = assess(
        machine.as_ref(),
        raw_image.as_ref(),
        guest_filesystem.as_ref(),
        store.as_ref(),
        system_df.as_ref(),
        unused_images.as_ref(),
        &issues,
    );
    let dangling_prune_approval_phrase = unused_images.as_ref().and_then(|evidence| {
        (evidence.unused_untagged_records > 0 && evidence.unused_tagged_records == 0)
            .then(|| prune_approval_phrase(&evidence.candidate_set_sha256))
    });
    PodmanReclaimPlan {
        schema_kind: PODMAN_RECLAIM_SCHEMA_KIND,
        schema_version: 3,
        platform: std::env::consts::OS,
        evidence_complete: issues.is_empty()
            && machine.is_some()
            && raw_image.is_some()
            && guest_filesystem.is_some()
            && store.is_some()
            && system_df.is_some()
            && unused_images.is_some(),
        elapsed_ms: started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
        machine,
        raw_image,
        guest_filesystem,
        store,
        system_df,
        unused_images,
        dangling_prune_approval_phrase,
        assessment,
        issues,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INSPECT: &str = r#"[{"ConfigDir":{"Path":"/tmp/podman"},"Name":"podman-machine-default","State":"running","Resources":{"DiskSize":100}}]"#;
    const INFO: &str = r#"{"store":{"graphRoot":"/var/home/core/.local/share/containers/storage","graphRootAllocated":106769133568,"graphRootUsed":36028432384,"imageStore":{"number":35},"containerStore":{"number":9,"running":0,"stopped":9}}}"#;
    const SYSTEM_DF: &str = r#"[{"Type":"Images","Total":46,"Active":39,"RawSize":18998832893,"RawReclaimable":14203017596},{"Type":"Containers","Total":9,"Active":0,"RawSize":96085,"RawReclaimable":96085},{"Type":"Local Volumes","Total":69,"Active":7,"RawSize":3107287618,"RawReclaimable":2889662821}]"#;
    const IMAGES: &str = r#"[{"Id":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","RepoTags":["localhost/used:latest"],"Containers":1,"Size":100},{"Id":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","RepoTags":[],"Containers":0,"Size":300},{"Id":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","RepoTags":null,"Containers":0,"Size":200}]"#;

    #[test]
    fn parses_machine_and_guest_evidence() {
        let inspect = parse_machine_inspect(INSPECT).unwrap();
        assert_eq!(inspect.name, DEFAULT_PODMAN_MACHINE);
        assert_eq!(inspect.resources.disk_size_gib, Some(100));
        let guest =
            parse_guest_df("1B-blocks Used Avail\n106769133568 36028432384 70740701184\n").unwrap();
        assert_eq!(guest.available_bytes, 70_740_701_184);
    }

    #[test]
    fn rejects_inconsistent_or_ambiguous_snapshots() {
        assert!(parse_guest_df("10 9 9\n").is_err());
        assert!(parse_machine_inspect("[]").is_err());
        assert!(!valid_machine_name("../escape"));
        assert!(!valid_machine_name("--connection"));
        assert!(!valid_machine_name(".."));
        assert!(valid_machine_name("podman-machine_default.1"));
    }

    #[test]
    fn parses_store_counts_without_claiming_reclaim() {
        let store = parse_podman_info(INFO).unwrap();
        let system_df = parse_podman_system_df(SYSTEM_DF).unwrap();
        let unused_images = parse_podman_images(IMAGES).unwrap();
        assert_eq!(store.images, 35);
        assert_eq!(store.containers_stopped, 9);
        assert_eq!(system_df.images.reclaimable_bytes, 14_203_017_596);
        assert_eq!(system_df.local_volumes.active, 7);
        assert_eq!(unused_images.total_records, 3);
        assert_eq!(unused_images.referenced_records, 1);
        assert_eq!(unused_images.unused_records, 2);
        assert_eq!(unused_images.unused_untagged_records, 2);
        assert_eq!(unused_images.candidate_record_size_sum, 500);
        assert_eq!(unused_images.candidate_set_sha256.len(), 64);
        let guest = GuestFilesystemEvidence {
            total_bytes: 100 * GIB,
            used_bytes: 30 * GIB,
            available_bytes: 69 * GIB,
        };
        let raw = RawImageEvidence {
            path: "/tmp/machine.raw".into(),
            logical_bytes: 100 * GIB,
            allocated_bytes: Some(70 * GIB),
            identity_sha256: Some("a".repeat(64)),
        };
        let result = assess(
            None,
            Some(&raw),
            Some(&guest),
            Some(&store),
            Some(&system_df),
            Some(&unused_images),
            &[],
        );
        assert_eq!(result.physically_reclaimable_bytes, None);
        assert_eq!(
            result.podman_reported_reclaimable_bytes,
            Some(17_092_776_502)
        );
        assert_eq!(result.raw_allocated_minus_guest_used_bytes, Some(40 * GIB));
        assert!(result
            .reason_codes
            .contains(&"raw-allocation-exceeds-guest-used".to_string()));
        assert!(result.recommended_actions.iter().all(|action| action.kind
            == PodmanRecommendedActionKind::InvestigateApi
            || action.requires_human_approval));
    }

    #[test]
    fn critical_guest_and_partial_evidence_are_explicit() {
        let machine = PodmanMachineEvidence {
            name: DEFAULT_PODMAN_MACHINE.into(),
            state: "running".into(),
            configured_disk_bytes: Some(100 * GIB),
        };
        let guest = GuestFilesystemEvidence {
            total_bytes: 100 * GIB,
            used_bytes: 99 * GIB,
            available_bytes: GIB,
        };
        let result = assess(
            Some(&machine),
            None,
            Some(&guest),
            None,
            None,
            None,
            &["podman-info-timeout".into()],
        );
        assert!(result
            .reason_codes
            .contains(&"guest-filesystem-critical".to_string()));
        assert!(result
            .reason_codes
            .contains(&"partial-evidence".to_string()));
        assert!(result.recommended_actions.iter().any(|action| action.kind
            == PodmanRecommendedActionKind::InvestigateApi
            && !action.requires_human_approval));
    }

    #[test]
    fn machine_config_requires_an_absolute_raw_path() {
        assert_eq!(
            parse_machine_config(r#"{"ImagePath":{"Path":"/tmp/machine.raw"}}"#).unwrap(),
            PathBuf::from("/tmp/machine.raw")
        );
        assert!(parse_machine_config(r#"{"ImagePath":{"Path":"relative.raw"}}"#).is_err());
    }

    #[test]
    fn system_df_rejects_missing_duplicate_or_inconsistent_categories() {
        assert!(parse_podman_system_df("[]").is_err());
        assert!(parse_podman_system_df(
            r#"[{"Type":"Images","Total":1,"Active":2,"RawSize":1,"RawReclaimable":0}]"#
        )
        .is_err());
        assert!(parse_podman_system_df(
            r#"[{"Type":"Images","Total":1,"Active":0,"RawSize":1,"RawReclaimable":0},{"Type":"Images","Total":1,"Active":0,"RawSize":1,"RawReclaimable":0}]"#
        )
        .is_err());
    }

    #[test]
    fn image_summary_binds_sorted_exact_candidates_without_exposing_ids() {
        let forward = parse_podman_images(IMAGES).unwrap();
        let reverse = parse_podman_images(
            r#"[{"Id":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","RepoTags":null,"Containers":0,"Size":200},{"Id":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","RepoTags":[],"Containers":0,"Size":300},{"Id":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","RepoTags":["localhost/used:latest"],"Containers":1,"Size":100}]"#,
        )
        .unwrap();
        assert_eq!(forward, reverse);
        assert!(!serde_json::to_string(&forward)
            .unwrap()
            .contains(&"b".repeat(64)));
        assert!(parse_podman_images(
            r#"[{"Id":"NOT-AN-ID","RepoTags":[],"Containers":0,"Size":1}]"#
        )
        .is_err());
    }

    #[test]
    fn dangling_prune_phrase_is_only_offered_for_untagged_candidates() {
        let (_, _, candidates) = parse_unused_image_candidates(IMAGES).unwrap();
        let evidence = summarize_unused_image_candidates(3, 1, &candidates).unwrap();
        assert_eq!(evidence.unused_untagged_records, 2);
        assert_eq!(evidence.unused_tagged_records, 0);
        assert_eq!(
            prune_approval_phrase(&evidence.candidate_set_sha256),
            format!(
                "DiskSage Podman dangling image prune 승인 {}",
                evidence.candidate_set_sha256
            )
        );
        let tagged = r#"[{"Id":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","RepoTags":["localhost/keep:latest"],"Containers":0,"Size":200}]"#;
        let (_, _, tagged_candidates) = parse_unused_image_candidates(tagged).unwrap();
        let tagged_evidence = summarize_unused_image_candidates(1, 0, &tagged_candidates).unwrap();
        assert_eq!(tagged_evidence.unused_untagged_records, 0);
        assert_eq!(tagged_evidence.unused_tagged_records, 1);
        let digest_only = r#"[{"Id":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","RepoTags":null,"RepoDigests":["docker.io/library/python@sha256:abc"],"Names":["docker.io/library/python@sha256:abc"],"Containers":0,"Size":200}]"#;
        let (_, _, digest_candidates) = parse_unused_image_candidates(digest_only).unwrap();
        let digest_evidence = summarize_unused_image_candidates(1, 0, &digest_candidates).unwrap();
        assert_eq!(digest_evidence.unused_untagged_records, 0);
        assert_eq!(digest_evidence.unused_tagged_records, 1);
    }

    #[test]
    fn serialized_contract_keeps_reclaim_unknown_and_action_codes_stable() {
        let plan = PodmanReclaimPlan {
            schema_kind: PODMAN_RECLAIM_SCHEMA_KIND,
            schema_version: 1,
            platform: "macos",
            evidence_complete: false,
            elapsed_ms: 7,
            machine: None,
            raw_image: None,
            guest_filesystem: None,
            store: None,
            system_df: None,
            unused_images: None,
            dangling_prune_approval_phrase: None,
            assessment: PodmanReclaimAssessment {
                physically_reclaimable_bytes: None,
                podman_reported_reclaimable_bytes: None,
                raw_allocated_minus_guest_used_bytes: None,
                status: "unverified".into(),
                reason_codes: vec!["host-physical-reclaim-unverified".into()],
                recommended_actions: vec![PodmanRecommendedAction {
                    kind: PodmanRecommendedActionKind::ReviewGuestTrim,
                    requires_human_approval: true,
                    rationale: "review".into(),
                }],
            },
            issues: vec!["partial-evidence".into()],
        };

        let value = serde_json::to_value(plan).unwrap();
        assert_eq!(value["schema_kind"], PODMAN_RECLAIM_SCHEMA_KIND);
        assert!(value["assessment"]["physically_reclaimable_bytes"].is_null());
        assert_eq!(
            value["assessment"]["recommended_actions"][0]["kind"],
            "review_guest_trim"
        );
        assert_eq!(
            value["assessment"]["recommended_actions"][0]["requires_human_approval"],
            true
        );
    }

    #[cfg(unix)]
    #[test]
    fn external_probe_drains_stdout_and_stderr_concurrently() {
        let started = Instant::now();
        let output = command_text(
            Path::new("/bin/sh"),
            &[
                "-c",
                "head -c 131072 /dev/zero; head -c 131072 /dev/zero >&2",
            ],
            Duration::from_secs(5),
            "dual-stream-probe",
        )
        .unwrap();
        assert_eq!(output.len(), 131_072);
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[cfg(unix)]
    #[test]
    fn external_probe_rejects_oversized_output_without_unbounded_allocation() {
        let error = command_text(
            Path::new("/bin/sh"),
            &["-c", "head -c 1048577 /dev/zero"],
            Duration::from_secs(5),
            "oversized-probe",
        )
        .unwrap_err();
        assert_eq!(error, "oversized-probe-output-too-large");
    }

    #[cfg(unix)]
    #[test]
    fn external_probe_timeout_is_bounded() {
        let started = Instant::now();
        let error = command_text(
            Path::new("/bin/sleep"),
            &["2"],
            Duration::from_millis(25),
            "slow-probe",
        )
        .unwrap_err();
        assert_eq!(error, "slow-probe-timeout");
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[cfg(unix)]
    #[test]
    fn completed_probe_closes_inherited_descendant_pipes() {
        let started = Instant::now();
        let output = command_text(
            Path::new("/bin/sh"),
            &["-c", "sleep 10 & printf complete"],
            Duration::from_secs(2),
            "descendant-pipe-probe",
        )
        .unwrap();
        assert_eq!(output, "complete");
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}
