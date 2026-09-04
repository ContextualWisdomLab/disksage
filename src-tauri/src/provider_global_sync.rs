//! Read-only, bounded global File Provider queue evidence for third-party cloud roots.
//!
//! Per-item `fileproviderctl evaluate` output can report an uploaded item while the provider is
//! still processing a global upload/download queue. New copies are therefore admitted only after a
//! fresh, provider-wide dump is quiet. The dump is parsed in memory and user paths are never kept
//! in the report.

use crate::cloud::CloudProvider;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

// macOS provider dumps include bounded item summaries even with --limit-dump-size. Keep enough
// room for real OneDrive/Google Drive dumps while retaining a hard memory ceiling.
const MAX_DUMP_BYTES: u64 = 32 * 1024 * 1024;
const PROBE_TIMEOUT_MS: u64 = 20_000;
const PROBE_TIMEOUT_MARKER: &str = "provider-global-sync-probe-timeout: yes";
const PROBE_TIMEOUT_NOTICE: &str = "provider-global-sync-probe-timeout";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderGlobalSyncState {
    Clear,
    Pending,
    Error,
    Unavailable,
}

impl ProviderGlobalSyncState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Clear => "clear",
            Self::Pending => "pending",
            Self::Error => "error",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderGlobalSyncReport {
    pub schema_version: u32,
    pub provider: CloudProvider,
    pub evidence_kind: String,
    #[serde(default)]
    pub observed_at_ms: u64,
    /// Earliest retained observation in the current provider admission-blocker run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub admission_blocked_since_ms: Option<u64>,
    pub evidence_complete: bool,
    pub state: ProviderGlobalSyncState,
    pub upload_progress_present: bool,
    pub download_progress_present: bool,
    pub pending_indexable_count: Option<u64>,
    pub blockers: Vec<String>,
    pub notices: Vec<String>,
}

pub const PROVIDER_GLOBAL_SYNC_SCHEMA_VERSION: u32 = 1;

fn provider_identifier(provider: CloudProvider) -> Option<&'static str> {
    match provider {
        CloudProvider::Onedrive => Some("com.microsoft.OneDrive.FileProvider"),
        CloudProvider::GoogleDrive => Some("com.google.drivefs.fpext"),
        CloudProvider::Icloud => None,
    }
}

fn line_has_active_progress(line: &str, label: &str) -> bool {
    let Some((_, value)) = line.split_once(label) else {
        return false;
    };
    let value = value.trim();
    !value.is_empty()
        && !value.eq_ignore_ascii_case("(null)")
        && !value.eq_ignore_ascii_case("none")
}

fn parse_pending_indexable_count(line: &str) -> Option<u64> {
    let line = line.trim_start();
    let line = line.strip_prefix("+ ").unwrap_or(line);
    line.strip_prefix("pending-indexable-count:")?
        .trim()
        .parse()
        .ok()
}

fn has_reconciliation_backlog(line: &str) -> bool {
    let line = line.trim_start();
    let line = line.strip_prefix("+ ").unwrap_or(line);
    let Some(rest) = line.strip_prefix("reconciliation (") else {
        return false;
    };
    let Some((count, _)) = rest.split_once(" entries") else {
        return false;
    };
    count
        .trim()
        .parse::<u64>()
        .ok()
        .is_some_and(|count| count > 0)
}

fn probe_output_is_truncated(bytes_len: usize) -> bool {
    bytes_len as u64 > MAX_DUMP_BYTES
}

fn contains_bounded_numeric_marker(text: &str, prefix: &str, number: &str) -> bool {
    text.match_indices(prefix).any(|(index, _)| {
        let remainder = &text[index + prefix.len()..];
        remainder.starts_with(number)
            && remainder
                .as_bytes()
                .get(number.len())
                .map_or(true, |next| !next.is_ascii_digit())
    })
}

/// Parse only aggregate queue markers from one provider-filtered File Provider dump.
pub fn parse_dump(
    provider: CloudProvider,
    output: &str,
) -> Result<ProviderGlobalSyncReport, String> {
    let identifier = provider_identifier(provider)
        .ok_or_else(|| "provider-global-sync-icloud-specialized".to_string())?;
    if !output.contains(identifier) || !output.contains("sync engine state:") {
        return Err("provider-global-sync-dump-incomplete".into());
    }
    let probe_timed_out = output.lines().any(|line| {
        line.trim()
            .strip_prefix("+ ")
            .unwrap_or_else(|| line.trim())
            == PROBE_TIMEOUT_MARKER
    });

    let mut upload_progress_present = false;
    let mut download_progress_present = false;
    let mut pending_indexable_count = None;
    let mut needs_indexing = false;
    let mut reconciliation_pending = false;
    let mut has_error = false;
    let mut has_filename_too_long = false;
    let mut has_temporarily_disconnected = false;
    let mut has_server_unreachable = false;
    let mut has_local_disk_full = false;
    let mut has_item_not_found = false;
    let mut hidden_default_domain = false;

    for line in output.lines() {
        let trimmed = line.trim();
        let marker = trimmed.strip_prefix("+ ").unwrap_or(trimmed).trim();
        let marker_lower = marker.to_ascii_lowercase();
        if marker.starts_with("domain: ") {
            hidden_default_domain = marker.contains("(default)") && marker.contains("(hidden)");
        }
        upload_progress_present |= line_has_active_progress(marker, "upload progress:");
        download_progress_present |= line_has_active_progress(marker, "download progress:");
        if let Some(count) = parse_pending_indexable_count(marker) {
            pending_indexable_count =
                Some(pending_indexable_count.map_or(count, |existing: u64| existing.max(count)));
        }
        if marker == "needs-indexing: yes" || marker == "indexing: yes" {
            needs_indexing = true;
        }
        reconciliation_pending |= has_reconciliation_backlog(marker);
        has_filename_too_long |= marker.contains("POSIX 63")
            || marker.contains("파일 이름이 너무 깁니다")
            || marker_lower.contains("filename too long");
        has_temporarily_disconnected |= marker_lower.contains("temporarily disconnected");
        has_server_unreachable |= marker_lower.contains("serverunreachable")
            || marker_lower.contains("server unreachable")
            || marker_lower.contains("code=-1004");
        has_item_not_found |= marker_lower.contains("code=-1005")
            || marker_lower.contains("itemnotfound")
            || marker.contains("파일이 존재하지 않습니다");
        has_local_disk_full |=
            contains_bounded_numeric_marker(&marker_lower, "odresult_errno ", "28")
                || contains_bounded_numeric_marker(&marker_lower, "errno ", "28")
                || marker_lower.contains("enospc")
                || contains_bounded_numeric_marker(&marker_lower, "code=", "28")
                || contains_bounded_numeric_marker(&marker_lower, "code ", "28")
                || contains_bounded_numeric_marker(&marker_lower, "osstatus -", "34")
                || marker_lower.contains("no space left on device")
                || marker_lower.contains("disk full");
        if has_filename_too_long
            || has_temporarily_disconnected
            || has_server_unreachable
            || has_local_disk_full
            || has_item_not_found
            || (marker.contains("user-disabled") && !hidden_default_domain)
            || marker.contains("can't dump the extension")
            || marker.contains("Error Domain=")
            || (marker.contains("error:'") && !marker.contains("error:'<nil>'"))
        {
            has_error = true;
        }
        if let Some(value) = marker.strip_prefix("errors:") {
            has_error |= value
                .trim()
                .parse::<u64>()
                .ok()
                .is_some_and(|count| count > 0);
        }
    }

    let pending = upload_progress_present
        || download_progress_present
        || needs_indexing
        || pending_indexable_count.is_some_and(|count| count > 0)
        || reconciliation_pending;
    let state = if probe_timed_out {
        ProviderGlobalSyncState::Unavailable
    } else if has_error {
        ProviderGlobalSyncState::Error
    } else if pending {
        ProviderGlobalSyncState::Pending
    } else {
        ProviderGlobalSyncState::Clear
    };
    let mut blockers = Vec::new();
    if upload_progress_present || download_progress_present {
        blockers.push("provider-global-sync-transfer-active".into());
    }
    if needs_indexing || pending_indexable_count.is_some_and(|count| count > 0) {
        blockers.push("provider-global-sync-indexing-pending".into());
    }
    if reconciliation_pending {
        blockers.push("provider-global-sync-reconciliation-pending".into());
    }
    if has_filename_too_long {
        blockers.push("provider-global-sync-filename-too-long".into());
    }
    if has_temporarily_disconnected {
        blockers.push("provider-global-sync-temporarily-disconnected".into());
    }
    if has_server_unreachable {
        blockers.push("provider-global-sync-server-unreachable".into());
    }
    if has_local_disk_full {
        blockers.push("provider-global-sync-local-disk-full".into());
    }
    if has_item_not_found {
        blockers.push("provider-global-sync-item-not-found".into());
    }
    if has_error {
        blockers.push("provider-global-sync-error".into());
    }
    if probe_timed_out {
        blockers.push(PROBE_TIMEOUT_NOTICE.into());
    }
    let mut notices = vec![
        "provider-global-sync-dump-read-only".into(),
        "provider-global-sync-user-paths-not-retained".into(),
    ];
    if probe_timed_out {
        notices.push(PROBE_TIMEOUT_NOTICE.into());
    }
    Ok(ProviderGlobalSyncReport {
        schema_version: PROVIDER_GLOBAL_SYNC_SCHEMA_VERSION,
        provider,
        evidence_kind: "fileproviderctl-global-dump".into(),
        observed_at_ms: 0,
        admission_blocked_since_ms: None,
        evidence_complete: !probe_timed_out,
        state,
        upload_progress_present,
        download_progress_present,
        pending_indexable_count,
        blockers,
        notices,
    })
}

#[cfg(target_os = "macos")]
fn partial_dump_after_timeout(bytes: Vec<u8>, identifier: &str) -> Option<String> {
    if probe_output_is_truncated(bytes.len()) {
        return None;
    }
    let mut output = String::from_utf8(bytes).ok()?;
    if !output.contains(identifier) || !output.contains("sync engine state:") {
        return None;
    }
    output.push_str("\n+ ");
    output.push_str(PROBE_TIMEOUT_MARKER);
    output.push('\n');
    Some(output)
}

#[cfg(target_os = "macos")]
fn run_dump(provider: CloudProvider) -> Result<String, String> {
    use std::io::Read;
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::{Duration, Instant};

    let identifier = provider_identifier(provider)
        .ok_or_else(|| "provider-global-sync-icloud-specialized".to_string())?;
    let mut command = Command::new("/usr/bin/fileproviderctl");
    command
        .args(["dump", identifier, "-l"])
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    // File Provider helpers can outlive the command leader and retain stdout. Keep the
    // entire probe in a private group so timeout cleanup can close the pipe and join it.
    unsafe {
        command.pre_exec(|| {
            if libc::setpgid(0, 0) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = command
        .spawn()
        .map_err(|_| "provider-global-sync-probe-unavailable".to_string())?;
    let child_pid = child.id();
    let kill_group = || unsafe {
        let _ = libc::kill(-(child_pid as libc::pid_t), libc::SIGKILL);
    };
    let Some(stdout) = child.stdout.take() else {
        kill_group();
        let _ = child.kill();
        let _ = child.wait();
        return Err("provider-global-sync-probe-stdout-unavailable".into());
    };
    let reader = thread::spawn(move || -> Result<Vec<u8>, String> {
        let max_bytes = MAX_DUMP_BYTES as usize + 1;
        let mut stdout = stdout;
        let mut bytes = Vec::with_capacity(64 * 1024);
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let read = stdout
                .read(&mut buffer)
                .map_err(|_| "provider-global-sync-probe-read-failed".to_string())?;
            if read == 0 {
                break;
            }
            let remaining = max_bytes.saturating_sub(bytes.len());
            if remaining > 0 {
                bytes.extend_from_slice(&buffer[..read.min(remaining)]);
            }
        }
        Ok(bytes)
    });
    let deadline = Instant::now() + Duration::from_millis(PROBE_TIMEOUT_MS);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                kill_group();
                break status;
            }
            Ok(None) if Instant::now() >= deadline => {
                kill_group();
                let _ = child.kill();
                let _ = child.wait();
                let partial = reader.join().ok().and_then(Result::ok);
                if let Some(output) =
                    partial.and_then(|bytes| partial_dump_after_timeout(bytes, identifier))
                {
                    return Ok(output);
                }
                return Err("provider-global-sync-probe-timeout".into());
            }
            Ok(None) => thread::sleep(Duration::from_millis(50)),
            Err(_) => {
                kill_group();
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                return Err("provider-global-sync-probe-failed".into());
            }
        }
    };
    let bytes = reader
        .join()
        .map_err(|_| "provider-global-sync-probe-read-failed".to_string())??;
    if !status.success() {
        return Err("provider-global-sync-probe-exit-failed".into());
    }
    if probe_output_is_truncated(bytes.len()) {
        return Err("provider-global-sync-probe-output-truncated".into());
    }
    String::from_utf8(bytes).map_err(|_| "provider-global-sync-probe-output-invalid".into())
}

#[cfg(target_os = "macos")]
pub fn inspect_new_copy_admission(
    provider: CloudProvider,
) -> Result<ProviderGlobalSyncReport, String> {
    let output = run_dump(provider)?;
    let mut report = parse_dump(provider, &output)?;
    report.observed_at_ms = system_time_ms();
    Ok(report)
}

#[cfg(not(target_os = "macos"))]
pub fn inspect_new_copy_admission(
    provider: CloudProvider,
) -> Result<ProviderGlobalSyncReport, String> {
    Err(format!(
        "provider-global-sync-unsupported-platform-{}",
        provider.as_str()
    ))
}

fn report_identity_is_valid(report: &ProviderGlobalSyncReport) -> bool {
    report.schema_version == PROVIDER_GLOBAL_SYNC_SCHEMA_VERSION
        && report.evidence_kind == "fileproviderctl-global-dump"
        && provider_identifier(report.provider).is_some()
}

pub const PROVIDER_GLOBAL_SYNC_EVIDENCE_SCHEMA_VERSION: u32 = 1;
pub const PROVIDER_GLOBAL_SYNC_EVIDENCE_DIRECTORY: &str = "provider-global-sync-evidence";
const MAX_PERSISTED_PROVIDER_GLOBAL_SYNC_SNAPSHOTS: usize = 128;
const MAX_PERSISTED_PROVIDER_GLOBAL_SYNC_SNAPSHOT_BYTES: usize = 64 * 1024;

/// Path-free provider-global evidence retained only to measure a blocker across restarts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderGlobalSyncEvidenceSnapshot {
    pub schema_version: u32,
    pub observed_at_ms: u64,
    pub provider: CloudProvider,
    pub evidence_complete: bool,
    pub state: ProviderGlobalSyncState,
    pub upload_progress_present: bool,
    pub download_progress_present: bool,
    pub pending_indexable_count: Option<u64>,
    pub blockers: Vec<String>,
    pub evidence_fingerprint_sha256: String,
}

fn system_time_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .unwrap_or(0)
}

fn provider_global_sync_blocker_key(report: &ProviderGlobalSyncReport) -> String {
    let mut blockers = report.blockers.clone();
    blockers.sort_unstable();
    blockers.dedup();
    format!(
        "{}|{}|{}|{}|{}|{}",
        report.provider.as_str(),
        report.state.as_str(),
        report.upload_progress_present,
        report.download_progress_present,
        report
            .pending_indexable_count
            .is_some_and(|count| count > 0),
        blockers.join(",")
    )
}

fn provider_global_sync_snapshot_key(snapshot: &ProviderGlobalSyncEvidenceSnapshot) -> String {
    let mut blockers = snapshot.blockers.clone();
    blockers.sort_unstable();
    blockers.dedup();
    format!(
        "{}|{}|{}|{}|{}|{}",
        snapshot.provider.as_str(),
        snapshot.state.as_str(),
        snapshot.upload_progress_present,
        snapshot.download_progress_present,
        snapshot
            .pending_indexable_count
            .is_some_and(|count| count > 0),
        blockers.join(",")
    )
}

fn provider_global_sync_fingerprint(
    snapshot: &ProviderGlobalSyncEvidenceSnapshot,
) -> Result<String, String> {
    let mut unsigned = snapshot.clone();
    unsigned.evidence_fingerprint_sha256.clear();
    let encoded = serde_json::to_vec(&unsigned)
        .map_err(|_| "provider-global-sync-evidence-fingerprint-encode-failed".to_string())?;
    let digest = Sha256::digest(encoded);
    Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}

pub fn provider_global_sync_evidence_snapshot_from_report(
    report: &ProviderGlobalSyncReport,
) -> Result<ProviderGlobalSyncEvidenceSnapshot, String> {
    if !report_identity_is_valid(report)
        || report.observed_at_ms == 0
        || report
            .blockers
            .iter()
            .any(|blocker| !is_stable_provider_blocker(blocker))
    {
        return Err("provider-global-sync-evidence-claim-invalid".into());
    }
    let mut snapshot = ProviderGlobalSyncEvidenceSnapshot {
        schema_version: PROVIDER_GLOBAL_SYNC_EVIDENCE_SCHEMA_VERSION,
        observed_at_ms: report.observed_at_ms,
        provider: report.provider,
        evidence_complete: report.evidence_complete,
        state: report.state,
        upload_progress_present: report.upload_progress_present,
        download_progress_present: report.download_progress_present,
        pending_indexable_count: report.pending_indexable_count,
        blockers: report.blockers.clone(),
        evidence_fingerprint_sha256: String::new(),
    };
    snapshot.evidence_fingerprint_sha256 = provider_global_sync_fingerprint(&snapshot)?;
    Ok(snapshot)
}

pub fn validate_provider_global_sync_evidence_snapshot(
    snapshot: &ProviderGlobalSyncEvidenceSnapshot,
) -> Result<(), String> {
    if snapshot.schema_version != PROVIDER_GLOBAL_SYNC_EVIDENCE_SCHEMA_VERSION
        || snapshot.observed_at_ms == 0
        || provider_identifier(snapshot.provider).is_none()
        || snapshot
            .blockers
            .iter()
            .any(|blocker| !is_stable_provider_blocker(blocker))
    {
        return Err("provider-global-sync-evidence-shape-invalid".into());
    }
    let expected = provider_global_sync_fingerprint(snapshot)?;
    if snapshot.evidence_fingerprint_sha256 != expected {
        return Err("provider-global-sync-evidence-fingerprint-invalid".into());
    }
    Ok(())
}

#[cfg(not(coverage))]
fn provider_global_sync_evidence_directory(app_data_dir: &Path) -> Result<PathBuf, String> {
    if !app_data_dir.is_absolute()
        || app_data_dir
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("provider-global-sync-evidence-parent-invalid".into());
    }
    std::fs::create_dir_all(app_data_dir)
        .map_err(|_| "provider-global-sync-evidence-parent-create-failed".to_string())?;
    let parent = std::fs::symlink_metadata(app_data_dir)
        .map_err(|_| "provider-global-sync-evidence-parent-unavailable".to_string())?;
    if parent.file_type().is_symlink() || !parent.is_dir() {
        return Err("provider-global-sync-evidence-parent-unsafe".into());
    }
    let directory = app_data_dir.join(PROVIDER_GLOBAL_SYNC_EVIDENCE_DIRECTORY);
    std::fs::create_dir_all(&directory)
        .map_err(|_| "provider-global-sync-evidence-directory-create-failed".to_string())?;
    let metadata = std::fs::symlink_metadata(&directory)
        .map_err(|_| "provider-global-sync-evidence-directory-unavailable".to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("provider-global-sync-evidence-directory-unsafe".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).map_err(
            |_| "provider-global-sync-evidence-directory-permissions-failed".to_string(),
        )?;
    }
    Ok(directory)
}

#[cfg(not(coverage))]
fn prune_provider_global_sync_evidence(directory: &Path) -> Result<(), String> {
    let mut records = std::fs::read_dir(directory)
        .map_err(|_| "provider-global-sync-evidence-directory-read-failed".to_string())?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let (timestamp, fingerprint) = name.strip_suffix(".json")?.split_once('-')?;
            (timestamp.len() == 20
                && timestamp.bytes().all(|byte| byte.is_ascii_digit())
                && fingerprint.len() == 64
                && fingerprint
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)))
            .then_some((name, entry.path()))
        })
        .collect::<Vec<_>>();
    records.sort_by(|left, right| left.0.cmp(&right.0));
    while records.len() > MAX_PERSISTED_PROVIDER_GLOBAL_SYNC_SNAPSHOTS {
        let (_, path) = records.remove(0);
        std::fs::remove_file(path)
            .map_err(|_| "provider-global-sync-evidence-retention-failed".to_string())?;
    }
    Ok(())
}

/// Persist a bounded, path-free provider observation; it never mutates the cloud root.
#[cfg(not(coverage))]
pub fn write_provider_global_sync_evidence(
    app_data_dir: &Path,
    report: &ProviderGlobalSyncReport,
) -> Result<PathBuf, String> {
    use std::io::Write;
    let snapshot = provider_global_sync_evidence_snapshot_from_report(report)?;
    validate_provider_global_sync_evidence_snapshot(&snapshot)?;
    let directory = provider_global_sync_evidence_directory(app_data_dir)?;
    let path = directory.join(format!(
        "{:020}-{}.json",
        snapshot.observed_at_ms, snapshot.evidence_fingerprint_sha256
    ));
    let encoded = serde_json::to_vec_pretty(&snapshot)
        .map_err(|_| "provider-global-sync-evidence-encode-failed".to_string())?;
    if encoded.len() > MAX_PERSISTED_PROVIDER_GLOBAL_SYNC_SNAPSHOT_BYTES {
        return Err("provider-global-sync-evidence-too-large".into());
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o400);
    }
    let mut file = options
        .open(&path)
        .map_err(|_| "provider-global-sync-evidence-create-failed".to_string())?;
    let result = file
        .write_all(&encoded)
        .and_then(|_| file.sync_all())
        .map_err(|_| "provider-global-sync-evidence-write-failed".to_string());
    if let Err(error) = result {
        drop(file);
        let _ = std::fs::remove_file(&path);
        return Err(error);
    }
    #[cfg(unix)]
    std::fs::File::open(&directory)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| "provider-global-sync-evidence-directory-sync-failed".to_string())?;
    prune_provider_global_sync_evidence(&directory)?;
    Ok(path)
}

/// Return the earliest retained observation with the same provider blocker fingerprint.
#[cfg(not(coverage))]
pub fn provider_global_sync_blocked_since_ms(
    app_data_dir: &Path,
    report: &ProviderGlobalSyncReport,
) -> Option<u64> {
    if report.blockers.is_empty() || report.observed_at_ms == 0 {
        return None;
    }
    let current_key = provider_global_sync_blocker_key(report);
    let directory = provider_global_sync_evidence_directory(app_data_dir).ok()?;
    let mut records = std::fs::read_dir(directory)
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            name.strip_suffix(".json")?.split_once('-')?;
            Some((name, entry.path()))
        })
        .collect::<Vec<_>>();
    records.sort_by(|left, right| right.0.cmp(&left.0));
    let mut since = report.observed_at_ms;
    for (_, path) in records {
        let encoded = match std::fs::read(path) {
            Ok(encoded) => encoded,
            Err(_) => break,
        };
        let snapshot = match serde_json::from_slice::<ProviderGlobalSyncEvidenceSnapshot>(&encoded)
        {
            Ok(snapshot) => snapshot,
            Err(_) => break,
        };
        if validate_provider_global_sync_evidence_snapshot(&snapshot).is_err() {
            break;
        }
        if snapshot.provider != report.provider {
            continue;
        }
        if !snapshot.evidence_complete {
            break;
        }
        if snapshot.observed_at_ms >= report.observed_at_ms {
            continue;
        }
        if provider_global_sync_snapshot_key(&snapshot) != current_key {
            break;
        }
        since = snapshot.observed_at_ms;
    }
    Some(since)
}

fn report_has_pending_aggregate_evidence(report: &ProviderGlobalSyncReport) -> bool {
    report.upload_progress_present
        || report.download_progress_present
        || report
            .pending_indexable_count
            .is_some_and(|count| count > 0)
}

fn report_is_authoritative_clear(report: &ProviderGlobalSyncReport) -> bool {
    report_identity_is_valid(report)
        && report.evidence_complete
        && report.state == ProviderGlobalSyncState::Clear
        && report.blockers.is_empty()
        && !report_has_pending_aggregate_evidence(report)
}

pub fn require_new_copy_admission(report: &ProviderGlobalSyncReport) -> Result<(), String> {
    if !report_identity_is_valid(report)
        || (report.state == ProviderGlobalSyncState::Clear
            && report_has_pending_aggregate_evidence(report))
    {
        return Err("provider-global-sync-evidence-invalid".into());
    }
    if !report.evidence_complete {
        return Err("provider-global-sync-evidence-incomplete".into());
    }
    if report_is_authoritative_clear(report) {
        Ok(())
    } else if report.blockers.is_empty() {
        Err(format!("provider-global-sync-{}", report.state.as_str()))
    } else {
        Err(report.blockers.join(","))
    }
}

fn is_stable_provider_blocker(notice: &str) -> bool {
    notice.len() <= 128
        && notice.starts_with("provider-global-sync-")
        && notice
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

pub fn attach_new_copy_admission_notice(
    notices: &mut Vec<String>,
    report: Option<&ProviderGlobalSyncReport>,
) {
    notices.retain(|notice| !notice.starts_with("provider-global-sync-"));
    let admission_notice = match report {
        Some(report) if report_is_authoritative_clear(report) => "provider-global-sync-clear",
        Some(_) => "provider-global-sync-blocked",
        None => "provider-global-sync-evidence-unavailable",
    }
    .to_string();
    notices.push(admission_notice);
    if let Some(report) = report {
        notices.extend(
            report
                .blockers
                .iter()
                .filter(|blocker| is_stable_provider_blocker(blocker))
                .cloned(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const QUIET_DUMP: &str = r#"
com.microsoft.OneDrive.FileProvider
sync engine state:
    + scheduling state: idle
    + pending-indexable-count: 0
    + errors: 0
"#;

    const ACTIVE_DUMP: &str = r#"
com.google.drivefs.fpext
sync engine state:
    + upload progress: <gprogress:NSProgressFileOperationKindUploading>
    + download progress: <gprogress:NSProgressFileOperationKindDownloading>
    + pending-indexable-count: 12
    + scheduling state: running
"#;

    const SCHEDULER_ERROR_DUMP: &str = r#"
com.microsoft.OneDrive.FileProvider
sync engine state:
    + scheduling state: idle
    + pending-indexable-count: 0
    + errors: 0
      i:227487 create-item: error:'NSError: POSIX 63 "filename too long"'
"#;

    const RECONCILIATION_BACKLOG_DUMP: &str = r#"
com.microsoft.OneDrive.FileProvider
sync engine state:
    + scheduling state: running
    + reconciliation (277399 entries):
"#;

    const HIDDEN_DEFAULT_USER_DISABLED_DUMP: &str = r#"
com.microsoft.OneDrive.FileProvider
domain: (default) (hidden)
  + (user-disabled)
domain: personal
sync engine state:
    + pending-indexable-count: 0
    + scheduling state: idle
"#;

    const ACTIVE_USER_DISABLED_DUMP: &str = r#"
com.microsoft.OneDrive.FileProvider
domain: personal
  + (user-disabled)
sync engine state:
    + pending-indexable-count: 0
    + scheduling state: idle
"#;

    #[test]
    fn quiet_dump_is_clear_without_retaining_paths() {
        let report = parse_dump(CloudProvider::Onedrive, QUIET_DUMP).unwrap();
        assert_eq!(report.state, ProviderGlobalSyncState::Clear);
        assert!(report.evidence_complete);
        assert!(report.blockers.is_empty());
        assert!(require_new_copy_admission(&report).is_ok());
    }

    #[test]
    fn forged_report_identity_cannot_authorize_new_copy() {
        let baseline = parse_dump(CloudProvider::Onedrive, QUIET_DUMP).unwrap();

        let mut schema_drift = baseline.clone();
        schema_drift.schema_version = schema_drift.schema_version.saturating_add(1);

        let mut evidence_kind_drift = baseline.clone();
        evidence_kind_drift.evidence_kind = "forged-global-sync-evidence".into();

        let mut provider_drift = baseline;
        provider_drift.provider = CloudProvider::Icloud;

        for report in [schema_drift, evidence_kind_drift, provider_drift] {
            assert_eq!(
                require_new_copy_admission(&report).unwrap_err(),
                "provider-global-sync-evidence-invalid"
            );
        }
    }

    #[test]
    fn active_transfer_and_indexing_block_new_copy() {
        let report = parse_dump(CloudProvider::GoogleDrive, ACTIVE_DUMP).unwrap();
        assert_eq!(report.state, ProviderGlobalSyncState::Pending);
        assert_eq!(report.pending_indexable_count, Some(12));
        assert!(report
            .blockers
            .contains(&"provider-global-sync-transfer-active".into()));
        assert!(require_new_copy_admission(&report).is_err());
    }

    #[test]
    fn scheduler_error_blocks_even_when_aggregate_errors_are_zero() {
        let report = parse_dump(CloudProvider::Onedrive, SCHEDULER_ERROR_DUMP).unwrap();
        assert_eq!(report.state, ProviderGlobalSyncState::Error);
        assert!(report
            .blockers
            .contains(&"provider-global-sync-error".into()));
        assert!(report
            .blockers
            .contains(&"provider-global-sync-filename-too-long".into()));
        assert!(require_new_copy_admission(&report).is_err());
    }

    #[test]
    fn reconciliation_backlog_blocks_without_transfer_markers() {
        let report = parse_dump(CloudProvider::Onedrive, RECONCILIATION_BACKLOG_DUMP).unwrap();
        assert_eq!(report.state, ProviderGlobalSyncState::Pending);
        assert!(report
            .blockers
            .contains(&"provider-global-sync-reconciliation-pending".into()));
        assert!(require_new_copy_admission(&report).is_err());
    }

    #[test]
    fn hidden_default_domain_user_disabled_marker_is_not_global_error() {
        let report =
            parse_dump(CloudProvider::Onedrive, HIDDEN_DEFAULT_USER_DISABLED_DUMP).unwrap();
        assert_eq!(report.state, ProviderGlobalSyncState::Clear);
        assert!(report.blockers.is_empty());
        assert!(require_new_copy_admission(&report).is_ok());
    }

    #[test]
    fn active_domain_user_disabled_marker_still_blocks() {
        let report = parse_dump(CloudProvider::Onedrive, ACTIVE_USER_DISABLED_DUMP).unwrap();
        assert_eq!(report.state, ProviderGlobalSyncState::Error);
        assert!(report
            .blockers
            .contains(&"provider-global-sync-error".into()));
        assert!(require_new_copy_admission(&report).is_err());
    }

    #[test]
    fn disconnected_provider_is_error_and_fails_closed() {
        let dump = "com.google.drivefs.fpext\nsync engine state:\n temporarily disconnected: yes\n";
        let report = parse_dump(CloudProvider::GoogleDrive, dump).unwrap();
        assert_eq!(report.state, ProviderGlobalSyncState::Error);
        assert!(report
            .blockers
            .contains(&"provider-global-sync-temporarily-disconnected".into()));
        assert!(require_new_copy_admission(&report).is_err());
    }

    #[test]
    fn server_unreachable_error_is_classified_without_retaining_provider_paths() {
        let dump = "com.google.drivefs.fpext\nsync engine state:\n NSFileProviderErrorDomain Code=-1004 server unreachable\n";
        let report = parse_dump(CloudProvider::GoogleDrive, dump).unwrap();
        assert_eq!(report.state, ProviderGlobalSyncState::Error);
        assert!(report
            .blockers
            .contains(&"provider-global-sync-server-unreachable".into()));
        assert!(report
            .notices
            .iter()
            .all(|notice| !notice.contains("server unreachable")));
    }

    #[test]
    fn item_not_found_error_is_classified_without_retaining_provider_paths() {
        let dump = "com.google.drivefs.fpext\nsync engine state:\n error:'NSFileProviderErrorDomain Code=-1005 itemNotFound'\n";
        let report = parse_dump(CloudProvider::GoogleDrive, dump).unwrap();
        assert_eq!(report.state, ProviderGlobalSyncState::Error);
        assert!(report
            .blockers
            .contains(&"provider-global-sync-item-not-found".into()));
        assert!(require_new_copy_admission(&report).is_err());
    }

    #[test]
    fn local_disk_full_error_is_classified_without_retaining_provider_paths() {
        let dump =
            "com.microsoft.OneDrive.FileProvider\nsync engine state:\n error:'NSError: ODResult_Errno 28'\n";
        let report = parse_dump(CloudProvider::Onedrive, dump).unwrap();
        assert_eq!(report.state, ProviderGlobalSyncState::Error);
        assert!(report
            .blockers
            .contains(&"provider-global-sync-local-disk-full".into()));
        assert!(require_new_copy_admission(&report).is_err());
    }

    #[test]
    fn common_posix_disk_full_markers_are_classified() {
        for marker in [
            "NSError: POSIX Code=28",
            "write failed: ENOSPC",
            "OSStatus -34 (disk full)",
        ] {
            let dump = format!("com.google.drivefs.fpext\nsync engine state:\n error:'{marker}'\n");
            let report = parse_dump(CloudProvider::GoogleDrive, &dump).unwrap();
            assert!(report
                .blockers
                .contains(&"provider-global-sync-local-disk-full".into()));
        }
    }

    #[test]
    fn admission_notice_exposes_stable_blockers_without_paths() {
        let report = parse_dump(
            CloudProvider::GoogleDrive,
            "com.google.drivefs.fpext\nsync engine state:\n temporarily disconnected: yes\n",
        )
        .unwrap();
        let mut notices = vec![
            "dry-run-only".into(),
            "provider-global-sync-old/path".into(),
        ];
        attach_new_copy_admission_notice(&mut notices, Some(&report));
        assert!(notices.contains(&"provider-global-sync-blocked".into()));
        assert!(notices.contains(&"provider-global-sync-temporarily-disconnected".into()));
        assert!(notices.iter().all(|notice| !notice.contains('/')));

        attach_new_copy_admission_notice(&mut notices, Some(&report));
        assert_eq!(
            notices
                .iter()
                .filter(|notice| notice.as_str() == "provider-global-sync-temporarily-disconnected")
                .count(),
            1
        );
    }

    #[test]
    fn bounded_probe_rejects_output_beyond_limit() {
        assert!(!probe_output_is_truncated(MAX_DUMP_BYTES as usize));
        assert!(probe_output_is_truncated(MAX_DUMP_BYTES as usize + 1));
    }

    #[test]
    fn timed_out_partial_dump_is_incomplete_and_fails_closed() {
        let report = parse_dump(
            CloudProvider::Onedrive,
            &format!("{QUIET_DUMP}\n+ {PROBE_TIMEOUT_MARKER}\n"),
        )
        .unwrap();
        assert_eq!(report.state, ProviderGlobalSyncState::Unavailable);
        assert!(!report.evidence_complete);
        assert!(report.blockers.contains(&PROBE_TIMEOUT_NOTICE.into()));
        assert_eq!(
            require_new_copy_admission(&report).unwrap_err(),
            "provider-global-sync-evidence-incomplete"
        );
        assert!(report.notices.contains(&PROBE_TIMEOUT_NOTICE.into()));
    }

    #[test]
    fn malformed_or_icloud_dump_is_rejected() {
        assert!(parse_dump(CloudProvider::Onedrive, "sync engine state:").is_err());
        assert!(parse_dump(CloudProvider::Icloud, QUIET_DUMP).is_err());
    }

    #[test]
    fn provider_blocker_onset_survives_restart_without_retaining_paths() {
        let directory = tempfile::tempdir().unwrap();
        let mut first = parse_dump(CloudProvider::GoogleDrive, ACTIVE_DUMP).unwrap();
        first.observed_at_ms = 1_000;
        write_provider_global_sync_evidence(directory.path(), &first).unwrap();

        let mut second = first.clone();
        second.observed_at_ms = 2_000;
        write_provider_global_sync_evidence(directory.path(), &second).unwrap();

        assert_eq!(
            provider_global_sync_blocked_since_ms(directory.path(), &second),
            Some(1_000)
        );
        let encoded = std::fs::read_dir(
            directory
                .path()
                .join(PROVIDER_GLOBAL_SYNC_EVIDENCE_DIRECTORY),
        )
        .unwrap()
        .next()
        .unwrap()
        .unwrap();
        let contents = std::fs::read_to_string(encoded.path()).unwrap();
        assert!(!contents.contains("/Users/"));
        assert!(!contents.contains("fileproviderctl"));
    }

    #[test]
    fn provider_blocker_onset_ignores_interleaved_provider_evidence() {
        let directory = tempfile::tempdir().unwrap();
        let mut google = parse_dump(CloudProvider::GoogleDrive, ACTIVE_DUMP).unwrap();
        google.observed_at_ms = 1_000;
        write_provider_global_sync_evidence(directory.path(), &google).unwrap();

        let onedrive_dump = ACTIVE_DUMP.replace(
            "com.google.drivefs.fpext",
            "com.microsoft.OneDrive.FileProvider",
        );
        let mut onedrive = parse_dump(CloudProvider::Onedrive, &onedrive_dump).unwrap();
        onedrive.observed_at_ms = 1_500;
        write_provider_global_sync_evidence(directory.path(), &onedrive).unwrap();

        let mut later_google = google.clone();
        later_google.observed_at_ms = 2_000;
        assert_eq!(
            provider_global_sync_blocked_since_ms(directory.path(), &later_google),
            Some(1_000)
        );
    }

    #[test]
    fn malformed_older_provider_evidence_preserves_newer_onset() {
        let directory = tempfile::tempdir().unwrap();
        let mut report = parse_dump(CloudProvider::GoogleDrive, ACTIVE_DUMP).unwrap();
        report.observed_at_ms = 1_500;
        write_provider_global_sync_evidence(directory.path(), &report).unwrap();
        let malformed_path = directory
            .path()
            .join(PROVIDER_GLOBAL_SYNC_EVIDENCE_DIRECTORY)
            .join(format!("{:020}-malformed.json", 1_000));
        std::fs::write(malformed_path, b"not-json").unwrap();

        let later = ProviderGlobalSyncReport {
            observed_at_ms: 2_000,
            ..report
        };
        assert_eq!(
            provider_global_sync_blocked_since_ms(directory.path(), &later),
            Some(1_500)
        );
    }

    #[test]
    fn tampered_provider_evidence_cannot_extend_blocker_duration() {
        let directory = tempfile::tempdir().unwrap();
        let mut report = parse_dump(CloudProvider::GoogleDrive, ACTIVE_DUMP).unwrap();
        report.observed_at_ms = 1_000;
        write_provider_global_sync_evidence(directory.path(), &report).unwrap();
        let mut snapshot = provider_global_sync_evidence_snapshot_from_report(&report).unwrap();
        snapshot.observed_at_ms = 1;
        let tampered_path = directory
            .path()
            .join(PROVIDER_GLOBAL_SYNC_EVIDENCE_DIRECTORY)
            .join(format!("{:020}-{}.json", 1, "0".repeat(64)));
        std::fs::write(tampered_path, serde_json::to_vec(&snapshot).unwrap()).unwrap();
        let later = ProviderGlobalSyncReport {
            observed_at_ms: 2_000,
            ..report
        };
        assert_eq!(
            provider_global_sync_blocked_since_ms(directory.path(), &later),
            Some(1_000)
        );
    }
}
