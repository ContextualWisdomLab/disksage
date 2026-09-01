//! Read-only Colima profile and cache evidence.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const MAX_OUTPUT_BYTES: usize = 1_048_576;
const MAX_CACHE_ENTRIES: u64 = 1_000_000;
const MAX_DANGLING_IMAGE_IDS: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ColimaProfileEvidence {
    pub name: String,
    pub status: String,
    pub runtime: String,
    pub configured_disk_bytes: u64,
    pub compaction_eligible: bool,
    pub blockers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ColimaReclaimPlan {
    pub schema_kind: &'static str,
    pub schema_version: u32,
    pub ontology_class: &'static str,
    pub evidence_complete: bool,
    pub executable_available: bool,
    pub cache_allocated_bytes: u64,
    pub cache_entries: u64,
    pub plan_fingerprint: Option<String>,
    pub cache_prune_approval_phrase: Option<String>,
    pub profiles: Vec<ColimaProfileEvidence>,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ColimaCachePruneExecution {
    pub schema_kind: &'static str,
    pub schema_version: u32,
    pub plan_fingerprint: String,
    pub before_cache_allocated_bytes: u64,
    pub after_cache_allocated_bytes: u64,
    pub observed_allocation_reduction_bytes: u64,
    pub status_code: i32,
    pub executed: bool,
    pub executed_at_ms: u64,
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ColimaDanglingImagePlan {
    pub schema_kind: &'static str,
    pub schema_version: u32,
    pub ontology_class: &'static str,
    pub profile: String,
    pub candidate_count: usize,
    pub candidate_set_sha256: Option<String>,
    pub exact_approval_phrase: Option<String>,
    pub evidence_complete: bool,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ColimaDanglingImageExecution {
    pub schema_kind: &'static str,
    pub schema_version: u32,
    pub ontology_class: &'static str,
    pub profile: String,
    pub candidate_set_sha256: String,
    pub before_candidate_count: usize,
    pub after_candidate_count: usize,
    pub observed_removed_count: usize,
    pub status_code: i32,
    pub executed_at_ms: u64,
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ColimaEmptyVolumePlan {
    pub schema_kind: &'static str,
    pub schema_version: u32,
    pub ontology_class: &'static str,
    pub profile: String,
    pub dangling_count: usize,
    pub empty_candidate_count: usize,
    pub candidate_set_sha256: Option<String>,
    pub exact_approval_phrase: Option<String>,
    pub evidence_complete: bool,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ColimaEmptyVolumeExecution {
    pub schema_kind: &'static str,
    pub schema_version: u32,
    pub ontology_class: &'static str,
    pub profile: String,
    pub candidate_set_sha256: String,
    pub before_candidate_count: usize,
    pub after_candidate_count: usize,
    pub observed_removed_count: usize,
    pub status_code: i32,
    pub executed_at_ms: u64,
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ColimaGuestTrimPlan {
    pub schema_kind: &'static str,
    pub schema_version: u32,
    pub ontology_class: &'static str,
    pub profile: String,
    pub configured_disk_bytes: u64,
    pub candidate_fingerprint: Option<String>,
    pub exact_approval_phrase: Option<String>,
    pub guest_trim_eligible: bool,
    pub native_host_compaction_supported: bool,
    pub blockers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ColimaGuestTrimExecution {
    pub schema_kind: &'static str,
    pub schema_version: u32,
    pub ontology_class: &'static str,
    pub profile: String,
    pub candidate_fingerprint: String,
    pub status_code: i32,
    pub executed: bool,
    pub before_available_bytes: Option<u64>,
    pub after_available_bytes: Option<u64>,
    pub observed_available_gain_bytes: Option<u64>,
    pub executed_at_ms: u64,
    pub rationale: String,
}

#[derive(Debug, Deserialize)]
struct ProfileRecord {
    name: String,
    status: String,
    #[serde(default)]
    runtime: String,
    #[serde(default)]
    disk: u64,
}

fn parse_profiles(output: &str) -> Result<Vec<ColimaProfileEvidence>, String> {
    output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let record: ProfileRecord =
                serde_json::from_str(line).map_err(|_| "colima-list-json-invalid".to_string())?;
            let stopped = !record.status.eq_ignore_ascii_case("running");
            Ok(ColimaProfileEvidence {
                name: record.name,
                status: record.status,
                runtime: record.runtime,
                configured_disk_bytes: record.disk,
                compaction_eligible: false,
                blockers: if stopped {
                    vec!["guest-free-space-trim-not-attested".into()]
                } else {
                    vec![
                        "virtual-machine-running".into(),
                        "guest-free-space-trim-not-attested".into(),
                    ]
                },
            })
        })
        .collect()
}

fn valid_profile_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value != "."
        && value != ".."
        && !value.starts_with('-')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn dangling_image_ids(output: &str) -> Result<Vec<String>, String> {
    let mut ids = output
        .lines()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.strip_prefix("sha256:").unwrap_or(value))
        .map(str::to_string)
        .collect::<Vec<_>>();
    if ids.len() > MAX_DANGLING_IMAGE_IDS {
        return Err("colima-dangling-image-limit-exceeded".into());
    }
    if ids.iter().any(|id| {
        id.len() != 64
            || !id
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    }) {
        return Err("colima-dangling-image-id-invalid".into());
    }
    ids.sort();
    ids.dedup();
    Ok(ids)
}

fn digest_values(domain: &[u8], values: impl IntoIterator<Item = impl AsRef<[u8]>>) -> String {
    let mut hasher = Sha256::new();
    hasher.update((domain.len() as u64).to_be_bytes());
    hasher.update(domain);
    for value in values {
        let value = value.as_ref();
        hasher.update((value.len() as u64).to_be_bytes());
        hasher.update(value);
    }
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn allocated_bytes(metadata: &fs::Metadata) -> u64 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        return metadata.blocks().saturating_mul(512);
    }
    #[cfg(not(unix))]
    metadata.len()
}

fn cache_allocation(root: &Path) -> Result<(u64, u64), String> {
    let metadata = match fs::symlink_metadata(root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok((0, 0)),
        Err(_) => return Err("colima-cache-metadata-unavailable".into()),
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("colima-cache-root-not-real-directory".into());
    }
    let mut pending = vec![root.to_path_buf()];
    let mut bytes = 0u64;
    let mut entries = 0u64;
    while let Some(directory) = pending.pop() {
        let children = fs::read_dir(directory).map_err(|_| "colima-cache-read-failed")?;
        for child in children {
            let child = child.map_err(|_| "colima-cache-read-failed")?;
            entries = entries.saturating_add(1);
            if entries > MAX_CACHE_ENTRIES {
                return Err("colima-cache-entry-limit-exceeded".into());
            }
            let metadata = fs::symlink_metadata(child.path())
                .map_err(|_| "colima-cache-metadata-unavailable")?;
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                pending.push(child.path());
            } else if metadata.is_file() {
                bytes = bytes.saturating_add(allocated_bytes(&metadata));
            }
        }
    }
    Ok((bytes, entries))
}

struct CommandOutput {
    status_code: i32,
    stdout: String,
}

fn run_command(
    executable: &Path,
    arguments: &[&str],
    timeout: Duration,
    label: &str,
) -> Result<CommandOutput, String> {
    let mut child = Command::new(executable)
        .args(arguments)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| format!("{label}-spawn-failed"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| format!("{label}-stdout-unavailable"))?;
    let reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout
            .take((MAX_OUTPUT_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                drop(reader);
                return Err(format!("{label}-timeout"));
            }
            Ok(None) => thread::sleep(Duration::from_millis(25)),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                drop(reader);
                return Err(format!("{label}-wait-failed"));
            }
        }
    };
    let bytes = reader
        .join()
        .map_err(|_| format!("{label}-reader-panicked"))?
        .map_err(|_| format!("{label}-read-failed"))?;
    if bytes.len() > MAX_OUTPUT_BYTES {
        return Err(format!("{label}-output-too-large"));
    }
    Ok(CommandOutput {
        status_code: status.code().unwrap_or(-1),
        stdout: String::from_utf8(bytes).map_err(|_| format!("{label}-output-not-utf8"))?,
    })
}

fn run_list(executable: &Path, timeout: Duration) -> Result<String, String> {
    let output = run_command(executable, &["list", "--json"], timeout, "colima-list")?;
    if output.status_code != 0 {
        return Err("colima-list-failed".into());
    }
    Ok(output.stdout)
}

fn plan_fingerprint(
    executable: &Path,
    cache_root: &Path,
    cache_allocated_bytes: u64,
    cache_entries: u64,
) -> Result<String, String> {
    let mut hasher = Sha256::new();
    let executable_identity = crate::safety::filesystem_object_id(executable)
        .map_err(|_| "colima-executable-identity-unavailable".to_string())?;
    let allocated_bytes = cache_allocated_bytes.to_be_bytes();
    let entries = cache_entries.to_be_bytes();
    for value in [
        b"disksage.colima-cache-prune/v1".as_slice(),
        executable_identity.as_bytes(),
        cache_root.as_os_str().as_encoded_bytes(),
        &allocated_bytes,
        &entries,
    ] {
        hasher.update((value.len() as u64).to_be_bytes());
        hasher.update(value);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

pub fn plan_colima_reclaim(
    executable: &Path,
    cache_root: &Path,
    timeout: Duration,
) -> ColimaReclaimPlan {
    let mut issues = Vec::new();
    let executable_available = fs::symlink_metadata(executable)
        .map(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
        .unwrap_or(false);
    let (cache_allocated_bytes, cache_entries) = match cache_allocation(cache_root) {
        Ok(value) => value,
        Err(error) => {
            issues.push(error);
            (0, 0)
        }
    };
    let profiles = if executable_available {
        match run_list(executable, timeout).and_then(|output| parse_profiles(&output)) {
            Ok(profiles) => profiles,
            Err(error) => {
                issues.push(error);
                Vec::new()
            }
        }
    } else {
        issues.push("colima-executable-unavailable".into());
        Vec::new()
    };
    let fingerprint = if executable_available && issues.is_empty() && cache_allocated_bytes > 0 {
        match plan_fingerprint(executable, cache_root, cache_allocated_bytes, cache_entries) {
            Ok(value) => Some(value),
            Err(error) => {
                issues.push(error);
                None
            }
        }
    } else {
        None
    };
    ColimaReclaimPlan {
        schema_kind: "disksage.colima-reclaim-plan",
        schema_version: 1,
        ontology_class: "https://disksage.app/ontology#ColimaVirtualDisk",
        evidence_complete: issues.is_empty(),
        executable_available,
        cache_allocated_bytes,
        cache_entries,
        cache_prune_approval_phrase: fingerprint
            .as_ref()
            .map(|value| format!("DiskSage Colima cache prune 승인 {value}")),
        plan_fingerprint: fingerprint,
        profiles,
        issues,
    }
}

pub fn execute_colima_cache_prune(
    executable: &Path,
    cache_root: &Path,
    confirmation_phrase: &str,
    rationale: &str,
    executed_at_ms: u64,
) -> Result<ColimaCachePruneExecution, String> {
    if executed_at_ms == 0 {
        return Err("colima-cache-prune-time-invalid".into());
    }
    if rationale.trim() != rationale
        || rationale.is_empty()
        || rationale.chars().count() > 1_000
        || rationale.chars().any(char::is_control)
    {
        return Err("colima-cache-prune-rationale-invalid".into());
    }
    let plan = plan_colima_reclaim(executable, cache_root, Duration::from_secs(10));
    if !plan.evidence_complete {
        return Err("colima-cache-prune-evidence-incomplete".into());
    }
    let fingerprint = plan
        .plan_fingerprint
        .ok_or("colima-cache-prune-empty-candidate")?;
    let expected = plan
        .cache_prune_approval_phrase
        .ok_or("colima-cache-prune-empty-candidate")?;
    if confirmation_phrase != expected {
        return Err("colima-cache-prune-confirmation-mismatch".into());
    }
    let output = run_command(
        executable,
        &["prune", "--force"],
        Duration::from_secs(30),
        "colima-cache-prune",
    )?;
    let after = cache_allocation(cache_root)?.0;
    Ok(ColimaCachePruneExecution {
        schema_kind: "disksage.colima-cache-prune-execution",
        schema_version: 1,
        plan_fingerprint: fingerprint,
        before_cache_allocated_bytes: plan.cache_allocated_bytes,
        after_cache_allocated_bytes: after,
        observed_allocation_reduction_bytes: plan.cache_allocated_bytes.saturating_sub(after),
        status_code: output.status_code,
        executed: output.status_code == 0,
        executed_at_ms,
        rationale: rationale.into(),
    })
}

fn current_dangling_image_ids(
    executable: &Path,
    profile: &str,
    timeout: Duration,
) -> Result<Vec<String>, String> {
    let output = run_command(
        executable,
        &[
            "--profile",
            profile,
            "ssh",
            "--",
            "docker",
            "image",
            "ls",
            "--filter",
            "dangling=true",
            "--quiet",
            "--no-trunc",
        ],
        timeout,
        "colima-dangling-image-list",
    )?;
    if output.status_code != 0 {
        return Err("colima-dangling-image-list-failed".into());
    }
    dangling_image_ids(&output.stdout)
}

fn validate_running_docker_profile(
    executable: &Path,
    profile: &str,
    timeout: Duration,
) -> Result<(), String> {
    if !valid_profile_name(profile) {
        return Err("colima-profile-name-invalid".into());
    }
    let profiles = run_list(executable, timeout).and_then(|output| parse_profiles(&output))?;
    match profiles.iter().find(|candidate| candidate.name == profile) {
        Some(candidate)
            if candidate.status.eq_ignore_ascii_case("running")
                && candidate.runtime == "docker" =>
        {
            Ok(())
        }
        Some(candidate) if !candidate.status.eq_ignore_ascii_case("running") => {
            Err("colima-profile-not-running".into())
        }
        Some(_) => Err("colima-profile-runtime-not-docker".into()),
        None => Err("colima-profile-not-found".into()),
    }
}

fn running_profile(
    executable: &Path,
    profile: &str,
    timeout: Duration,
) -> Result<ColimaProfileEvidence, String> {
    if !valid_profile_name(profile) {
        return Err("colima-profile-name-invalid".into());
    }
    let profiles = run_list(executable, timeout).and_then(|output| parse_profiles(&output))?;
    match profiles
        .into_iter()
        .find(|candidate| candidate.name == profile)
    {
        Some(candidate) if candidate.status.eq_ignore_ascii_case("running") => Ok(candidate),
        Some(_) => Err("colima-profile-not-running".into()),
        None => Err("colima-profile-not-found".into()),
    }
}

pub fn plan_colima_dangling_images(
    executable: &Path,
    profile: &str,
    timeout: Duration,
) -> ColimaDanglingImagePlan {
    let mut issues = Vec::new();
    if let Err(error) = validate_running_docker_profile(executable, profile, timeout) {
        issues.push(error);
    }
    let ids = if issues.is_empty() {
        current_dangling_image_ids(executable, profile, timeout)
            .map_err(|error| issues.push(error))
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let fingerprint =
        (!ids.is_empty()).then(|| digest_values(b"disksage.colima-dangling-images/v1", ids.iter()));
    ColimaDanglingImagePlan {
        schema_kind: "disksage.colima-dangling-image-plan",
        schema_version: 1,
        ontology_class: "https://disksage.app/ontology#ColimaDanglingImage",
        profile: profile.into(),
        candidate_count: ids.len(),
        exact_approval_phrase: fingerprint
            .as_ref()
            .map(|value| format!("DiskSage Colima dangling image 제거 승인 {value}")),
        candidate_set_sha256: fingerprint,
        evidence_complete: issues.is_empty(),
        issues,
    }
}

fn valid_volume_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 255
        && !value.starts_with('-')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'))
}

fn dangling_volume_names(output: &str) -> Result<Vec<String>, String> {
    let mut names = output
        .lines()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    if names.len() > MAX_DANGLING_IMAGE_IDS {
        return Err("colima-dangling-volume-limit-exceeded".into());
    }
    if names.iter().any(|name| !valid_volume_name(name)) {
        return Err("colima-dangling-volume-name-invalid".into());
    }
    names.sort();
    names.dedup();
    Ok(names)
}

fn safe_guest_path(value: &str) -> bool {
    value.starts_with('/')
        && !value.split('/').any(|component| component == "..")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'_' | b'.' | b'-'))
}

fn current_empty_dangling_volumes(
    executable: &Path,
    profile: &str,
    timeout: Duration,
) -> Result<(usize, Vec<String>), String> {
    let listed = run_command(
        executable,
        &[
            "--profile",
            profile,
            "ssh",
            "--",
            "docker",
            "volume",
            "ls",
            "--filter",
            "dangling=true",
            "--quiet",
        ],
        timeout,
        "colima-dangling-volume-list",
    )?;
    if listed.status_code != 0 {
        return Err("colima-dangling-volume-list-failed".into());
    }
    let names = dangling_volume_names(&listed.stdout)?;
    if names.is_empty() {
        return Ok((0, Vec::new()));
    }
    let mut arguments = vec![
        "--profile".to_string(),
        profile.to_string(),
        "ssh".into(),
        "--".into(),
        "docker".into(),
        "volume".into(),
        "inspect".into(),
        "--format".into(),
        "{{.Name}}|{{.Mountpoint}}".into(),
    ];
    arguments.extend(names.iter().cloned());
    let references = arguments.iter().map(String::as_str).collect::<Vec<_>>();
    let inspected = run_command(
        executable,
        &references,
        timeout,
        "colima-dangling-volume-inspect",
    )?;
    if inspected.status_code != 0 {
        return Err("colima-dangling-volume-inspect-failed".into());
    }
    let mut mounts = std::collections::BTreeMap::new();
    for line in inspected.stdout.lines() {
        let (name, mount) = line
            .trim()
            .split_once('|')
            .ok_or("colima-dangling-volume-inspect-invalid")?;
        if !names.iter().any(|candidate| candidate == name) || !safe_guest_path(mount) {
            return Err("colima-dangling-volume-inspect-invalid".into());
        }
        mounts.insert(name.to_string(), mount.to_string());
    }
    if mounts.len() != names.len() {
        return Err("colima-dangling-volume-inspect-incomplete".into());
    }
    let mut empty = Vec::new();
    for name in &names {
        let mount = &mounts[name];
        let observed = run_command(
            executable,
            &[
                "--profile",
                profile,
                "ssh",
                "--",
                "sudo",
                "find",
                mount,
                "-mindepth",
                "1",
                "-print",
                "-quit",
            ],
            timeout,
            "colima-dangling-volume-content",
        )?;
        if observed.status_code != 0 {
            return Err("colima-dangling-volume-content-failed".into());
        }
        if observed.stdout.trim().is_empty() {
            empty.push(name.clone());
        }
    }
    Ok((names.len(), empty))
}

pub fn plan_colima_empty_volumes(
    executable: &Path,
    profile: &str,
    timeout: Duration,
) -> ColimaEmptyVolumePlan {
    let mut issues = Vec::new();
    if let Err(error) = validate_running_docker_profile(executable, profile, timeout) {
        issues.push(error);
    }
    let (dangling_count, candidates) = if issues.is_empty() {
        current_empty_dangling_volumes(executable, profile, timeout)
            .map_err(|error| issues.push(error))
            .unwrap_or_default()
    } else {
        (0, Vec::new())
    };
    let fingerprint = (!candidates.is_empty()).then(|| {
        digest_values(
            b"disksage.colima-empty-dangling-volumes/v1",
            candidates.iter(),
        )
    });
    ColimaEmptyVolumePlan {
        schema_kind: "disksage.colima-empty-volume-plan",
        schema_version: 1,
        ontology_class: "https://disksage.app/ontology#ColimaEmptyVolume",
        profile: profile.into(),
        dangling_count,
        empty_candidate_count: candidates.len(),
        exact_approval_phrase: fingerprint
            .as_ref()
            .map(|value| format!("DiskSage Colima empty volume 제거 승인 {value}")),
        candidate_set_sha256: fingerprint,
        evidence_complete: issues.is_empty(),
        issues,
    }
}

pub fn execute_colima_empty_volumes(
    executable: &Path,
    profile: &str,
    confirmation_phrase: &str,
    rationale: &str,
    executed_at_ms: u64,
) -> Result<ColimaEmptyVolumeExecution, String> {
    if executed_at_ms == 0 {
        return Err("colima-empty-volume-time-invalid".into());
    }
    if rationale.trim() != rationale
        || rationale.is_empty()
        || rationale.chars().count() > 1_000
        || rationale.chars().any(char::is_control)
    {
        return Err("colima-empty-volume-rationale-invalid".into());
    }
    let plan = plan_colima_empty_volumes(executable, profile, Duration::from_secs(10));
    if !plan.evidence_complete {
        return Err("colima-empty-volume-evidence-incomplete".into());
    }
    let fingerprint = plan
        .candidate_set_sha256
        .ok_or("colima-empty-volume-candidate-empty")?;
    if confirmation_phrase != plan.exact_approval_phrase.as_deref().unwrap_or_default() {
        return Err("colima-empty-volume-confirmation-mismatch".into());
    }
    let (_, candidates) =
        current_empty_dangling_volumes(executable, profile, Duration::from_secs(10))?;
    if digest_values(
        b"disksage.colima-empty-dangling-volumes/v1",
        candidates.iter(),
    ) != fingerprint
    {
        return Err("colima-empty-volume-candidate-set-changed".into());
    }
    let mut arguments = vec![
        "--profile".to_string(),
        profile.to_string(),
        "ssh".into(),
        "--".into(),
        "docker".into(),
        "volume".into(),
        "rm".into(),
    ];
    arguments.extend(candidates.iter().cloned());
    let references = arguments.iter().map(String::as_str).collect::<Vec<_>>();
    let output = run_command(
        executable,
        &references,
        Duration::from_secs(30),
        "colima-empty-volume-remove",
    )?;
    let (_, after) = current_empty_dangling_volumes(executable, profile, Duration::from_secs(10))?;
    let remaining = candidates
        .iter()
        .filter(|name| after.contains(name))
        .count();
    Ok(ColimaEmptyVolumeExecution {
        schema_kind: "disksage.colima-empty-volume-execution",
        schema_version: 1,
        ontology_class: "https://disksage.app/ontology#ColimaEmptyVolume",
        profile: profile.into(),
        candidate_set_sha256: fingerprint,
        before_candidate_count: candidates.len(),
        after_candidate_count: after.len(),
        observed_removed_count: candidates.len().saturating_sub(remaining),
        status_code: output.status_code,
        executed_at_ms,
        rationale: rationale.into(),
    })
}

pub fn plan_colima_guest_trim(
    executable: &Path,
    profile: &str,
    timeout: Duration,
) -> ColimaGuestTrimPlan {
    let evidence = running_profile(executable, profile, timeout);
    let mut blockers = vec!["lima-native-host-compaction-command-unavailable".into()];
    let configured_disk_bytes = evidence
        .as_ref()
        .map(|candidate| candidate.configured_disk_bytes)
        .unwrap_or(0);
    if let Err(error) = &evidence {
        blockers.push(error.clone());
    }
    let fingerprint = evidence.ok().map(|_| {
        let disk = configured_disk_bytes.to_be_bytes();
        digest_values(
            b"disksage.colima-guest-trim/v1",
            [profile.as_bytes(), disk.as_slice()],
        )
    });
    ColimaGuestTrimPlan {
        schema_kind: "disksage.colima-guest-trim-plan",
        schema_version: 1,
        ontology_class: "https://disksage.app/ontology#ColimaGuestTrim",
        profile: profile.into(),
        configured_disk_bytes,
        exact_approval_phrase: fingerprint
            .as_ref()
            .map(|value| format!("DiskSage Colima guest trim 승인 {value}")),
        candidate_fingerprint: fingerprint.clone(),
        guest_trim_eligible: fingerprint.is_some(),
        native_host_compaction_supported: false,
        blockers,
    }
}

pub fn execute_colima_guest_trim(
    executable: &Path,
    profile: &str,
    confirmation_phrase: &str,
    rationale: &str,
    executed_at_ms: u64,
) -> Result<ColimaGuestTrimExecution, String> {
    if executed_at_ms == 0 {
        return Err("colima-guest-trim-time-invalid".into());
    }
    if rationale.trim() != rationale
        || rationale.is_empty()
        || rationale.chars().count() > 1_000
        || rationale.chars().any(char::is_control)
    {
        return Err("colima-guest-trim-rationale-invalid".into());
    }
    let plan = plan_colima_guest_trim(executable, profile, Duration::from_secs(10));
    let fingerprint = plan
        .candidate_fingerprint
        .ok_or("colima-guest-trim-evidence-incomplete")?;
    if confirmation_phrase != plan.exact_approval_phrase.as_deref().unwrap_or_default() {
        return Err("colima-guest-trim-confirmation-mismatch".into());
    }
    let before = std::env::current_dir()
        .ok()
        .and_then(|path| crate::volume_pressure::snapshot_volume(&path, executed_at_ms).ok())
        .map(|snapshot| snapshot.available_bytes);
    let output = run_command(
        executable,
        &[
            "--profile",
            profile,
            "ssh",
            "--",
            "sudo",
            "fstrim",
            "--all",
            "--verbose",
        ],
        Duration::from_secs(60),
        "colima-guest-trim",
    )?;
    let after_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(executed_at_ms);
    let after = std::env::current_dir()
        .ok()
        .and_then(|path| crate::volume_pressure::snapshot_volume(&path, after_time).ok())
        .map(|snapshot| snapshot.available_bytes);
    Ok(ColimaGuestTrimExecution {
        schema_kind: "disksage.colima-guest-trim-execution",
        schema_version: 1,
        ontology_class: "https://disksage.app/ontology#ColimaGuestTrim",
        profile: profile.into(),
        candidate_fingerprint: fingerprint,
        status_code: output.status_code,
        executed: output.status_code == 0,
        before_available_bytes: before,
        after_available_bytes: after,
        observed_available_gain_bytes: before
            .zip(after)
            .and_then(|(before, after)| after.checked_sub(before)),
        executed_at_ms,
        rationale: rationale.into(),
    })
}

pub fn execute_colima_dangling_images(
    executable: &Path,
    profile: &str,
    confirmation_phrase: &str,
    rationale: &str,
    executed_at_ms: u64,
) -> Result<ColimaDanglingImageExecution, String> {
    if executed_at_ms == 0 {
        return Err("colima-dangling-image-time-invalid".into());
    }
    if rationale.trim() != rationale
        || rationale.is_empty()
        || rationale.chars().count() > 1_000
        || rationale.chars().any(char::is_control)
    {
        return Err("colima-dangling-image-rationale-invalid".into());
    }
    let plan = plan_colima_dangling_images(executable, profile, Duration::from_secs(10));
    if !plan.evidence_complete {
        return Err("colima-dangling-image-evidence-incomplete".into());
    }
    let fingerprint = plan
        .candidate_set_sha256
        .ok_or("colima-dangling-image-candidate-empty")?;
    if confirmation_phrase != plan.exact_approval_phrase.as_deref().unwrap_or_default() {
        return Err("colima-dangling-image-confirmation-mismatch".into());
    }
    let ids = current_dangling_image_ids(executable, profile, Duration::from_secs(10))?;
    if digest_values(b"disksage.colima-dangling-images/v1", ids.iter()) != fingerprint {
        return Err("colima-dangling-image-candidate-set-changed".into());
    }
    let mut arguments = vec![
        "--profile".to_string(),
        profile.to_string(),
        "ssh".into(),
        "--".into(),
        "docker".into(),
        "image".into(),
        "rm".into(),
    ];
    arguments.extend(ids.iter().map(|id| format!("sha256:{id}")));
    let references = arguments.iter().map(String::as_str).collect::<Vec<_>>();
    let output = run_command(
        executable,
        &references,
        Duration::from_secs(30),
        "colima-dangling-image-remove",
    )?;
    let after = current_dangling_image_ids(executable, profile, Duration::from_secs(10))?;
    let remaining_authorized = ids.iter().filter(|id| after.contains(id)).count();
    Ok(ColimaDanglingImageExecution {
        schema_kind: "disksage.colima-dangling-image-execution",
        schema_version: 1,
        ontology_class: "https://disksage.app/ontology#ColimaDanglingImage",
        profile: profile.into(),
        candidate_set_sha256: fingerprint,
        before_candidate_count: ids.len(),
        after_candidate_count: after.len(),
        observed_removed_count: ids.len().saturating_sub(remaining_authorized),
        status_code: output.status_code,
        executed_at_ms,
        rationale: rationale.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_state_fails_closed_for_compaction() {
        let profiles = parse_profiles(
            "{\"name\":\"default\",\"status\":\"Running\",\"runtime\":\"docker\",\"disk\":107374182400}\n{\"name\":\"old\",\"status\":\"Stopped\",\"runtime\":\"containerd\",\"disk\":53687091200}\n",
        )
        .unwrap();
        assert_eq!(profiles.len(), 2);
        assert!(profiles.iter().all(|profile| !profile.compaction_eligible));
        assert!(profiles[0]
            .blockers
            .contains(&"virtual-machine-running".into()));
        assert_eq!(
            profiles[1].blockers,
            vec!["guest-free-space-trim-not-attested"]
        );
    }

    #[test]
    fn missing_runtime_is_explicit_and_cache_is_measured() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("asset"), vec![1u8; 8192]).unwrap();
        let plan = plan_colima_reclaim(
            &temp.path().join("missing-colima"),
            temp.path(),
            Duration::from_secs(1),
        );
        assert!(!plan.executable_available);
        assert!(!plan.evidence_complete);
        assert!(plan.cache_allocated_bytes > 0);
        assert_eq!(plan.issues, vec!["colima-executable-unavailable"]);
    }

    #[cfg(unix)]
    #[test]
    fn cache_prune_requires_fresh_exact_approval() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let cache = temp.path().join("cache");
        fs::create_dir(&cache).unwrap();
        fs::write(cache.join("asset"), vec![1u8; 8192]).unwrap();
        let executable = temp.path().join("colima");
        fs::write(
            &executable,
            format!(
                "#!/bin/sh\nif [ \"$1\" = list ]; then exit 0; fi\nif [ \"$1\" = prune ] && [ \"$2\" = --force ]; then rm -rf '{}'; exit 0; fi\nexit 2\n",
                cache.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        let plan = plan_colima_reclaim(&executable, &cache, Duration::from_secs(1));
        assert!(plan.evidence_complete);
        assert!(execute_colima_cache_prune(&executable, &cache, "wrong", "reviewed", 1).is_err());
        let execution = execute_colima_cache_prune(
            &executable,
            &cache,
            plan.cache_prune_approval_phrase.as_deref().unwrap(),
            "reviewed",
            1,
        )
        .unwrap();
        assert!(execution.executed);
        assert!(execution.observed_allocation_reduction_bytes > 0);
        assert_eq!(execution.after_cache_allocated_bytes, 0);
    }

    #[cfg(unix)]
    #[test]
    fn dangling_images_remove_only_fingerprinted_ids() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let marker = temp.path().join("present");
        fs::write(&marker, b"1").unwrap();
        let executable = temp.path().join("colima");
        let id = "a".repeat(64);
        fs::write(
            &executable,
            format!(
                "#!/bin/sh\nif [ \"$1\" = list ]; then echo '{{\"name\":\"default\",\"status\":\"Running\",\"runtime\":\"docker\",\"disk\":100}}'; exit 0; fi\nif [ \"$7\" = ls ]; then [ -f '{}' ] && echo 'sha256:{}'; exit 0; fi\nif [ \"$7\" = rm ] && [ \"$8\" = 'sha256:{}' ]; then rm -f '{}'; exit 0; fi\nexit 2\n",
                marker.display(), id, id, marker.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        let plan = plan_colima_dangling_images(&executable, "default", Duration::from_secs(1));
        assert!(plan.evidence_complete);
        assert_eq!(plan.candidate_count, 1);
        assert!(
            execute_colima_dangling_images(&executable, "default", "wrong", "reviewed", 1).is_err()
        );
        let execution = execute_colima_dangling_images(
            &executable,
            "default",
            plan.exact_approval_phrase.as_deref().unwrap(),
            "reviewed",
            1,
        )
        .unwrap();
        assert_eq!(execution.observed_removed_count, 1);
        assert_eq!(execution.after_candidate_count, 0);
    }

    #[cfg(unix)]
    #[test]
    fn empty_volumes_require_dangling_and_empty_evidence() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let marker = temp.path().join("present");
        fs::write(&marker, b"1").unwrap();
        let executable = temp.path().join("colima");
        fs::write(
            &executable,
            format!(
                "#!/bin/sh\nif [ \"$1\" = list ]; then echo '{{\"name\":\"default\",\"status\":\"Running\",\"runtime\":\"docker\",\"disk\":100}}'; exit 0; fi\nif [ \"$7\" = ls ]; then [ -f '{}' ] && echo 'empty_volume'; exit 0; fi\nif [ \"$7\" = inspect ]; then echo 'empty_volume|/var/lib/docker/volumes/empty_volume/_data'; exit 0; fi\nif [ \"$6\" = find ]; then exit 0; fi\nif [ \"$7\" = rm ] && [ \"$8\" = empty_volume ]; then rm -f '{}'; exit 0; fi\nexit 2\n",
                marker.display(), marker.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        let plan = plan_colima_empty_volumes(&executable, "default", Duration::from_secs(1));
        assert!(plan.evidence_complete);
        assert_eq!(plan.dangling_count, 1);
        assert_eq!(plan.empty_candidate_count, 1);
        assert!(
            execute_colima_empty_volumes(&executable, "default", "wrong", "reviewed", 1).is_err()
        );
        let execution = execute_colima_empty_volumes(
            &executable,
            "default",
            plan.exact_approval_phrase.as_deref().unwrap(),
            "reviewed",
            1,
        )
        .unwrap();
        assert_eq!(execution.observed_removed_count, 1);
        assert_eq!(execution.after_candidate_count, 0);
    }

    #[cfg(unix)]
    #[test]
    fn guest_trim_requires_running_profile_and_exact_approval() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let executable = temp.path().join("colima");
        fs::write(
            &executable,
            "#!/bin/sh\nif [ \"$1\" = list ]; then echo '{\"name\":\"default\",\"status\":\"Running\",\"runtime\":\"docker\",\"disk\":100}'; exit 0; fi\nif [ \"$6\" = fstrim ] && [ \"$7\" = --all ] && [ \"$8\" = --verbose ]; then exit 0; fi\nexit 2\n",
        )
        .unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        let plan = plan_colima_guest_trim(&executable, "default", Duration::from_secs(1));
        assert!(plan.guest_trim_eligible);
        assert!(!plan.native_host_compaction_supported);
        assert!(plan
            .blockers
            .contains(&"lima-native-host-compaction-command-unavailable".into()));
        assert!(execute_colima_guest_trim(&executable, "default", "wrong", "reviewed", 1).is_err());
        let execution = execute_colima_guest_trim(
            &executable,
            "default",
            plan.exact_approval_phrase.as_deref().unwrap(),
            "reviewed",
            1,
        )
        .unwrap();
        assert!(execution.executed);
    }
}
