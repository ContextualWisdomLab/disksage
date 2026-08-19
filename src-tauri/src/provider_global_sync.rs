//! Read-only, bounded global File Provider queue evidence for third-party cloud roots.
//!
//! Per-item `fileproviderctl evaluate` output can report an uploaded item while the provider is
//! still processing a global upload/download queue. New copies are therefore admitted only after a
//! fresh, provider-wide dump is quiet. The dump is parsed in memory and user paths are never kept
//! in the report.

use crate::cloud::CloudProvider;
use serde::{Deserialize, Serialize};

// macOS provider dumps include bounded item summaries even with --limit-dump-size. Keep enough
// room for real OneDrive/Google Drive dumps while retaining a hard memory ceiling.
const MAX_DUMP_BYTES: u64 = 32 * 1024 * 1024;
const PROBE_TIMEOUT_MS: u64 = 20_000;

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

    let mut upload_progress_present = false;
    let mut download_progress_present = false;
    let mut pending_indexable_count = None;
    let mut needs_indexing = false;
    let mut reconciliation_pending = false;
    let mut has_error = false;
    let mut has_filename_too_long = false;
    let mut has_temporarily_disconnected = false;
    let mut has_server_unreachable = false;
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
        if has_filename_too_long
            || has_temporarily_disconnected
            || has_server_unreachable
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
    let state = if has_error {
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
    if has_error {
        blockers.push("provider-global-sync-error".into());
    }
    Ok(ProviderGlobalSyncReport {
        schema_version: PROVIDER_GLOBAL_SYNC_SCHEMA_VERSION,
        provider,
        evidence_kind: "fileproviderctl-global-dump".into(),
        evidence_complete: true,
        state,
        upload_progress_present,
        download_progress_present,
        pending_indexable_count,
        blockers,
        notices: vec![
            "provider-global-sync-dump-read-only".into(),
            "provider-global-sync-user-paths-not-retained".into(),
        ],
    })
}

#[cfg(target_os = "macos")]
fn run_dump(provider: CloudProvider) -> Result<String, String> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::{Duration, Instant};

    let identifier = provider_identifier(provider)
        .ok_or_else(|| "provider-global-sync-icloud-specialized".to_string())?;
    let mut child = Command::new("/usr/bin/fileproviderctl")
        .args(["dump", identifier, "-l"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "provider-global-sync-probe-unavailable".to_string())?;
    let Some(stdout) = child.stdout.take() else {
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
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                return Err("provider-global-sync-probe-timeout".into());
            }
            Ok(None) => thread::sleep(Duration::from_millis(50)),
            Err(_) => {
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
    report.schema_version == PROVIDER_GLOBAL_SYNC_SCHEMA_VERSION
        && report.evidence_kind == "fileproviderctl-global-dump"
        && provider_identifier(report.provider).is_some()
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
    fn malformed_or_icloud_dump_is_rejected() {
        assert!(parse_dump(CloudProvider::Onedrive, "sync engine state:").is_err());
        assert!(parse_dump(CloudProvider::Icloud, QUIET_DUMP).is_err());
    }
}
