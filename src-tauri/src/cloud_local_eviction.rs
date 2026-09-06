//! Evidence-bound removal of local iCloud bytes while retaining the cloud object.
//!
//! Planning is read-only and never opens file content. Execution is macOS-only, requires a
//! fingerprint-bound human approval, revalidates native iCloud state and active handles, calls
//! Foundation's local-only ubiquitous-item eviction API, and reports allocation reduction
//! separately from the API request. Modern macOS File Provider status is observed through a
//! bounded, read-only diagnostic command because ubiquitous URL resource values are absent for
//! some File Provider-backed iCloud paths.

use crate::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::Metadata;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const ICLOUD_LOCAL_EVICTION_VERSION: u32 = 2;
const ACTIVE_USE_TIMEOUT_MS: u64 = 5_000;
const MAX_ACTIVE_USE_OUTPUT_BYTES: u64 = 2 * 1024 * 1024;
const MAX_ACTIVE_PIDS: usize = 64;
const MAX_RATIONALE_BYTES: usize = 1_024;
const POST_EVICTION_WAIT_MS: u64 = 5_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IcloudStateObservationMethod {
    FileProviderCtlEvaluate,
    FoundationUbiquitousResourceValues,
}

impl IcloudStateObservationMethod {
    fn as_str(self) -> &'static str {
        match self {
            Self::FileProviderCtlEvaluate => "fileproviderctl-evaluate",
            Self::FoundationUbiquitousResourceValues => "foundation-ubiquitous-resource-values",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IcloudLocalState {
    pub observation_method: IcloudStateObservationMethod,
    pub is_ubiquitous: bool,
    pub is_uploaded: bool,
    pub is_uploading: bool,
    pub is_downloading: bool,
    pub downloading_status_current: bool,
    pub has_unresolved_conflicts: bool,
    pub is_excluded_from_sync: bool,
    pub is_sync_paused: Option<bool>,
    pub is_trashed: Option<bool>,
    pub allows_eviction: Option<bool>,
    pub provider_reported_bytes: Option<u64>,
    pub item_identifier_fingerprint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActiveUseEvidence {
    pub method: String,
    pub evidence_complete: bool,
    pub active: bool,
    pub observed_pids: Vec<u32>,
    pub results_truncated: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IcloudLocalEvictionPlan {
    pub version: u32,
    pub provider: CloudProvider,
    pub account_scope: CloudAccountScope,
    pub cloud_root: String,
    pub path: String,
    pub logical_bytes: u64,
    pub allocated_bytes: u64,
    pub filesystem_modified_ms: u64,
    pub observed_at_ms: u64,
    pub icloud_state: IcloudLocalState,
    pub active_use: ActiveUseEvidence,
    pub plan_fingerprint: String,
    pub eligible_after_human_approval: bool,
    pub blockers: Vec<String>,
    pub notices: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IcloudLocalEvictionApproval {
    pub version: u32,
    pub approval_id: String,
    pub plan_fingerprint: String,
    pub approved_at_ms: u64,
    pub approved_by: String,
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IcloudLocalEvictionResult {
    pub version: u32,
    pub result_id: String,
    pub plan_fingerprint: String,
    pub approval_id: String,
    pub path: String,
    pub requested_at_ms: u64,
    pub allocated_bytes_before: u64,
    pub allocated_bytes_after: u64,
    pub observed_allocation_reduction_bytes: u64,
    pub eviction_request_succeeded: bool,
    pub cloud_item_path_retained: bool,
    pub is_ubiquitous_after: bool,
    pub local_allocation_reduction_verified: bool,
    pub verification_complete: bool,
    pub verification_blockers: Vec<String>,
    pub notices: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LocalFileObservation {
    logical_bytes: u64,
    allocated_bytes: u64,
    modified_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PostEvictionObservation {
    path_retained: bool,
    is_ubiquitous: bool,
    allocated_bytes: u64,
}

fn valid_hex64(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn absolute_without_parent(path: &Path) -> bool {
    path.is_absolute()
        && !path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
}

fn system_time_ms(value: std::io::Result<SystemTime>) -> u64 {
    value
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .unwrap_or_default()
}

#[cfg(unix)]
fn allocated_bytes(metadata: &Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    metadata.blocks().saturating_mul(512)
}

#[cfg(windows)]
fn allocated_bytes(metadata: &Metadata) -> u64 {
    use std::os::windows::fs::MetadataExt;
    metadata.file_size()
}

#[cfg(not(any(unix, windows)))]
fn allocated_bytes(metadata: &Metadata) -> u64 {
    metadata.len()
}

fn observe_local_file(root: &CloudRoot, path: &Path) -> Result<LocalFileObservation, String> {
    if crate::safety::agent_state_guard::is_agent_state(path) {
        return Err("icloud-local-eviction-agent-state-protected".into());
    }
    if root.provider != CloudProvider::Icloud {
        return Err("icloud-local-eviction-requires-icloud-root".into());
    }
    let root_path = Path::new(&root.path);
    if !absolute_without_parent(root_path) || !absolute_without_parent(path) {
        return Err("icloud-local-eviction-path-not-safe-absolute".into());
    }
    let relative = path
        .strip_prefix(root_path)
        .map_err(|_| "icloud-local-eviction-path-outside-root".to_string())?;
    if relative.as_os_str().is_empty() {
        return Err("icloud-local-eviction-root-not-file".into());
    }

    let root_metadata =
        std::fs::symlink_metadata(root_path).map_err(|_| "icloud-root-unavailable".to_string())?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err("icloud-root-not-real-directory".into());
    }

    let mut current = PathBuf::from(root_path);
    for component in relative.components() {
        let std::path::Component::Normal(segment) = component else {
            return Err("icloud-local-eviction-path-not-safe".into());
        };
        current.push(segment);
        let metadata = std::fs::symlink_metadata(&current)
            .map_err(|_| "icloud-local-eviction-path-unavailable".to_string())?;
        if metadata.file_type().is_symlink() {
            return Err("icloud-local-eviction-symlink-rejected".into());
        }
    }

    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| "icloud-local-eviction-path-unavailable".to_string())?;
    if !metadata.is_file() {
        return Err("icloud-local-eviction-path-not-regular-file".into());
    }
    Ok(LocalFileObservation {
        logical_bytes: metadata.len(),
        allocated_bytes: allocated_bytes(&metadata),
        modified_ms: system_time_ms(metadata.modified()),
    })
}

fn push_unique(values: &mut Vec<String>, value: &str) {
    if !values.iter().any(|existing| existing == value) {
        values.push(value.into());
    }
}

fn hash_bool(hasher: &mut blake3::Hasher, value: bool) {
    hasher.update(&[u8::from(value)]);
}

fn hash_optional_bool(hasher: &mut blake3::Hasher, value: Option<bool>) {
    match value {
        None => hasher.update(&[0]),
        Some(false) => hasher.update(&[1]),
        Some(true) => hasher.update(&[2]),
    };
}

fn hash_optional_u64(hasher: &mut blake3::Hasher, value: Option<u64>) {
    match value {
        None => {
            hasher.update(&[0]);
        }
        Some(value) => {
            hasher.update(&[1]);
            hasher.update(&value.to_le_bytes());
        }
    };
}

fn hash_optional_string(hasher: &mut blake3::Hasher, value: Option<&str>) {
    match value {
        None => {
            hasher.update(&[0]);
        }
        Some(value) => {
            hasher.update(&[1]);
            hasher.update(value.as_bytes());
            hasher.update(&[0]);
        }
    }
}

fn plan_fingerprint(
    root: &CloudRoot,
    path: &Path,
    file: &LocalFileObservation,
    state: &IcloudLocalState,
    active_use: &ActiveUseEvidence,
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-icloud-local-eviction-plan-v2\0");
    for value in [
        root.id.as_bytes(),
        root.provider.as_str().as_bytes(),
        root.account_scope.as_str().as_bytes(),
        root.path.as_bytes(),
        path.to_string_lossy().as_bytes(),
        state.observation_method.as_str().as_bytes(),
        active_use.method.as_bytes(),
    ] {
        hasher.update(value);
        hasher.update(&[0]);
    }
    for value in [file.logical_bytes, file.allocated_bytes, file.modified_ms] {
        hasher.update(&value.to_le_bytes());
    }
    for value in [
        state.is_ubiquitous,
        state.is_uploaded,
        state.is_uploading,
        state.is_downloading,
        state.downloading_status_current,
        state.has_unresolved_conflicts,
        state.is_excluded_from_sync,
        active_use.evidence_complete,
        active_use.active,
        active_use.results_truncated,
    ] {
        hash_bool(&mut hasher, value);
    }
    for value in [
        state.is_sync_paused,
        state.is_trashed,
        state.allows_eviction,
    ] {
        hash_optional_bool(&mut hasher, value);
    }
    hash_optional_u64(&mut hasher, state.provider_reported_bytes);
    hash_optional_string(&mut hasher, state.item_identifier_fingerprint.as_deref());
    for pid in &active_use.observed_pids {
        hasher.update(&pid.to_le_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

fn build_plan(
    root: &CloudRoot,
    path: &Path,
    file: LocalFileObservation,
    state: IcloudLocalState,
    active_use: ActiveUseEvidence,
    observed_at_ms: u64,
) -> IcloudLocalEvictionPlan {
    let mut blockers = Vec::new();
    if file.allocated_bytes == 0 {
        push_unique(&mut blockers, "icloud-local-copy-not-allocated");
    }
    if !state.is_ubiquitous {
        push_unique(&mut blockers, "icloud-item-not-ubiquitous");
    }
    if state.observation_method == IcloudStateObservationMethod::FoundationUbiquitousResourceValues
    {
        push_unique(
            &mut blockers,
            "icloud-file-provider-native-status-unavailable",
        );
    }
    if !state.is_uploaded {
        push_unique(&mut blockers, "icloud-upload-not-confirmed");
    }
    if state.is_uploading {
        push_unique(&mut blockers, "icloud-upload-still-running");
    }
    if state.is_downloading {
        push_unique(&mut blockers, "icloud-download-running");
    }
    if !state.downloading_status_current {
        push_unique(&mut blockers, "icloud-current-version-unconfirmed");
    }
    if state.has_unresolved_conflicts {
        push_unique(&mut blockers, "icloud-unresolved-conflict");
    }
    if state.is_excluded_from_sync {
        push_unique(&mut blockers, "icloud-item-excluded-from-sync");
    }
    if state.observation_method == IcloudStateObservationMethod::FileProviderCtlEvaluate {
        if state.is_sync_paused != Some(false) {
            push_unique(
                &mut blockers,
                "icloud-file-provider-sync-paused-or-unconfirmed",
            );
        }
        if state.is_trashed != Some(false) {
            push_unique(
                &mut blockers,
                "icloud-file-provider-item-trashed-or-unconfirmed",
            );
        }
        if state.allows_eviction != Some(true) {
            push_unique(
                &mut blockers,
                "icloud-file-provider-eviction-capability-unconfirmed",
            );
        }
        if state.provider_reported_bytes != Some(file.logical_bytes) {
            push_unique(&mut blockers, "icloud-file-provider-document-size-mismatch");
        }
        if !state
            .item_identifier_fingerprint
            .as_deref()
            .is_some_and(valid_hex64)
        {
            push_unique(
                &mut blockers,
                "icloud-file-provider-item-identity-unconfirmed",
            );
        }
    }
    if !active_use.evidence_complete {
        push_unique(&mut blockers, "active-use-evidence-incomplete");
    }
    if active_use.active {
        push_unique(&mut blockers, "active-file-use-detected");
    }
    let eligible_after_human_approval = blockers.is_empty();
    push_unique(&mut blockers, "human-local-eviction-approval-required");
    let fingerprint = plan_fingerprint(root, path, &file, &state, &active_use);
    IcloudLocalEvictionPlan {
        version: ICLOUD_LOCAL_EVICTION_VERSION,
        provider: root.provider,
        account_scope: root.account_scope,
        cloud_root: root.path.clone(),
        path: path.to_string_lossy().into_owned(),
        logical_bytes: file.logical_bytes,
        allocated_bytes: file.allocated_bytes,
        filesystem_modified_ms: file.modified_ms,
        observed_at_ms,
        icloud_state: state,
        active_use,
        plan_fingerprint: fingerprint,
        eligible_after_human_approval,
        blockers,
        notices: vec![
            "file-content-not-opened".into(),
            "embedded-metadata-not-required-for-local-cache-eviction".into(),
            "cloud-object-must-remain-present".into(),
            "allocated-byte-reduction-is-not-volume-free-space-proof".into(),
        ],
    }
}

fn drain_bounded<R: Read + Send + 'static>(
    reader: R,
) -> std::thread::JoinHandle<Result<Vec<u8>, String>> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        reader
            .take(MAX_ACTIVE_USE_OUTPUT_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "active-use-output-read-failed".to_string())?;
        Ok(bytes)
    })
}

#[cfg(all(unix, not(coverage)))]
fn lsof_stderr_is_benign(stderr: &[u8], target: &Path) -> bool {
    let Ok(text) = std::str::from_utf8(stderr) else {
        return false;
    };
    let target = target.to_string_lossy();
    let mut warning_seen = false;
    text.lines().all(|line| {
        let line = line.trim();
        if line.is_empty() {
            return true;
        }
        if line.starts_with("lsof: WARNING:") {
            warning_seen = true;
            return !line.contains(target.as_ref());
        }
        warning_seen && line == "Output information may be incomplete."
    })
}

#[cfg(all(unix, not(coverage)))]
fn observe_lsof_active_use(path: &Path, deadline: Instant) -> ActiveUseEvidence {
    let mut command = Command::new("lsof");
    command.arg("-F").arg("p");
    if path.is_dir() {
        // `lsof PATH` only proves the directory itself is referenced. Recursive +D is required
        // for a cache directory whose open files live below the directory entry.
        command.arg("+D");
    }
    // A bounded lsof probe may be a shell wrapper in tests or on user systems. Kill its process
    // group on timeout so descendants cannot keep the stdout pipe alive and starve the ps probe.
    unsafe {
        use std::os::unix::process::CommandExt;
        command.pre_exec(|| {
            if libc::setpgid(0, 0) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = match command
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => {
            return ActiveUseEvidence {
                method: "lsof-fp".into(),
                evidence_complete: false,
                active: false,
                observed_pids: Vec::new(),
                results_truncated: false,
                error: Some("active-use-lsof-unavailable".into()),
            }
        }
    };
    let Some(stdout) = child.stdout.take() else {
        return ActiveUseEvidence {
            method: "lsof-fp+ps-command".into(),
            evidence_complete: false,
            active: false,
            observed_pids: Vec::new(),
            results_truncated: false,
            error: Some("active-use-output-missing".into()),
        };
    };
    let Some(stderr) = child.stderr.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return ActiveUseEvidence {
            method: "lsof-fp+ps-command".into(),
            evidence_complete: false,
            active: false,
            observed_pids: Vec::new(),
            results_truncated: false,
            error: Some("active-use-error-output-missing".into()),
        };
    };
    let reader = drain_bounded(stdout);
    let error_reader = drain_bounded(stderr);
    let child_pid = child.id();
    let kill_group = || unsafe {
        let _ = libc::kill(-(child_pid as libc::pid_t), libc::SIGKILL);
    };
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(25));
            }
            Ok(None) => {
                kill_group();
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            Err(_) => break None,
        }
    };
    let output = reader.join().ok().and_then(Result::ok).unwrap_or_default();
    let error_output = error_reader
        .join()
        .ok()
        .and_then(Result::ok)
        .unwrap_or_default();
    if status.is_none() {
        return ActiveUseEvidence {
            method: "lsof-fp".into(),
            evidence_complete: false,
            active: false,
            observed_pids: Vec::new(),
            results_truncated: output.len() as u64 > MAX_ACTIVE_USE_OUTPUT_BYTES,
            error: Some("active-use-check-timeout-or-wait-failed".into()),
        };
    }
    let results_truncated = output.len() as u64 > MAX_ACTIVE_USE_OUTPUT_BYTES
        || error_output.len() as u64 > MAX_ACTIVE_USE_OUTPUT_BYTES;
    if results_truncated {
        return ActiveUseEvidence {
            method: "lsof-fp".into(),
            evidence_complete: false,
            active: false,
            observed_pids: Vec::new(),
            results_truncated: true,
            error: Some("active-use-output-truncated".into()),
        };
    }
    let text = String::from_utf8_lossy(&output);
    let mut pids: Vec<u32> = text
        .lines()
        .filter_map(|line| line.strip_prefix('p')?.parse().ok())
        .collect();
    pids.sort_unstable();
    pids.dedup();
    let pid_results_truncated = pids.len() > MAX_ACTIVE_PIDS;
    if pid_results_truncated {
        pids.truncate(MAX_ACTIVE_PIDS);
    }
    let success = status.is_some_and(|value| value.success());
    let no_matches = status.and_then(|value| value.code()) == Some(1) && pids.is_empty();
    let stderr_benign = lsof_stderr_is_benign(&error_output, path);
    let evidence_complete = stderr_benign && (success || no_matches);
    ActiveUseEvidence {
        method: "lsof-fp".into(),
        evidence_complete,
        active: !pids.is_empty(),
        observed_pids: pids,
        results_truncated: pid_results_truncated,
        error: (!evidence_complete).then(|| "active-use-lsof-status-unexpected".into()),
    }
}

#[cfg(all(unix, not(coverage)))]
fn process_command_matches_target(command: &str, path: &Path) -> bool {
    let full_path = path.to_string_lossy();
    if command.contains(full_path.as_ref()) {
        return true;
    }
    let basename = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if basename.len() < 8 {
        return false;
    }
    let parent_and_name = path
        .parent()
        .and_then(Path::file_name)
        .and_then(|value| value.to_str())
        .map(|parent| format!("{parent}/{basename}"));
    parent_and_name
        .as_deref()
        .is_some_and(|relative| command.contains(relative))
        || command.contains(basename)
}

#[cfg(all(unix, not(coverage)))]
fn parse_process_command_references(output: &[u8], path: &Path, own_pid: u32) -> Vec<u32> {
    let text = String::from_utf8_lossy(output);
    let mut records = Vec::new();
    for line in text.lines() {
        let line = line.trim_start();
        let pid_end = line.find(char::is_whitespace).unwrap_or(line.len());
        let (pid_text, remainder) = line.split_at(pid_end);
        let Ok(pid) = pid_text.parse::<u32>() else {
            continue;
        };
        let remainder = remainder.trim_start();
        let parent_pid_end = remainder
            .find(char::is_whitespace)
            .unwrap_or(remainder.len());
        let (parent_pid_text, command) = remainder.split_at(parent_pid_end);
        let Ok(parent_pid) = parent_pid_text.parse::<u32>() else {
            continue;
        };
        records.push((pid, parent_pid, command.trim_start()));
    }

    // A watchdog or timeout wrapper commonly includes the full child command (and therefore the
    // target path) in its own argv. It supervises the planner but does not itself use the file.
    // Exclude the planner and its complete ancestor chain while retaining unrelated processes that
    // independently reference the same target.
    let parent_by_pid: BTreeMap<u32, u32> = records
        .iter()
        .map(|(pid, parent_pid, _)| (*pid, *parent_pid))
        .collect();
    let mut planner_lineage = BTreeSet::new();
    let mut lineage_pid = own_pid;
    while planner_lineage.insert(lineage_pid) {
        let Some(parent_pid) = parent_by_pid.get(&lineage_pid).copied() else {
            break;
        };
        if parent_pid == 0 || parent_pid == lineage_pid {
            break;
        }
        lineage_pid = parent_pid;
    }

    let mut pids: Vec<u32> = records
        .into_iter()
        .filter_map(|(pid, _, command)| {
            (!planner_lineage.contains(&pid) && process_command_matches_target(command, path))
                .then_some(pid)
        })
        .collect();
    pids.sort_unstable();
    pids.dedup();
    pids
}

#[cfg(all(unix, not(coverage)))]
fn observe_process_command_use(path: &Path, deadline: Instant) -> ActiveUseEvidence {
    let mut child = match Command::new("ps")
        .args(["-axo", "pid=,ppid=,command="])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => {
            return ActiveUseEvidence {
                method: "ps-command".into(),
                evidence_complete: false,
                active: false,
                observed_pids: Vec::new(),
                results_truncated: false,
                error: Some("active-use-ps-unavailable".into()),
            }
        }
    };
    let Some(stdout) = child.stdout.take() else {
        return ActiveUseEvidence {
            method: "ps-command".into(),
            evidence_complete: false,
            active: false,
            observed_pids: Vec::new(),
            results_truncated: false,
            error: Some("active-use-ps-output-missing".into()),
        };
    };
    let reader = drain_bounded(stdout);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(25));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            Err(_) => break None,
        }
    };
    let output = reader.join().ok().and_then(Result::ok).unwrap_or_default();
    let output_truncated = output.len() as u64 > MAX_ACTIVE_USE_OUTPUT_BYTES;
    let mut pids = parse_process_command_references(&output, path, std::process::id());
    let pid_results_truncated = pids.len() > MAX_ACTIVE_PIDS;
    if pid_results_truncated {
        pids.truncate(MAX_ACTIVE_PIDS);
    }
    let results_truncated = output_truncated || pid_results_truncated;
    let evidence_complete = status.is_some_and(|value| value.success()) && !results_truncated;
    ActiveUseEvidence {
        method: "ps-command".into(),
        evidence_complete,
        active: !pids.is_empty(),
        observed_pids: pids,
        results_truncated,
        error: (!evidence_complete).then(|| "active-use-ps-status-unexpected".into()),
    }
}

#[cfg(all(unix, not(coverage)))]
fn observe_active_use_until(path: &Path, deadline: Instant) -> ActiveUseEvidence {
    let started = Instant::now();
    let remaining = deadline.saturating_duration_since(started);
    let per_probe_budget = std::cmp::min(
        remaining / 2,
        Duration::from_millis(ACTIVE_USE_TIMEOUT_MS),
    );
    // Reserve an independent bounded slice for each source. A recursive `lsof +D` may consume its
    // entire allocation; it must not starve the process-command probe and hide an active PID.
    let lsof = observe_lsof_active_use(path, started + per_probe_budget);
    let ps_started = Instant::now();
    let process_commands = observe_process_command_use(
        path,
        std::cmp::min(deadline, ps_started + per_probe_budget),
    );
    let mut pids = lsof.observed_pids;
    pids.extend(process_commands.observed_pids);
    pids.sort_unstable();
    pids.dedup();
    let pid_results_truncated = pids.len() > MAX_ACTIVE_PIDS;
    if pid_results_truncated {
        pids.truncate(MAX_ACTIVE_PIDS);
    }
    let results_truncated =
        lsof.results_truncated || process_commands.results_truncated || pid_results_truncated;
    let evidence_complete =
        lsof.evidence_complete && process_commands.evidence_complete && !results_truncated;
    let error = [lsof.error, process_commands.error]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    ActiveUseEvidence {
        method: "lsof-fp+ps-command".into(),
        evidence_complete,
        active: !pids.is_empty(),
        observed_pids: pids,
        results_truncated,
        error: (!error.is_empty()).then(|| error.join(";")),
    }
}

#[cfg(any(not(unix), coverage))]
fn observe_active_use_until(_path: &Path, _deadline: Instant) -> ActiveUseEvidence {
    ActiveUseEvidence {
        method: "unsupported".into(),
        evidence_complete: false,
        active: false,
        observed_pids: Vec::new(),
        results_truncated: false,
        error: Some("active-use-check-unsupported-platform".into()),
    }
}

/// Reuse the same bounded open-handle and process-command evidence for any source whose local
/// bytes may be released. Unsupported or incomplete platforms remain explicit and fail closed at
/// the caller.
pub fn observe_path_active_use(path: &Path) -> ActiveUseEvidence {
    observe_active_use_until(
        path,
        Instant::now() + Duration::from_millis(
            ACTIVE_USE_TIMEOUT_MS.saturating_mul(2),
        ),
    )
}

/// Observe open handles without allowing the probe to outlive an enclosing plan deadline.
pub fn observe_path_active_use_until(path: &Path, deadline: Instant) -> ActiveUseEvidence {
    observe_active_use_until(path, deadline)
}

#[cfg(all(target_os = "macos", not(coverage)))]
fn foundation_bool_resource(
    url: &objc2_foundation::NSURL,
    key: &objc2_foundation::NSURLResourceKey,
) -> Result<bool, String> {
    use objc2::runtime::AnyObject;
    use objc2_foundation::NSNumber;

    let mut value: Option<objc2::rc::Retained<AnyObject>> = None;
    // SAFETY: Every caller passes a Foundation NSURL key documented as NSNumber-valued.
    unsafe { url.getResourceValue_forKey_error(&mut value, key) }
        .map_err(|error| error.localizedDescription().to_string())?;
    value
        .ok_or_else(|| "icloud-resource-value-missing".to_string())?
        .downcast::<NSNumber>()
        .map(|number| number.as_bool())
        .map_err(|_| "icloud-resource-value-not-boolean".to_string())
}

#[cfg(all(target_os = "macos", not(coverage)))]
fn foundation_string_resource(
    url: &objc2_foundation::NSURL,
    key: &objc2_foundation::NSURLResourceKey,
) -> Result<objc2::rc::Retained<objc2_foundation::NSString>, String> {
    use objc2::runtime::AnyObject;
    use objc2_foundation::NSString;

    let mut value: Option<objc2::rc::Retained<AnyObject>> = None;
    // SAFETY: The downloading-status resource key is documented as NSString-valued.
    unsafe { url.getResourceValue_forKey_error(&mut value, key) }
        .map_err(|error| error.localizedDescription().to_string())?;
    value
        .ok_or_else(|| "icloud-resource-value-missing".to_string())?
        .downcast::<NSString>()
        .map_err(|_| "icloud-resource-value-not-string".to_string())
}

#[cfg(all(target_os = "macos", not(coverage)))]
fn observe_foundation_icloud_state(path: &Path) -> Result<IcloudLocalState, String> {
    use objc2::rc::autoreleasepool;
    use objc2_foundation::{
        NSString, NSURLIsUbiquitousItemKey, NSURLUbiquitousItemDownloadingStatusCurrent,
        NSURLUbiquitousItemDownloadingStatusKey, NSURLUbiquitousItemHasUnresolvedConflictsKey,
        NSURLUbiquitousItemIsDownloadingKey, NSURLUbiquitousItemIsExcludedFromSyncKey,
        NSURLUbiquitousItemIsUploadedKey, NSURLUbiquitousItemIsUploadingKey, NSURL,
    };

    let path = path
        .to_str()
        .ok_or_else(|| "icloud-local-eviction-path-not-unicode".to_string())?;
    autoreleasepool(|_| {
        let url = NSURL::fileURLWithPath(&NSString::from_str(path));
        // SAFETY: These are Foundation-exported process-lifetime resource-key constants.
        unsafe {
            let status = foundation_string_resource(&url, NSURLUbiquitousItemDownloadingStatusKey)?;
            Ok(IcloudLocalState {
                observation_method:
                    IcloudStateObservationMethod::FoundationUbiquitousResourceValues,
                is_ubiquitous: foundation_bool_resource(&url, NSURLIsUbiquitousItemKey)?,
                is_uploaded: foundation_bool_resource(&url, NSURLUbiquitousItemIsUploadedKey)?,
                is_uploading: foundation_bool_resource(&url, NSURLUbiquitousItemIsUploadingKey)?,
                is_downloading: foundation_bool_resource(
                    &url,
                    NSURLUbiquitousItemIsDownloadingKey,
                )?,
                downloading_status_current: status
                    .isEqualToString(NSURLUbiquitousItemDownloadingStatusCurrent),
                has_unresolved_conflicts: foundation_bool_resource(
                    &url,
                    NSURLUbiquitousItemHasUnresolvedConflictsKey,
                )?,
                is_excluded_from_sync: foundation_bool_resource(
                    &url,
                    NSURLUbiquitousItemIsExcludedFromSyncKey,
                )?,
                is_sync_paused: None,
                is_trashed: None,
                allows_eviction: None,
                provider_reported_bytes: None,
                item_identifier_fingerprint: None,
            })
        }
    })
}

fn file_provider_icloud_state(
    is_ubiquitous: bool,
    status: &crate::provider_sync::FileProviderItemStatus,
) -> IcloudLocalState {
    IcloudLocalState {
        observation_method: IcloudStateObservationMethod::FileProviderCtlEvaluate,
        is_ubiquitous,
        is_uploaded: status.is_uploaded,
        is_uploading: status.is_uploading,
        is_downloading: status.is_downloading,
        downloading_status_current: status.is_local_current(),
        has_unresolved_conflicts: status.has_unresolved_conflicts,
        is_excluded_from_sync: status.is_excluded_from_sync,
        is_sync_paused: Some(status.is_sync_paused),
        is_trashed: Some(status.is_trashed),
        allows_eviction: Some(status.allows_eviction),
        provider_reported_bytes: Some(status.observed_bytes),
        item_identifier_fingerprint: Some(status.item_identifier_fingerprint.clone()),
    }
}

#[cfg(all(target_os = "macos", not(coverage)))]
fn observe_file_provider_icloud_state(
    path: &Path,
    observed_bytes: u64,
) -> Result<IcloudLocalState, String> {
    use objc2::rc::autoreleasepool;
    use objc2_foundation::{NSFileManager, NSString, NSURL};

    let path = path
        .to_str()
        .ok_or_else(|| "icloud-local-eviction-path-not-unicode".to_string())?;
    let is_ubiquitous = autoreleasepool(|_| {
        let url = NSURL::fileURLWithPath(&NSString::from_str(path));
        Ok::<bool, String>(NSFileManager::defaultManager().isUbiquitousItemAtURL(&url))
    })?;
    if !is_ubiquitous {
        return Err("icloud-item-not-ubiquitous".into());
    }
    let output = crate::provider_sync::file_providerctl_status(path)?;
    let status = crate::provider_sync::parse_file_providerctl_item_status(&output, observed_bytes)?;
    Ok(file_provider_icloud_state(is_ubiquitous, &status))
}

#[cfg(all(target_os = "macos", not(coverage)))]
fn observe_icloud_state(path: &Path, observed_bytes: u64) -> Result<IcloudLocalState, String> {
    match observe_file_provider_icloud_state(path, observed_bytes) {
        Ok(state) => Ok(state),
        Err(error) if file_provider_command_allows_foundation_fallback(&error) => {
            observe_foundation_icloud_state(path)
                .map_err(|_| "icloud-state-observation-unavailable".to_string())
        }
        Err(error) => Err(error),
    }
}

fn file_provider_command_allows_foundation_fallback(error: &str) -> bool {
    matches!(
        error,
        "file-provider-status-command-unavailable"
            | "file-provider-status-command-failed"
            | "file-provider-status-field-missing:hasUnresolvedConflicts"
            | "file-provider-status-field-missing:itemIdentifier"
    )
}

#[cfg(any(not(target_os = "macos"), coverage))]
fn observe_icloud_state(_path: &Path, _observed_bytes: u64) -> Result<IcloudLocalState, String> {
    Err("icloud-local-eviction-unsupported-platform".into())
}

/// Build a read-only, exact-path local eviction plan. File content is never opened.
#[cfg(not(coverage))]
pub fn plan_icloud_local_eviction(
    root: &CloudRoot,
    path: &Path,
    observed_at_ms: u64,
) -> Result<IcloudLocalEvictionPlan, String> {
    let file = observe_local_file(root, path)?;
    let state = observe_icloud_state(path, file.logical_bytes)?;
    let active_use = observe_path_active_use(path);
    Ok(build_plan(
        root,
        path,
        file,
        state,
        active_use,
        observed_at_ms,
    ))
}

fn approval_id_for(
    plan_fingerprint: &str,
    approved_at_ms: u64,
    approved_by: &str,
    rationale: &str,
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-icloud-local-eviction-approval-v1\0");
    for value in [
        plan_fingerprint.as_bytes(),
        approved_by.as_bytes(),
        rationale.as_bytes(),
    ] {
        hasher.update(value);
        hasher.update(&[0]);
    }
    hasher.update(&approved_at_ms.to_le_bytes());
    hasher.finalize().to_hex().to_string()
}

/// Bind a human decision to one exact eligible plan. This function performs no eviction.
pub fn approve_icloud_local_eviction(
    plan: &IcloudLocalEvictionPlan,
    approved_plan_fingerprint: &str,
    approved_at_ms: u64,
    approved_by: &str,
    rationale: &str,
) -> Result<IcloudLocalEvictionApproval, String> {
    if plan.version != ICLOUD_LOCAL_EVICTION_VERSION
        || !valid_hex64(&plan.plan_fingerprint)
        || plan.plan_fingerprint != approved_plan_fingerprint
    {
        return Err("icloud-local-eviction-plan-fingerprint-mismatch".into());
    }
    if !plan.eligible_after_human_approval
        || plan
            .blockers
            .iter()
            .any(|blocker| blocker != "human-local-eviction-approval-required")
    {
        return Err("icloud-local-eviction-plan-not-eligible".into());
    }
    let reviewer = approved_by.trim();
    if !reviewer.starts_with("human:") || reviewer.len() <= "human:".len() {
        return Err("icloud-local-eviction-human-attribution-required".into());
    }
    let rationale = rationale.trim();
    if rationale.is_empty() || rationale.len() > MAX_RATIONALE_BYTES {
        return Err("icloud-local-eviction-rationale-invalid".into());
    }
    if approved_at_ms < plan.observed_at_ms {
        return Err("icloud-local-eviction-approval-predates-plan".into());
    }
    Ok(IcloudLocalEvictionApproval {
        version: ICLOUD_LOCAL_EVICTION_VERSION,
        approval_id: approval_id_for(&plan.plan_fingerprint, approved_at_ms, reviewer, rationale),
        plan_fingerprint: plan.plan_fingerprint.clone(),
        approved_at_ms,
        approved_by: reviewer.into(),
        rationale: rationale.into(),
    })
}

fn validate_approval(
    plan: &IcloudLocalEvictionPlan,
    approval: &IcloudLocalEvictionApproval,
    confirmation_plan_fingerprint: &str,
) -> Result<(), String> {
    if approval.version != ICLOUD_LOCAL_EVICTION_VERSION
        || approval.plan_fingerprint != plan.plan_fingerprint
        || approval.plan_fingerprint != confirmation_plan_fingerprint
        || approval.approval_id
            != approval_id_for(
                &approval.plan_fingerprint,
                approval.approved_at_ms,
                &approval.approved_by,
                &approval.rationale,
            )
    {
        return Err("icloud-local-eviction-approval-integrity-mismatch".into());
    }
    if approval.approved_at_ms < plan.observed_at_ms
        || !approval.approved_by.starts_with("human:")
        || approval.rationale.trim().is_empty()
    {
        return Err("icloud-local-eviction-approval-invalid".into());
    }
    Ok(())
}

#[cfg(all(target_os = "macos", not(coverage)))]
fn request_native_icloud_eviction(path: &Path) -> Result<(), String> {
    use objc2::rc::autoreleasepool;
    use objc2_foundation::{NSFileManager, NSString, NSURL};

    let path = path
        .to_str()
        .ok_or_else(|| "icloud-local-eviction-path-not-unicode".to_string())?;
    autoreleasepool(|_| {
        let url = NSURL::fileURLWithPath(&NSString::from_str(path));
        NSFileManager::defaultManager()
            .evictUbiquitousItemAtURL_error(&url)
            .map_err(|error| error.localizedDescription().to_string())
    })
}

#[cfg(any(not(target_os = "macos"), coverage))]
fn request_native_icloud_eviction(_path: &Path) -> Result<(), String> {
    Err("icloud-local-eviction-unsupported-platform".into())
}

fn observe_post_eviction(path: &Path) -> PostEvictionObservation {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return PostEvictionObservation {
            path_retained: false,
            is_ubiquitous: false,
            allocated_bytes: 0,
        };
    };
    let is_ubiquitous = observe_icloud_state(path, metadata.len())
        .map(|state| state.is_ubiquitous)
        .unwrap_or(false);
    PostEvictionObservation {
        path_retained: metadata.is_file() && !metadata.file_type().is_symlink(),
        is_ubiquitous,
        allocated_bytes: allocated_bytes(&metadata),
    }
}

fn result_id_for(result: &IcloudLocalEvictionResult) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-icloud-local-eviction-result-v1\0");
    for value in [
        result.plan_fingerprint.as_bytes(),
        result.approval_id.as_bytes(),
        result.path.as_bytes(),
    ] {
        hasher.update(value);
        hasher.update(&[0]);
    }
    for value in [
        result.requested_at_ms,
        result.allocated_bytes_before,
        result.allocated_bytes_after,
        result.observed_allocation_reduction_bytes,
    ] {
        hasher.update(&value.to_le_bytes());
    }
    for value in [
        result.eviction_request_succeeded,
        result.cloud_item_path_retained,
        result.is_ubiquitous_after,
        result.local_allocation_reduction_verified,
        result.verification_complete,
    ] {
        hash_bool(&mut hasher, value);
    }
    for blocker in &result.verification_blockers {
        hasher.update(blocker.as_bytes());
        hasher.update(&[0]);
    }
    hasher.finalize().to_hex().to_string()
}

fn build_result(
    plan: &IcloudLocalEvictionPlan,
    approval: &IcloudLocalEvictionApproval,
    requested_at_ms: u64,
    post: PostEvictionObservation,
) -> IcloudLocalEvictionResult {
    let reduction = plan.allocated_bytes.saturating_sub(post.allocated_bytes);
    let reduced = post.allocated_bytes < plan.allocated_bytes;
    let mut blockers = Vec::new();
    if !post.path_retained {
        blockers.push("icloud-cloud-item-path-not-retained".into());
    }
    if !post.is_ubiquitous {
        blockers.push("icloud-ubiquitous-identity-not-retained".into());
    }
    if !reduced {
        blockers.push("local-allocation-reduction-unverified".into());
    }
    let verification_complete = blockers.is_empty();
    let mut result = IcloudLocalEvictionResult {
        version: ICLOUD_LOCAL_EVICTION_VERSION,
        result_id: String::new(),
        plan_fingerprint: plan.plan_fingerprint.clone(),
        approval_id: approval.approval_id.clone(),
        path: plan.path.clone(),
        requested_at_ms,
        allocated_bytes_before: plan.allocated_bytes,
        allocated_bytes_after: post.allocated_bytes,
        observed_allocation_reduction_bytes: reduction,
        eviction_request_succeeded: true,
        cloud_item_path_retained: post.path_retained,
        is_ubiquitous_after: post.is_ubiquitous,
        local_allocation_reduction_verified: reduced,
        verification_complete,
        verification_blockers: blockers,
        notices: vec![
            "cloud-object-delete-not-requested".into(),
            "observed-allocation-reduction-is-not-volume-free-space-proof".into(),
        ],
    };
    result.result_id = result_id_for(&result);
    result
}

/// Remove only the local iCloud copy after revalidating the exact approved plan.
///
/// This never calls the regular file deletion APIs. A successful Foundation request is reported
/// separately from the observed local-allocation reduction.
#[cfg(not(coverage))]
pub fn execute_icloud_local_eviction(
    root: &CloudRoot,
    approved_plan: &IcloudLocalEvictionPlan,
    approval: &IcloudLocalEvictionApproval,
    confirmation_plan_fingerprint: &str,
    requested_at_ms: u64,
) -> Result<IcloudLocalEvictionResult, String> {
    validate_approval(approved_plan, approval, confirmation_plan_fingerprint)?;
    let path = Path::new(&approved_plan.path);
    let live_plan = plan_icloud_local_eviction(root, path, requested_at_ms)?;
    if live_plan.plan_fingerprint != approved_plan.plan_fingerprint
        || !live_plan.eligible_after_human_approval
    {
        return Err("icloud-local-eviction-live-plan-changed".into());
    }
    request_native_icloud_eviction(path)?;

    let started = Instant::now();
    let post = loop {
        let observed = observe_post_eviction(path);
        if !observed.path_retained
            || !observed.is_ubiquitous
            || observed.allocated_bytes < approved_plan.allocated_bytes
            || u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
                >= POST_EVICTION_WAIT_MS
        {
            break observed;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    Ok(build_result(approved_plan, approval, requested_at_ms, post))
}

/// Prepare the private approval/result directory without allowing an app-data symlink or a
/// not-yet-created app-data path to redirect the first directory creation into the cloud root.
pub fn prepare_immutable_record_directory(
    app_data_dir: &Path,
    cloud_root: &Path,
    directory_name: &str,
) -> Result<PathBuf, String> {
    let mut name_components = Path::new(directory_name).components();
    if !absolute_without_parent(app_data_dir)
        || !absolute_without_parent(cloud_root)
        || !matches!(
            name_components.next(),
            Some(std::path::Component::Normal(_))
        )
        || name_components.next().is_some()
    {
        return Err("icloud-local-eviction-record-path-invalid".into());
    }

    let canonical_cloud_root = std::fs::canonicalize(cloud_root)
        .map_err(|_| "icloud-local-eviction-cloud-root-unavailable".to_string())?;
    let existing_ancestor = app_data_dir
        .ancestors()
        .find(|candidate| candidate.exists())
        .ok_or_else(|| "icloud-local-eviction-record-parent-unavailable".to_string())?;
    let canonical_existing_ancestor = std::fs::canonicalize(existing_ancestor)
        .map_err(|_| "icloud-local-eviction-record-parent-unavailable".to_string())?;
    let missing_suffix = app_data_dir
        .strip_prefix(existing_ancestor)
        .map_err(|_| "icloud-local-eviction-record-parent-unavailable".to_string())?;
    let prospective_app_data_dir = canonical_existing_ancestor.join(missing_suffix);
    if prospective_app_data_dir.starts_with(&canonical_cloud_root) {
        return Err("icloud-local-eviction-record-dir-overlaps-cloud-data".into());
    }

    std::fs::create_dir_all(app_data_dir)
        .map_err(|_| "icloud-local-eviction-record-parent-create-failed".to_string())?;
    let canonical_app_data_dir = std::fs::canonicalize(app_data_dir)
        .map_err(|_| "icloud-local-eviction-record-parent-unavailable".to_string())?;
    if canonical_app_data_dir.starts_with(&canonical_cloud_root) {
        return Err("icloud-local-eviction-record-dir-overlaps-cloud-data".into());
    }

    let record_dir = app_data_dir.join(directory_name);
    match std::fs::symlink_metadata(&record_dir) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err("icloud-local-eviction-record-dir-not-real-directory".into());
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir(&record_dir)
                .map_err(|_| "icloud-local-eviction-record-dir-create-failed".to_string())?;
        }
        Err(_) => return Err("icloud-local-eviction-record-dir-unavailable".into()),
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&record_dir, std::fs::Permissions::from_mode(0o700))
            .map_err(|_| "icloud-local-eviction-record-dir-permissions-failed".to_string())?;
    }
    let canonical_record_dir = std::fs::canonicalize(&record_dir)
        .map_err(|_| "icloud-local-eviction-record-dir-unavailable".to_string())?;
    if canonical_record_dir.starts_with(&canonical_cloud_root) {
        return Err("icloud-local-eviction-record-dir-overlaps-cloud-data".into());
    }
    Ok(record_dir)
}

/// Persist an approval or result as a create-new, read-only JSON record.
pub fn write_immutable_record<T: Serialize>(
    record_dir: &Path,
    filename: &str,
    value: &T,
) -> Result<PathBuf, String> {
    if !absolute_without_parent(record_dir)
        || filename.is_empty()
        || filename.contains('/')
        || filename.contains('\\')
        || !filename.ends_with(".json")
    {
        return Err("icloud-local-eviction-record-path-invalid".into());
    }
    let directory =
        std::fs::symlink_metadata(record_dir).map_err(|_| "record-dir-unavailable".to_string())?;
    if directory.file_type().is_symlink() || !directory.is_dir() {
        return Err("record-dir-not-real-directory".into());
    }
    let path = record_dir.join(filename);
    let encoded = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|error| error.to_string())?;
    let result = (|| -> Result<(), String> {
        file.write_all(&encoded)
            .map_err(|error| error.to_string())?;
        file.write_all(b"\n").map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        let mut permissions = file
            .metadata()
            .map_err(|error| error.to_string())?
            .permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&path, permissions).map_err(|error| error.to_string())?;
        #[cfg(unix)]
        std::fs::File::open(record_dir)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| error.to_string())?;
        Ok(())
    })();
    if let Err(error) = result {
        drop(file);
        let _ = std::fs::remove_file(&path);
        return Err(error);
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(path: &Path) -> CloudRoot {
        CloudRoot {
            id: "icloud:test".into(),
            provider: CloudProvider::Icloud,
            account_scope: CloudAccountScope::Personal,
            label: "iCloud".into(),
            path: path.to_string_lossy().into_owned(),
            readable: true,
            access_issue: None,
        }
    }

    fn state() -> IcloudLocalState {
        IcloudLocalState {
            observation_method: IcloudStateObservationMethod::FileProviderCtlEvaluate,
            is_ubiquitous: true,
            is_uploaded: true,
            is_uploading: false,
            is_downloading: false,
            downloading_status_current: true,
            has_unresolved_conflicts: false,
            is_excluded_from_sync: false,
            is_sync_paused: Some(false),
            is_trashed: Some(false),
            allows_eviction: Some(true),
            provider_reported_bytes: Some(100),
            item_identifier_fingerprint: Some("a".repeat(64)),
        }
    }

    #[test]
    fn session_state_is_retained_before_provider_observation() {
        let temporary = tempfile::tempdir().unwrap();
        let cloud_root = root(temporary.path());
        for relative in [
            ".codex/sessions/example.jsonl",
            ".claude/projects/example.jsonl",
            ".claude.json",
        ] {
            let path = temporary.path().join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, b"synthetic session sentinel").unwrap();
            assert_eq!(
                observe_local_file(&cloud_root, &path).unwrap_err(),
                "icloud-local-eviction-agent-state-protected"
            );
            #[cfg(not(coverage))]
            assert_eq!(
                plan_icloud_local_eviction(&cloud_root, &path, 1).unwrap_err(),
                "icloud-local-eviction-agent-state-protected"
            );
            assert_eq!(std::fs::read(&path).unwrap(), b"synthetic session sentinel");
        }
        let ordinary = temporary.path().join("ordinary.jsonl");
        std::fs::write(&ordinary, b"ordinary data").unwrap();
        assert!(observe_local_file(&cloud_root, &ordinary).is_ok());
    }

    #[test]
    fn foundation_fallback_is_limited_to_unavailable_provider_commands() {
        for error in [
            "file-provider-status-command-unavailable",
            "file-provider-status-command-failed",
            "file-provider-status-field-missing:hasUnresolvedConflicts",
            "file-provider-status-field-missing:itemIdentifier",
        ] {
            assert!(file_provider_command_allows_foundation_fallback(error));
        }
        for error in [
            "file-provider-status-command-timeout",
            "file-provider-status-output-too-large",
            "file-provider-status-field-missing:isUploaded",
            "file-provider-status-document-size-mismatch",
            "icloud-item-not-ubiquitous",
        ] {
            assert!(!file_provider_command_allows_foundation_fallback(error));
        }
    }

    #[test]
    fn foundation_observation_never_authorizes_local_eviction() {
        let temp = tempfile::tempdir().unwrap();
        let mut state = state();
        state.observation_method = IcloudStateObservationMethod::FoundationUbiquitousResourceValues;
        state.is_sync_paused = None;
        state.is_trashed = None;
        state.allows_eviction = None;
        state.provider_reported_bytes = None;
        state.item_identifier_fingerprint = None;
        let plan = build_plan(
            &root(temp.path()),
            &temp.path().join("file.bin"),
            file(),
            state,
            idle(),
            20,
        );
        assert!(!plan.eligible_after_human_approval);
        assert!(plan
            .blockers
            .contains(&"icloud-file-provider-native-status-unavailable".into()));
    }

    fn file_provider_state() -> IcloudLocalState {
        file_provider_icloud_state(
            true,
            &crate::provider_sync::FileProviderItemStatus {
                is_downloaded: true,
                is_downloading: false,
                is_most_recent_version_downloaded: true,
                is_uploaded: true,
                is_uploading: false,
                has_unresolved_conflicts: false,
                is_excluded_from_sync: false,
                is_sync_paused: false,
                is_trashed: false,
                capabilities: 805_306_495,
                allows_eviction: true,
                observed_bytes: 100,
                item_identifier_fingerprint: "a".repeat(64),
            },
        )
    }

    fn idle() -> ActiveUseEvidence {
        ActiveUseEvidence {
            method: "lsof-fp+ps-command".into(),
            evidence_complete: true,
            active: false,
            observed_pids: Vec::new(),
            results_truncated: false,
            error: None,
        }
    }

    fn file() -> LocalFileObservation {
        LocalFileObservation {
            logical_bytes: 100,
            allocated_bytes: 4096,
            modified_ms: 10,
        }
    }

    fn plan(temp: &Path) -> IcloudLocalEvictionPlan {
        build_plan(
            &root(temp),
            &temp.join("file.bin"),
            file(),
            state(),
            idle(),
            20,
        )
    }

    #[test]
    fn synced_idle_item_is_eligible_only_after_human_approval() {
        let temp = tempfile::tempdir().unwrap();
        let plan = plan(temp.path());
        assert!(plan.eligible_after_human_approval);
        assert_eq!(
            plan.blockers,
            ["human-local-eviction-approval-required".to_string()]
        );
        assert!(valid_hex64(&plan.plan_fingerprint));
        assert!(plan
            .notices
            .contains(&"cloud-object-must-remain-present".into()));
    }

    #[test]
    fn sync_conflict_and_active_use_fail_closed() {
        let temp = tempfile::tempdir().unwrap();
        let mut state = state();
        state.is_uploaded = false;
        state.is_uploading = true;
        state.has_unresolved_conflicts = true;
        let mut active = idle();
        active.active = true;
        active.observed_pids = vec![42];
        let plan = build_plan(
            &root(temp.path()),
            &temp.path().join("file.bin"),
            file(),
            state,
            active,
            20,
        );
        assert!(!plan.eligible_after_human_approval);
        for blocker in [
            "icloud-upload-not-confirmed",
            "icloud-upload-still-running",
            "icloud-unresolved-conflict",
            "active-file-use-detected",
        ] {
            assert!(plan.blockers.contains(&blocker.to_string()));
        }
    }

    #[test]
    fn file_provider_policy_and_identity_must_be_complete() {
        let temp = tempfile::tempdir().unwrap();
        let eligible = build_plan(
            &root(temp.path()),
            &temp.path().join("file.bin"),
            file(),
            file_provider_state(),
            idle(),
            20,
        );
        assert!(eligible.eligible_after_human_approval);

        for (state, blocker) in [
            (
                {
                    let mut state = file_provider_state();
                    state.is_sync_paused = Some(true);
                    state
                },
                "icloud-file-provider-sync-paused-or-unconfirmed",
            ),
            (
                {
                    let mut state = file_provider_state();
                    state.is_trashed = Some(true);
                    state
                },
                "icloud-file-provider-item-trashed-or-unconfirmed",
            ),
            (
                {
                    let mut state = file_provider_state();
                    state.allows_eviction = Some(false);
                    state
                },
                "icloud-file-provider-eviction-capability-unconfirmed",
            ),
            (
                {
                    let mut state = file_provider_state();
                    state.provider_reported_bytes = Some(99);
                    state
                },
                "icloud-file-provider-document-size-mismatch",
            ),
            (
                {
                    let mut state = file_provider_state();
                    state.item_identifier_fingerprint = None;
                    state
                },
                "icloud-file-provider-item-identity-unconfirmed",
            ),
        ] {
            let plan = build_plan(
                &root(temp.path()),
                &temp.path().join("file.bin"),
                file(),
                state,
                idle(),
                20,
            );
            assert!(!plan.eligible_after_human_approval, "{blocker}");
            assert!(plan.blockers.contains(&blocker.to_string()), "{blocker}");
        }
    }

    #[test]
    fn incomplete_active_use_evidence_blocks() {
        let temp = tempfile::tempdir().unwrap();
        let mut active = idle();
        active.evidence_complete = false;
        active.error = Some("lsof-timeout".into());
        let plan = build_plan(
            &root(temp.path()),
            &temp.path().join("file.bin"),
            file(),
            state(),
            active,
            20,
        );
        assert!(!plan.eligible_after_human_approval);
        assert!(plan
            .blockers
            .contains(&"active-use-evidence-incomplete".into()));
    }

    #[cfg(all(unix, not(coverage)))]
    #[test]
    fn lsof_unrelated_warnings_are_benign_but_target_warnings_block() {
        let target = Path::new("/Users/test/Library/Caches/example");
        assert!(lsof_stderr_is_benign(
            b"lsof: WARNING: can't stat() /Volumes/Other\n",
            target,
        ));
        assert!(!lsof_stderr_is_benign(
            b"lsof: WARNING: can't stat() /Users/test/Library/Caches/example/nested\n",
            target,
        ));
        assert!(!lsof_stderr_is_benign(b"lsof: error: permission denied\n", target));
    }

    #[cfg(all(unix, not(coverage)))]
    #[test]
    fn process_command_reference_parser_detects_relative_path_and_excludes_self() {
        let path = Path::new("/Cloud/SONY ICD-TX650/FOLDER01/231031_2308.wav");
        let output = b"  101 1 python audio_library.py /Cloud/SONY ICD-TX650/FOLDER01/other.wav\n\
  202 1 python audio_library.py --path FOLDER01/231031_2308.wav --keep-local\n\
  303 1 checker --path /Cloud/SONY ICD-TX650/FOLDER01/231031_2308.wav\n";
        assert_eq!(
            parse_process_command_references(output, path, 303),
            vec![202]
        );
    }

    #[cfg(all(unix, not(coverage)))]
    #[test]
    fn process_command_reference_parser_excludes_path_bearing_supervisor_lineage() {
        let path = Path::new("/Cloud/large-upload.zip");
        let output =
            b"  500 1 gtimeout 8 disksage-icloud-local-eviction --path /Cloud/large-upload.zip\n\
  501 500 disksage-icloud-local-eviction --path /Cloud/large-upload.zip\n\
  502 1 preview-worker --input /Cloud/large-upload.zip\n";
        assert_eq!(
            parse_process_command_references(output, path, 501),
            vec![502]
        );
    }

    #[cfg(all(unix, not(coverage)))]
    #[test]
    fn short_basename_does_not_create_broad_process_match() {
        assert!(!process_command_matches_target(
            "worker --path x.wav",
            Path::new("/Cloud/x.wav"),
        ));
    }

    #[test]
    fn fingerprint_changes_with_allocation_and_sync_state() {
        let temp = tempfile::tempdir().unwrap();
        let first = plan(temp.path());
        let mut changed_file = file();
        changed_file.allocated_bytes += 512;
        let second = build_plan(
            &root(temp.path()),
            &temp.path().join("file.bin"),
            changed_file,
            state(),
            idle(),
            21,
        );
        let mut changed_state = state();
        changed_state.is_uploading = true;
        let third = build_plan(
            &root(temp.path()),
            &temp.path().join("file.bin"),
            file(),
            changed_state,
            idle(),
            22,
        );
        assert_ne!(first.plan_fingerprint, second.plan_fingerprint);
        assert_ne!(first.plan_fingerprint, third.plan_fingerprint);
    }

    #[test]
    fn approval_is_human_fingerprint_and_time_bound() {
        let temp = tempfile::tempdir().unwrap();
        let plan = plan(temp.path());
        assert!(approve_icloud_local_eviction(
            &plan,
            &plan.plan_fingerprint,
            21,
            "agent:test",
            "reviewed"
        )
        .is_err());
        assert!(approve_icloud_local_eviction(
            &plan,
            &"0".repeat(64),
            21,
            "human:test",
            "reviewed"
        )
        .is_err());
        assert!(approve_icloud_local_eviction(
            &plan,
            &plan.plan_fingerprint,
            19,
            "human:test",
            "reviewed"
        )
        .is_err());
        let approval = approve_icloud_local_eviction(
            &plan,
            &plan.plan_fingerprint,
            21,
            "human:test",
            "retain cloud object, release local allocation",
        )
        .unwrap();
        validate_approval(&plan, &approval, &plan.plan_fingerprint).unwrap();
        assert!(valid_hex64(&approval.approval_id));
    }

    #[test]
    fn post_result_never_equates_path_blocks_with_volume_free_space() {
        let temp = tempfile::tempdir().unwrap();
        let plan = plan(temp.path());
        let approval = approve_icloud_local_eviction(
            &plan,
            &plan.plan_fingerprint,
            21,
            "human:test",
            "reviewed",
        )
        .unwrap();
        let result = build_result(
            &plan,
            &approval,
            22,
            PostEvictionObservation {
                path_retained: true,
                is_ubiquitous: true,
                allocated_bytes: 512,
            },
        );
        assert!(result.verification_complete);
        assert_eq!(result.observed_allocation_reduction_bytes, 3584);
        assert!(result
            .notices
            .contains(&"observed-allocation-reduction-is-not-volume-free-space-proof".into()));
        assert!(valid_hex64(&result.result_id));
    }

    #[test]
    fn missing_cloud_path_or_unchanged_allocation_remains_unverified() {
        let temp = tempfile::tempdir().unwrap();
        let plan = plan(temp.path());
        let approval = approve_icloud_local_eviction(
            &plan,
            &plan.plan_fingerprint,
            21,
            "human:test",
            "reviewed",
        )
        .unwrap();
        let result = build_result(
            &plan,
            &approval,
            22,
            PostEvictionObservation {
                path_retained: false,
                is_ubiquitous: false,
                allocated_bytes: 4096,
            },
        );
        assert!(!result.verification_complete);
        assert_eq!(result.observed_allocation_reduction_bytes, 0);
        assert!(result
            .verification_blockers
            .contains(&"icloud-cloud-item-path-not-retained".into()));
        assert!(result
            .verification_blockers
            .contains(&"local-allocation-reduction-unverified".into()));
    }

    #[test]
    fn path_observation_rejects_escape_symlink_and_directory() {
        let temp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let root = root(temp.path());
        std::fs::write(temp.path().join("file.bin"), b"bytes").unwrap();
        assert!(observe_local_file(&root, &temp.path().join("file.bin")).is_ok());
        assert!(observe_local_file(&root, &outside.path().join("outside.bin")).is_err());
        assert!(observe_local_file(&root, temp.path()).is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(
                outside.path().join("outside.bin"),
                temp.path().join("link.bin"),
            )
            .unwrap();
            assert!(observe_local_file(&root, &temp.path().join("link.bin")).is_err());
        }
    }

    #[test]
    fn immutable_records_are_create_new_and_read_only() {
        let temp = tempfile::tempdir().unwrap();
        let plan = plan(temp.path());
        let path = write_immutable_record(temp.path(), "plan.json", &plan).unwrap();
        assert!(path.exists());
        assert!(std::fs::metadata(&path).unwrap().permissions().readonly());
        assert!(write_immutable_record(temp.path(), "plan.json", &plan).is_err());
        assert!(write_immutable_record(temp.path(), "../escape.json", &plan).is_err());
    }

    #[test]
    fn record_directory_supports_safe_first_use_outside_cloud() {
        let temp = tempfile::tempdir().unwrap();
        let cloud = temp.path().join("cloud");
        let app_data = temp.path().join("state").join("new-app-data");
        std::fs::create_dir(&cloud).unwrap();

        let record_dir =
            prepare_immutable_record_directory(&app_data, &cloud, "icloud-local-evictions")
                .unwrap();

        assert_eq!(record_dir, app_data.join("icloud-local-evictions"));
        assert!(record_dir.is_dir());
        assert!(!record_dir.starts_with(&cloud));
    }

    #[test]
    fn record_directory_rejects_prospective_cloud_parent_before_creation() {
        let temp = tempfile::tempdir().unwrap();
        let cloud = temp.path().join("cloud");
        let app_data = cloud.join("new-app-data");
        std::fs::create_dir(&cloud).unwrap();

        assert_eq!(
            prepare_immutable_record_directory(&app_data, &cloud, "icloud-local-evictions")
                .unwrap_err(),
            "icloud-local-eviction-record-dir-overlaps-cloud-data"
        );
        assert!(!app_data.exists());
    }

    #[cfg(unix)]
    #[test]
    fn record_directory_rejects_symlinked_parent_to_cloud_before_creation() {
        let temp = tempfile::tempdir().unwrap();
        let cloud = temp.path().join("cloud");
        let linked_cloud = temp.path().join("linked-cloud");
        std::fs::create_dir(&cloud).unwrap();
        std::os::unix::fs::symlink(&cloud, &linked_cloud).unwrap();
        let app_data = linked_cloud.join("new-app-data");

        assert_eq!(
            prepare_immutable_record_directory(&app_data, &cloud, "icloud-local-evictions")
                .unwrap_err(),
            "icloud-local-eviction-record-dir-overlaps-cloud-data"
        );
        assert!(!cloud.join("new-app-data").exists());
    }

    #[cfg(unix)]
    #[test]
    fn record_directory_rejects_symlink_without_following_it() {
        let temp = tempfile::tempdir().unwrap();
        let cloud = temp.path().join("cloud");
        let app_data = temp.path().join("app-data");
        let redirect = temp.path().join("redirect");
        std::fs::create_dir(&cloud).unwrap();
        std::fs::create_dir(&app_data).unwrap();
        std::fs::create_dir(&redirect).unwrap();
        std::os::unix::fs::symlink(&redirect, app_data.join("icloud-local-evictions")).unwrap();

        assert_eq!(
            prepare_immutable_record_directory(&app_data, &cloud, "icloud-local-evictions")
                .unwrap_err(),
            "icloud-local-eviction-record-dir-not-real-directory"
        );
        assert!(!std::fs::metadata(&redirect)
            .unwrap()
            .permissions()
            .readonly());
    }
}
