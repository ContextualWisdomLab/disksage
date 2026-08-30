//! Read-only, bounded global File Provider queue evidence for third-party cloud roots.
//!
//! Per-item `fileproviderctl evaluate` output can report an uploaded item while the provider is
//! still processing a global upload/download queue. New copies are therefore admitted only after a
//! fresh, provider-wide dump is quiet. The dump is parsed in memory and user paths are never kept
//! in the report.

use crate::cloud::CloudProvider;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::{Path, PathBuf};

// macOS provider dumps include bounded item summaries even with --limit-dump-size. Keep enough
// room for real OneDrive/Google Drive dumps while retaining a hard memory ceiling.
const MAX_DUMP_BYTES: u64 = 32 * 1024 * 1024;
const PROBE_TIMEOUT_MS: u64 = 20_000;
const PROBE_TIMEOUT_MARKER: &str = "provider-global-sync-probe-timeout: yes";
const PROBE_TIMEOUT_NOTICE: &str = "provider-global-sync-probe-timeout";
const PROBE_RECEIPT_SCHEMA_KIND: &str = "disksage.provider-probe-receipt";
const PROBE_RECEIPT_SCHEMA_VERSION: u32 = 1;
const AUTOMATIC_PROBE_BACKOFF_MS: u64 = 5 * 60 * 1_000;
const PROBE_CHECKPOINT_SCHEMA_KIND: &str = "disksage.provider-global-sync-checkpoint";
const PROBE_CHECKPOINT_SCHEMA_VERSION: u32 = 1;

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
    pub evidence_complete: bool,
    pub state: ProviderGlobalSyncState,
    pub upload_progress_present: bool,
    pub download_progress_present: bool,
    pub pending_indexable_count: Option<u64>,
    pub blockers: Vec<String>,
    pub notices: Vec<String>,
    /// Present when the native probe cannot produce decision-grade evidence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub probe_receipt: Option<ProviderProbeReceipt>,
}

/// Path-free, bounded evidence explaining why a native probe cannot authorize a mutation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderProbeReceipt {
    pub schema_kind: String,
    pub schema_version: u32,
    pub observed_at_ms: u64,
    pub outcome: ProviderProbeOutcome,
    pub keep_local: bool,
    pub next_action: ProviderProbeNextAction,
    pub audit_reason_codes: Vec<String>,
}

/// Path-free checkpoint for bounded automatic provider re-observation.
///
/// A clear global queue permits a later item-specific check; it never authorizes local eviction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderGlobalSyncCheckpoint {
    pub schema_kind: String,
    pub schema_version: u32,
    pub provider: CloudProvider,
    pub observation_count: u64,
    pub observed_at_ms: u64,
    pub evidence_complete: bool,
    pub state: ProviderGlobalSyncState,
    pub keep_local: bool,
    pub next_automatic_probe_at_ms: Option<u64>,
    pub audit_reason_codes: Vec<String>,
    pub checkpoint_fingerprint_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderProbeOutcome {
    Inconclusive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderProbeNextAction {
    KeepLocalAndRescan,
}

pub const PROVIDER_GLOBAL_SYNC_SCHEMA_VERSION: u32 = 1;

fn checkpoint_fingerprint(checkpoint: &ProviderGlobalSyncCheckpoint) -> String {
    let mut hash = Sha256::new();
    hash.update(b"disksage.provider-global-sync-checkpoint/v1\0");
    hash.update(checkpoint.provider.as_str().as_bytes());
    hash.update(checkpoint.observation_count.to_be_bytes());
    hash.update(checkpoint.observed_at_ms.to_be_bytes());
    hash.update([checkpoint.evidence_complete as u8]);
    hash.update(checkpoint.state.as_str().as_bytes());
    hash.update([checkpoint.keep_local as u8]);
    hash.update(
        checkpoint
            .next_automatic_probe_at_ms
            .unwrap_or(u64::MAX)
            .to_be_bytes(),
    );
    for reason in &checkpoint.audit_reason_codes {
        hash.update((reason.len() as u64).to_be_bytes());
        hash.update(reason.as_bytes());
    }
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn validate_probe_checkpoint(checkpoint: &ProviderGlobalSyncCheckpoint) -> Result<(), String> {
    let blocked =
        !(checkpoint.evidence_complete && checkpoint.state == ProviderGlobalSyncState::Clear);
    if checkpoint.schema_kind != PROBE_CHECKPOINT_SCHEMA_KIND
        || checkpoint.schema_version != PROBE_CHECKPOINT_SCHEMA_VERSION
        || checkpoint.observation_count == 0
        || checkpoint.observed_at_ms == 0
        || checkpoint.keep_local != blocked
        || blocked != checkpoint.next_automatic_probe_at_ms.is_some()
        || checkpoint.next_automatic_probe_at_ms.is_some_and(|next| {
            next != checkpoint
                .observed_at_ms
                .saturating_add(AUTOMATIC_PROBE_BACKOFF_MS)
        })
        || checkpoint.audit_reason_codes.len() > 32
        || checkpoint
            .audit_reason_codes
            .iter()
            .any(|reason| !probe_reason_code_is_valid(reason))
        || checkpoint_fingerprint(checkpoint) != checkpoint.checkpoint_fingerprint_sha256
    {
        return Err("provider-global-sync-checkpoint-invalid".into());
    }
    Ok(())
}

pub fn checkpoint_report(
    report: &ProviderGlobalSyncReport,
    observed_at_ms: u64,
    previous: Option<&ProviderGlobalSyncCheckpoint>,
) -> Result<ProviderGlobalSyncCheckpoint, String> {
    validate_report_evidence(report)?;
    if observed_at_ms == 0 {
        return Err("provider-global-sync-checkpoint-time-invalid".into());
    }
    if report
        .probe_receipt
        .as_ref()
        .is_some_and(|receipt| receipt.observed_at_ms != observed_at_ms)
    {
        return Err("provider-global-sync-checkpoint-time-mismatch".into());
    }
    let observation_count = if let Some(previous) = previous {
        validate_probe_checkpoint(previous)?;
        if previous.provider != report.provider || observed_at_ms <= previous.observed_at_ms {
            return Err("provider-global-sync-checkpoint-lineage-invalid".into());
        }
        previous
            .observation_count
            .checked_add(1)
            .ok_or_else(|| "provider-global-sync-checkpoint-count-overflow".to_string())?
    } else {
        1
    };
    let keep_local = !(report.evidence_complete && report.state == ProviderGlobalSyncState::Clear);
    let mut audit_reason_codes = report.blockers.clone();
    audit_reason_codes.sort();
    audit_reason_codes.dedup();
    let mut checkpoint = ProviderGlobalSyncCheckpoint {
        schema_kind: PROBE_CHECKPOINT_SCHEMA_KIND.into(),
        schema_version: PROBE_CHECKPOINT_SCHEMA_VERSION,
        provider: report.provider,
        observation_count,
        observed_at_ms,
        evidence_complete: report.evidence_complete,
        state: report.state,
        keep_local,
        next_automatic_probe_at_ms: keep_local
            .then(|| observed_at_ms.saturating_add(AUTOMATIC_PROBE_BACKOFF_MS)),
        audit_reason_codes,
        checkpoint_fingerprint_sha256: String::new(),
    };
    checkpoint.checkpoint_fingerprint_sha256 = checkpoint_fingerprint(&checkpoint);
    validate_probe_checkpoint(&checkpoint)?;
    Ok(checkpoint)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProviderCheckpointDocument {
    checkpoint: ProviderGlobalSyncCheckpoint,
    report: ProviderGlobalSyncReport,
}

fn checkpoint_path(app_data_dir: &Path, provider: CloudProvider) -> Result<PathBuf, String> {
    if !app_data_dir.is_absolute() || provider_identifier(provider).is_none() {
        return Err("provider-global-sync-checkpoint-path-invalid".into());
    }
    Ok(app_data_dir.join(format!("provider-global-sync-{}.json", provider.as_str())))
}

fn load_checkpoint_document(
    app_data_dir: &Path,
    provider: CloudProvider,
) -> Result<Option<ProviderCheckpointDocument>, String> {
    let path = checkpoint_path(app_data_dir, provider)?;
    let metadata = match std::fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("provider-global-sync-checkpoint-read-failed".into()),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > 256 * 1024 {
        return Err("provider-global-sync-checkpoint-file-invalid".into());
    }
    let document: ProviderCheckpointDocument = serde_json::from_slice(
        &std::fs::read(path).map_err(|_| "provider-global-sync-checkpoint-read-failed")?,
    )
    .map_err(|_| "provider-global-sync-checkpoint-decode-failed")?;
    validate_probe_checkpoint(&document.checkpoint)?;
    validate_report_evidence(&document.report)?;
    if document.checkpoint.provider != provider || document.report.provider != provider {
        return Err("provider-global-sync-checkpoint-provider-invalid".into());
    }
    Ok(Some(document))
}

fn persist_checkpoint_document(
    app_data_dir: &Path,
    document: &ProviderCheckpointDocument,
) -> Result<(), String> {
    std::fs::create_dir_all(app_data_dir)
        .map_err(|_| "provider-global-sync-checkpoint-directory-failed")?;
    let path = checkpoint_path(app_data_dir, document.checkpoint.provider)?;
    let mut nonce = [0_u8; 16];
    getrandom::fill(&mut nonce).map_err(|_| "provider-global-sync-checkpoint-random-failed")?;
    let nonce = nonce
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let temporary = path.with_extension(format!("{nonce}.tmp"));
    let encoded = serde_json::to_vec_pretty(document)
        .map_err(|_| "provider-global-sync-checkpoint-encode-failed")?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temporary)
        .map_err(|_| "provider-global-sync-checkpoint-create-failed")?;
    if file.write_all(&encoded).is_err() || file.sync_all().is_err() {
        let _ = std::fs::remove_file(&temporary);
        return Err("provider-global-sync-checkpoint-write-failed".into());
    }
    #[cfg(windows)]
    if path.exists() {
        std::fs::remove_file(&path)
            .map_err(|_| "provider-global-sync-checkpoint-replace-failed")?;
    }
    std::fs::rename(&temporary, &path).map_err(|_| {
        let _ = std::fs::remove_file(&temporary);
        "provider-global-sync-checkpoint-replace-failed".to_string()
    })
}

pub(crate) fn inspect_new_copy_admission_checkpointed_with<F>(
    provider: CloudProvider,
    app_data_dir: &Path,
    now_ms: u64,
    probe: F,
) -> Result<ProviderGlobalSyncReport, String>
where
    F: FnOnce() -> Result<ProviderGlobalSyncReport, String>,
{
    let previous = load_checkpoint_document(app_data_dir, provider)?;
    if let Some(previous) = previous.as_ref() {
        if !previous.checkpoint.keep_local || !automatic_probe_due(&previous.checkpoint, now_ms)? {
            return Ok(previous.report.clone());
        }
    }
    let report = probe()?;
    let observed_at_ms = report
        .probe_receipt
        .as_ref()
        .map_or(now_ms, |receipt| receipt.observed_at_ms);
    let checkpoint = checkpoint_report(
        &report,
        observed_at_ms,
        previous.as_ref().map(|document| &document.checkpoint),
    )?;
    persist_checkpoint_document(
        app_data_dir,
        &ProviderCheckpointDocument {
            checkpoint,
            report: report.clone(),
        },
    )?;
    Ok(report)
}

pub fn inspect_new_copy_admission_checkpointed(
    provider: CloudProvider,
    app_data_dir: &Path,
    now_ms: u64,
) -> Result<ProviderGlobalSyncReport, String> {
    inspect_new_copy_admission_checkpointed_with(provider, app_data_dir, now_ms, || {
        inspect_new_copy_admission(provider)
    })
}

pub fn automatic_probe_due(
    checkpoint: &ProviderGlobalSyncCheckpoint,
    now_ms: u64,
) -> Result<bool, String> {
    validate_probe_checkpoint(checkpoint)?;
    Ok(checkpoint
        .next_automatic_probe_at_ms
        .is_some_and(|next| now_ms >= next))
}

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

fn probe_reason_code_is_valid(reason: &str) -> bool {
    !reason.is_empty()
        && reason.len() <= 128
        && reason.starts_with("provider-global-sync-")
        && reason
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn inconclusive_probe_receipt(observed_at_ms: u64, reason: &str) -> ProviderProbeReceipt {
    ProviderProbeReceipt {
        schema_kind: PROBE_RECEIPT_SCHEMA_KIND.into(),
        schema_version: PROBE_RECEIPT_SCHEMA_VERSION,
        observed_at_ms,
        outcome: ProviderProbeOutcome::Inconclusive,
        keep_local: true,
        next_action: ProviderProbeNextAction::KeepLocalAndRescan,
        audit_reason_codes: vec![reason.into()],
    }
}

fn probe_receipt_is_consistent(report: &ProviderGlobalSyncReport) -> bool {
    let Some(receipt) = report.probe_receipt.as_ref() else {
        return report.evidence_complete && report.state != ProviderGlobalSyncState::Unavailable;
    };
    !report.evidence_complete
        && report.state == ProviderGlobalSyncState::Unavailable
        && receipt.schema_kind == PROBE_RECEIPT_SCHEMA_KIND
        && receipt.schema_version == PROBE_RECEIPT_SCHEMA_VERSION
        && receipt.observed_at_ms > 0
        && receipt.outcome == ProviderProbeOutcome::Inconclusive
        && receipt.keep_local
        && receipt.next_action == ProviderProbeNextAction::KeepLocalAndRescan
        && !receipt.audit_reason_codes.is_empty()
        && receipt.audit_reason_codes.len() <= 16
        && receipt.audit_reason_codes.iter().all(|reason| {
            probe_reason_code_is_valid(reason)
                && report.blockers.iter().any(|blocker| blocker == reason)
        })
}

/// Validate the bounded identity and receipt relationship of one provider report.
///
/// This deliberately does not require complete evidence: callers that only need to transport a
/// blocked/inconclusive report can validate its shape without accidentally authorizing a copy.
pub fn validate_report_evidence(report: &ProviderGlobalSyncReport) -> Result<(), String> {
    if report.schema_version != PROVIDER_GLOBAL_SYNC_SCHEMA_VERSION
        || report.evidence_kind != "fileproviderctl-global-dump"
        || provider_identifier(report.provider).is_none()
        || !probe_receipt_is_consistent(report)
    {
        return Err("provider-global-sync-evidence-invalid".into());
    }
    Ok(())
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
                || contains_bounded_numeric_marker(&marker_lower, "osstatus ", "-34")
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
        evidence_complete: !probe_timed_out,
        state,
        upload_progress_present,
        download_progress_present,
        pending_indexable_count,
        blockers,
        notices,
        probe_receipt: probe_timed_out.then(|| {
            inconclusive_probe_receipt(crate::cloud::system_now_ms(), PROBE_TIMEOUT_NOTICE)
        }),
    })
}

fn inconclusive_probe_report(
    provider: CloudProvider,
    observed_at_ms: u64,
    reason: &str,
) -> ProviderGlobalSyncReport {
    ProviderGlobalSyncReport {
        schema_version: PROVIDER_GLOBAL_SYNC_SCHEMA_VERSION,
        provider,
        evidence_kind: "fileproviderctl-global-dump".into(),
        evidence_complete: false,
        state: ProviderGlobalSyncState::Unavailable,
        upload_progress_present: false,
        download_progress_present: false,
        pending_indexable_count: None,
        blockers: vec![reason.into()],
        notices: vec![
            "provider-global-sync-dump-read-only".into(),
            "provider-global-sync-user-paths-not-retained".into(),
        ],
        probe_receipt: Some(inconclusive_probe_receipt(observed_at_ms, reason)),
    }
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
    let output = match run_dump(provider) {
        Ok(output) => output,
        Err(error) if error == "provider-global-sync-probe-timeout" => {
            return Ok(inconclusive_probe_report(
                provider,
                crate::cloud::system_now_ms(),
                "provider-global-sync-probe-timeout",
            ));
        }
        Err(error) => return Err(error),
    };
    if output.trim().is_empty() {
        return Ok(inconclusive_probe_report(
            provider,
            crate::cloud::system_now_ms(),
            "provider-global-sync-probe-empty-output",
        ));
    }
    parse_dump(provider, &output)
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
    validate_report_evidence(report).is_ok()
}

pub(crate) fn report_receipt_is_consistent(report: &ProviderGlobalSyncReport) -> bool {
    match &report.probe_receipt {
        None => report.evidence_complete && report.state != ProviderGlobalSyncState::Unavailable,
        Some(receipt) => {
            !report.evidence_complete
                && report.state == ProviderGlobalSyncState::Unavailable
                && !report.blockers.is_empty()
                && receipt.schema_kind == "disksage.provider-probe-receipt"
                && receipt.schema_version == 1
                && receipt.observed_at_ms > 0
                && receipt.outcome == ProviderProbeOutcome::Inconclusive
                && receipt.keep_local
                && receipt.next_action == ProviderProbeNextAction::KeepLocalAndRescan
                && !receipt.audit_reason_codes.is_empty()
                && receipt.audit_reason_codes.iter().all(|reason| {
                    is_stable_provider_blocker(reason) && report.blockers.contains(reason)
                })
        }
    }
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
        && report.probe_receipt.is_none()
        && report.evidence_complete
        && report.state == ProviderGlobalSyncState::Clear
        && report.blockers.is_empty()
        && !report_has_pending_aggregate_evidence(report)
}

pub fn require_new_copy_admission(report: &ProviderGlobalSyncReport) -> Result<(), String> {
    if !report_identity_is_valid(report)
        || !report_receipt_is_consistent(report)
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
        let receipt = report
            .probe_receipt
            .expect("partial timeout must carry its receipt");
        assert_eq!(receipt.outcome, ProviderProbeOutcome::Inconclusive);
        assert!(receipt.keep_local);
        assert_eq!(receipt.audit_reason_codes, vec![PROBE_TIMEOUT_NOTICE]);
    }

    #[test]
    fn empty_or_timed_out_probe_has_bounded_keep_local_receipt() {
        for reason in [
            "provider-global-sync-probe-empty-output",
            "provider-global-sync-probe-timeout",
        ] {
            let report = inconclusive_probe_report(CloudProvider::Onedrive, 42, reason);
            assert_eq!(report.state, ProviderGlobalSyncState::Unavailable);
            assert!(!report.evidence_complete);
            assert_eq!(report.blockers, vec![reason]);
            let receipt = report.probe_receipt.unwrap();
            assert_eq!(receipt.observed_at_ms, 42);
            assert_eq!(receipt.outcome, ProviderProbeOutcome::Inconclusive);
            assert!(receipt.keep_local);
            assert_eq!(
                receipt.next_action,
                ProviderProbeNextAction::KeepLocalAndRescan
            );
            assert_eq!(receipt.audit_reason_codes, vec![reason]);
            let encoded = serde_json::to_string(&receipt).unwrap();
            assert!(!encoded.contains('/'));
            assert!(encoded.len() < 1_024);
        }
    }

    #[test]
    fn contradictory_clear_receipt_is_invalid() {
        let mut report = parse_dump(CloudProvider::Onedrive, QUIET_DUMP).unwrap();
        report.probe_receipt = Some(inconclusive_probe_receipt(42, PROBE_TIMEOUT_NOTICE));
        assert_eq!(
            require_new_copy_admission(&report).unwrap_err(),
            "provider-global-sync-evidence-invalid"
        );
    }

    #[test]
    fn blocked_probe_checkpoint_backs_off_and_preserves_lineage() {
        let report = inconclusive_probe_report(
            CloudProvider::Onedrive,
            1_000,
            "provider-global-sync-probe-timeout",
        );
        let first = checkpoint_report(&report, 1_000, None).unwrap();
        assert!(first.keep_local);
        assert_eq!(first.observation_count, 1);
        assert_eq!(first.next_automatic_probe_at_ms, Some(301_000));
        assert!(!automatic_probe_due(&first, 300_999).unwrap());
        assert!(automatic_probe_due(&first, 301_000).unwrap());

        let fresh_report = inconclusive_probe_report(
            CloudProvider::Onedrive,
            301_000,
            "provider-global-sync-probe-timeout",
        );
        let second = checkpoint_report(&fresh_report, 301_000, Some(&first)).unwrap();
        assert_eq!(second.observation_count, 2);
        assert_ne!(
            first.checkpoint_fingerprint_sha256,
            second.checkpoint_fingerprint_sha256
        );
    }

    #[test]
    fn checkpoint_rejects_a_timestamp_newer_than_inconclusive_evidence() {
        let report = inconclusive_probe_report(
            CloudProvider::Onedrive,
            1_000,
            "provider-global-sync-probe-timeout",
        );
        assert_eq!(
            checkpoint_report(&report, 301_000, None).unwrap_err(),
            "provider-global-sync-checkpoint-time-mismatch"
        );
    }

    #[test]
    fn checkpointed_inspection_persists_and_delays_blocked_reobservation() {
        let directory = tempfile::tempdir().unwrap();
        let probes = std::cell::Cell::new(0_u32);
        let first = inspect_new_copy_admission_checkpointed_with(
            CloudProvider::Onedrive,
            directory.path(),
            1_000,
            || {
                probes.set(probes.get() + 1);
                Ok(inconclusive_probe_report(
                    CloudProvider::Onedrive,
                    1_000,
                    "provider-global-sync-probe-timeout",
                ))
            },
        )
        .unwrap();
        assert!(!first.evidence_complete);

        let cached = inspect_new_copy_admission_checkpointed_with(
            CloudProvider::Onedrive,
            directory.path(),
            300_999,
            || -> Result<ProviderGlobalSyncReport, String> {
                panic!("a blocked provider must respect the persisted backoff")
            },
        )
        .unwrap();
        assert_eq!(cached, first);
        assert_eq!(probes.get(), 1);

        let fresh = inspect_new_copy_admission_checkpointed_with(
            CloudProvider::Onedrive,
            directory.path(),
            301_000,
            || {
                probes.set(probes.get() + 1);
                Ok(parse_dump(CloudProvider::Onedrive, QUIET_DUMP).unwrap())
            },
        )
        .unwrap();
        assert!(fresh.evidence_complete);
        assert_eq!(fresh.state, ProviderGlobalSyncState::Clear);
        assert_eq!(probes.get(), 2);

        let stopped = inspect_new_copy_admission_checkpointed_with(
            CloudProvider::Onedrive,
            directory.path(),
            u64::MAX,
            || -> Result<ProviderGlobalSyncReport, String> {
                panic!("a complete clear checkpoint stops automatic polling")
            },
        )
        .unwrap();
        assert_eq!(stopped, fresh);
    }

    #[test]
    fn clear_global_checkpoint_stops_polling_without_granting_eviction() {
        let report = parse_dump(CloudProvider::Onedrive, QUIET_DUMP).unwrap();
        let checkpoint = checkpoint_report(&report, 2_000, None).unwrap();

        assert!(!checkpoint.keep_local);
        assert_eq!(checkpoint.next_automatic_probe_at_ms, None);
        assert!(!automatic_probe_due(&checkpoint, u64::MAX).unwrap());
        assert!(checkpoint.audit_reason_codes.is_empty());
    }

    #[test]
    fn checkpoint_rejects_provider_or_time_drift() {
        let report = parse_dump(CloudProvider::Onedrive, QUIET_DUMP).unwrap();
        let first = checkpoint_report(&report, 2_000, None).unwrap();
        assert_eq!(
            checkpoint_report(&report, 2_000, Some(&first)).unwrap_err(),
            "provider-global-sync-checkpoint-lineage-invalid"
        );
        let google = parse_dump(
            CloudProvider::GoogleDrive,
            &QUIET_DUMP.replace(
                "com.microsoft.OneDrive.FileProvider",
                "com.google.drivefs.fpext",
            ),
        )
        .unwrap();
        assert_eq!(
            checkpoint_report(&google, 3_000, Some(&first)).unwrap_err(),
            "provider-global-sync-checkpoint-lineage-invalid"
        );
    }

    #[test]
    fn malformed_or_icloud_dump_is_rejected() {
        assert!(parse_dump(CloudProvider::Onedrive, "sync engine state:").is_err());
        assert!(parse_dump(CloudProvider::Icloud, QUIET_DUMP).is_err());
    }
}
