//! Evidence-bound recovery for the current user's system-managed iCloud File Provider daemon.
//!
//! This module never edits provider databases, cloud objects, or user files. It permits only one
//! graceful SIGTERM after fresh, complete stalled-sync evidence and exact daemon identity checks.

use crate::icloud_sync_health::{
    validate_icloud_sync_health_evidence_snapshot, IcloudSyncHealthEvidenceSnapshot,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

#[cfg(target_os = "macos")]
use std::process::{Command, Stdio};
#[cfg(target_os = "macos")]
use std::thread;
#[cfg(target_os = "macos")]
use std::time::{Duration, Instant};

pub const RECOVERY_SCHEMA_VERSION: u32 = 1;
pub const FILE_PROVIDER_SERVICE_LABEL: &str = "com.apple.FileProvider";
pub const FILE_PROVIDER_EXECUTABLE: &str =
    "/System/Library/Frameworks/FileProvider.framework/Support/fileproviderd";
const MAX_EVIDENCE_AGE_MS: u64 = 5 * 60 * 1_000;
const MIN_STALE_AGE_MS: u64 = 15 * 60 * 1_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IcloudFileProviderDaemonIdentity {
    pub uid: u32,
    pub pid: i32,
    pub service_label: String,
    pub executable_path: String,
    pub executable_object_id: String,
    pub apple_signature_valid: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IcloudFileProviderRecoveryPlan {
    pub schema_version: u32,
    pub observed_at_ms: u64,
    pub health_evidence_fingerprint_sha256: String,
    pub stale_error_count: u64,
    pub oldest_stale_error_age_ms: u64,
    pub daemon: IcloudFileProviderDaemonIdentity,
    pub blockers: Vec<String>,
    pub eligible: bool,
    pub plan_fingerprint_sha256: String,
    pub exact_approval_phrase: String,
    pub mutation_performed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IcloudFileProviderRecoveryResult {
    pub schema_version: u32,
    pub plan_fingerprint_sha256: String,
    pub pre_daemon: IcloudFileProviderDaemonIdentity,
    pub post_daemon: IcloudFileProviderDaemonIdentity,
    pub graceful_sigterm_sent: bool,
    pub launchd_respawn_observed: bool,
    pub cloud_write_executed: bool,
    pub source_eviction_executed: bool,
    pub provider_database_mutated: bool,
    /// A respawn is not proof that the provider stall cleared; a fresh full health probe is required.
    pub recovery_verified: bool,
    pub sync_health_recheck_required: bool,
}

fn valid_hex64(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn plan_fingerprint(plan: &IcloudFileProviderRecoveryPlan) -> String {
    let mut unsigned = plan.clone();
    unsigned.plan_fingerprint_sha256.clear();
    unsigned.exact_approval_phrase.clear();
    let encoded = serde_json::to_vec(&unsigned).expect("recovery plan is serializable");
    let digest = Sha256::digest(encoded);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn daemon_identity_valid(daemon: &IcloudFileProviderDaemonIdentity, current_uid: u32) -> bool {
    daemon.uid == current_uid
        && daemon.pid > 1
        && daemon.service_label == FILE_PROVIDER_SERVICE_LABEL
        && daemon.executable_path == FILE_PROVIDER_EXECUTABLE
        && valid_hex64(&daemon.executable_object_id)
        && daemon.apple_signature_valid
}

pub fn plan_icloud_file_provider_recovery(
    health: &IcloudSyncHealthEvidenceSnapshot,
    daemon: IcloudFileProviderDaemonIdentity,
    current_uid: u32,
    now_ms: u64,
) -> IcloudFileProviderRecoveryPlan {
    let mut blockers = Vec::new();
    if validate_icloud_sync_health_evidence_snapshot(health).is_err() {
        blockers.push("icloud-recovery-health-evidence-invalid".into());
    }
    if now_ms < health.observed_at_ms
        || now_ms.saturating_sub(health.observed_at_ms) > MAX_EVIDENCE_AGE_MS
    {
        blockers.push("icloud-recovery-health-evidence-stale".into());
    }
    let activity = health.file_provider_activity.as_ref();
    if !health.evidence_complete
        || activity.is_none_or(|value| {
            !value.command_succeeded || value.timed_out || value.output_truncated
        })
    {
        blockers.push("icloud-recovery-activity-evidence-incomplete".into());
    }
    if activity
        .is_some_and(|value| value.active_upload_count > 0 || value.active_download_count > 0)
    {
        blockers.push("icloud-recovery-active-transfer-observed".into());
    }
    let stale_error_count = activity.map_or(0, |value| value.stale_error_count);
    let oldest_stale_error_age_ms = activity
        .and_then(|value| value.oldest_stale_error_age_ms)
        .unwrap_or(0);
    if stale_error_count == 0 || oldest_stale_error_age_ms < MIN_STALE_AGE_MS {
        blockers.push("icloud-recovery-stall-not-proven".into());
    }
    if !daemon_identity_valid(&daemon, current_uid) {
        blockers.push("icloud-recovery-daemon-identity-invalid".into());
    }
    blockers.sort();
    blockers.dedup();
    let mut plan = IcloudFileProviderRecoveryPlan {
        schema_version: RECOVERY_SCHEMA_VERSION,
        observed_at_ms: now_ms,
        health_evidence_fingerprint_sha256: health.evidence_fingerprint_sha256.clone(),
        stale_error_count,
        oldest_stale_error_age_ms,
        daemon,
        eligible: blockers.is_empty(),
        blockers,
        plan_fingerprint_sha256: String::new(),
        exact_approval_phrase: String::new(),
        mutation_performed: false,
    };
    plan.plan_fingerprint_sha256 = plan_fingerprint(&plan);
    plan.exact_approval_phrase = format!(
        "DiskSage iCloud File Provider 복구 승인 {}",
        plan.plan_fingerprint_sha256
    );
    plan
}

fn validate_plan(plan: &IcloudFileProviderRecoveryPlan) -> Result<(), String> {
    if plan.schema_version != RECOVERY_SCHEMA_VERSION
        || plan.mutation_performed
        || plan.eligible != plan.blockers.is_empty()
        || !valid_hex64(&plan.health_evidence_fingerprint_sha256)
        || plan.plan_fingerprint_sha256 != plan_fingerprint(plan)
        || plan.exact_approval_phrase
            != format!(
                "DiskSage iCloud File Provider 복구 승인 {}",
                plan.plan_fingerprint_sha256
            )
    {
        return Err("icloud-recovery-plan-invalid".into());
    }
    Ok(())
}

pub fn authorize_icloud_file_provider_recovery(
    plan: &IcloudFileProviderRecoveryPlan,
    fresh_health: &IcloudSyncHealthEvidenceSnapshot,
    fresh_daemon: &IcloudFileProviderDaemonIdentity,
    current_uid: u32,
    now_ms: u64,
    confirmation: &str,
    rationale: &str,
) -> Result<(), String> {
    validate_plan(plan)?;
    if !plan.eligible {
        return Err("icloud-recovery-plan-blocked".into());
    }
    if confirmation != plan.exact_approval_phrase {
        return Err("icloud-recovery-approval-mismatch".into());
    }
    if rationale.trim() != rationale
        || rationale.is_empty()
        || rationale.chars().count() > 1_000
        || rationale.chars().any(char::is_control)
    {
        return Err("icloud-recovery-rationale-invalid".into());
    }
    if now_ms < plan.observed_at_ms
        || now_ms.saturating_sub(plan.observed_at_ms) > MAX_EVIDENCE_AGE_MS
    {
        return Err("icloud-recovery-plan-stale".into());
    }
    let fresh_plan =
        plan_icloud_file_provider_recovery(fresh_health, fresh_daemon.clone(), current_uid, now_ms);
    if !fresh_plan.eligible {
        return Err("icloud-recovery-revalidation-blocked".into());
    }
    if fresh_daemon != &plan.daemon {
        return Err("icloud-recovery-daemon-changed".into());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn executable_object_id(path: &Path) -> Result<String, String> {
    use std::os::unix::fs::MetadataExt;
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| "icloud-recovery-daemon-metadata-unavailable".to_string())?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("icloud-recovery-daemon-object-unsafe".into());
    }
    let mut hasher = Sha256::new();
    hasher.update(metadata.dev().to_le_bytes());
    hasher.update(metadata.ino().to_le_bytes());
    hasher.update(metadata.len().to_le_bytes());
    hasher.update(metadata.mtime().to_le_bytes());
    hasher.update(metadata.mtime_nsec().to_le_bytes());
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

#[cfg(target_os = "macos")]
fn launchd_pid(uid: u32) -> Result<i32, String> {
    let service = format!("gui/{uid}/{FILE_PROVIDER_SERVICE_LABEL}");
    let output = Command::new("/bin/launchctl")
        .args(["print", &service])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|_| "icloud-recovery-launchd-observation-unavailable".to_string())?;
    if !output.status.success() || output.stdout.len() > 64 * 1024 {
        return Err("icloud-recovery-launchd-observation-unavailable".into());
    }
    let text = std::str::from_utf8(&output.stdout)
        .map_err(|_| "icloud-recovery-launchd-observation-invalid".to_string())?;
    let pids = text
        .lines()
        .filter_map(|line| line.trim().strip_prefix("pid = "))
        .map(str::parse::<i32>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "icloud-recovery-launchd-pid-invalid".to_string())?;
    match pids.as_slice() {
        [pid] if *pid > 1 => Ok(*pid),
        _ => Err("icloud-recovery-launchd-pid-ambiguous".into()),
    }
}

#[cfg(target_os = "macos")]
fn process_path(pid: i32) -> Result<String, String> {
    let mut buffer = vec![0_u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    let length = unsafe {
        libc::proc_pidpath(
            pid,
            buffer.as_mut_ptr().cast(),
            buffer.len().try_into().unwrap_or(u32::MAX),
        )
    };
    if length <= 0 {
        return Err("icloud-recovery-daemon-path-unavailable".into());
    }
    buffer.truncate(length as usize);
    std::str::from_utf8(&buffer)
        .map(str::to_owned)
        .map_err(|_| "icloud-recovery-daemon-path-invalid".into())
}

#[cfg(target_os = "macos")]
pub fn observe_icloud_file_provider_daemon() -> Result<IcloudFileProviderDaemonIdentity, String> {
    let uid = unsafe { libc::getuid() };
    let pid = launchd_pid(uid)?;
    let executable_path = process_path(pid)?;
    if executable_path != FILE_PROVIDER_EXECUTABLE {
        return Err("icloud-recovery-daemon-path-mismatch".into());
    }
    let signature = Command::new("/usr/bin/codesign")
        .args(["--verify", "--strict", FILE_PROVIDER_EXECUTABLE])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|_| "icloud-recovery-daemon-signature-unavailable".to_string())?;
    if !signature.success() {
        return Err("icloud-recovery-daemon-signature-invalid".into());
    }
    Ok(IcloudFileProviderDaemonIdentity {
        uid,
        pid,
        service_label: FILE_PROVIDER_SERVICE_LABEL.into(),
        executable_path,
        executable_object_id: executable_object_id(Path::new(FILE_PROVIDER_EXECUTABLE))?,
        apple_signature_valid: true,
    })
}

#[cfg(not(target_os = "macos"))]
pub fn observe_icloud_file_provider_daemon() -> Result<IcloudFileProviderDaemonIdentity, String> {
    Err("icloud-recovery-platform-unsupported".into())
}

#[cfg(target_os = "macos")]
pub fn execute_icloud_file_provider_recovery(
    plan: &IcloudFileProviderRecoveryPlan,
    fresh_health: &IcloudSyncHealthEvidenceSnapshot,
    now_ms: u64,
    confirmation: &str,
    rationale: &str,
) -> Result<IcloudFileProviderRecoveryResult, String> {
    let pre_daemon = observe_icloud_file_provider_daemon()?;
    authorize_icloud_file_provider_recovery(
        plan,
        fresh_health,
        &pre_daemon,
        unsafe { libc::getuid() },
        now_ms,
        confirmation,
        rationale,
    )?;
    // Revalidate immediately before signaling; PID reuse or executable replacement fails closed.
    if observe_icloud_file_provider_daemon()? != pre_daemon {
        return Err("icloud-recovery-daemon-changed".into());
    }
    if unsafe { libc::kill(pre_daemon.pid, libc::SIGTERM) } != 0 {
        return Err("icloud-recovery-sigterm-failed".into());
    }
    let deadline = Instant::now() + Duration::from_secs(20);
    let post_daemon = loop {
        if Instant::now() >= deadline {
            return Err("icloud-recovery-launchd-respawn-timeout".into());
        }
        if let Ok(observed) = observe_icloud_file_provider_daemon() {
            if observed.pid != pre_daemon.pid {
                break observed;
            }
        }
        thread::sleep(Duration::from_millis(100));
    };
    Ok(IcloudFileProviderRecoveryResult {
        schema_version: RECOVERY_SCHEMA_VERSION,
        plan_fingerprint_sha256: plan.plan_fingerprint_sha256.clone(),
        pre_daemon,
        post_daemon,
        graceful_sigterm_sent: true,
        launchd_respawn_observed: true,
        cloud_write_executed: false,
        source_eviction_executed: false,
        provider_database_mutated: false,
        recovery_verified: false,
        sync_health_recheck_required: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icloud_sync_health::{
        health_evidence_snapshot_from_report, IcloudFileProviderActivityEvidence,
        IcloudSyncHealthReport, IcloudUploadQueueSummary, ManagedDatabaseFileEvidence,
        ICLOUD_FILE_PROVIDER_ACTIVITY_SCHEMA_VERSION, ICLOUD_SYNC_HEALTH_SCHEMA_VERSION,
    };

    fn health(active: bool) -> IcloudSyncHealthEvidenceSnapshot {
        let activity = IcloudFileProviderActivityEvidence {
            schema_version: ICLOUD_FILE_PROVIDER_ACTIVITY_SCHEMA_VERSION,
            observed_at_ms: 100,
            command_succeeded: true,
            timed_out: false,
            output_truncated: false,
            no_progress_fetch_count: 0,
            no_progress_create_count: 0,
            materialization_failure_count: 0,
            staged_item_missing_count: 0,
            stale_error_count: 2,
            oldest_stale_error_age_ms: Some(3_600_000),
            sync_excluded_filename_count: 0,
            sync_excluded_root_count: 0,
            active_upload_count: u64::from(active),
            active_download_count: 0,
            active_upload_progress_millionths: active.then_some(500_000),
            active_download_progress_millionths: None,
            notices: vec!["icloud-file-provider-stale-error-observed".into()],
        };
        health_evidence_snapshot_from_report(&IcloudSyncHealthReport {
            schema_version: ICLOUD_SYNC_HEALTH_SCHEMA_VERSION,
            output_mode: "icloud-local-sync-health".into(),
            observed_at_ms: 100,
            provider: "icloud".into(),
            evidence_kind: "supplementary-local-cloud-docs-private-schema".into(),
            evidence_complete: true,
            database_snapshot_includes_wal: true,
            database_sidecar_write_permitted: false,
            managed_database_files: vec![ManagedDatabaseFileEvidence {
                role: "client.db".into(),
                present: true,
                logical_bytes: 1,
                allocated_bytes: 1,
                modified_ms: Some(1),
            }],
            managed_database_allocated_bytes: 1,
            upload_queue: IcloudUploadQueueSummary::default(),
            native_status: None,
            file_provider_activity: Some(activity),
            sync_backlog_present: true,
            new_copy_admission_state: "blocked".into(),
            new_copy_admission_blockers: vec!["icloud-file-provider-stalled".into()],
            blockers: vec!["icloud-file-provider-stalled".into()],
            notices: vec![],
            paths_redacted: true,
            user_filenames_read: false,
            user_file_contents_read: false,
            remote_capacity_verified: false,
            provider_sync_attested: false,
            local_eviction_authorized: false,
            mutation_performed: false,
        })
        .unwrap()
    }

    fn daemon() -> IcloudFileProviderDaemonIdentity {
        IcloudFileProviderDaemonIdentity {
            uid: 501,
            pid: 42,
            service_label: FILE_PROVIDER_SERVICE_LABEL.into(),
            executable_path: FILE_PROVIDER_EXECUTABLE.into(),
            executable_object_id: "a".repeat(64),
            apple_signature_valid: true,
        }
    }

    #[test]
    fn only_fresh_complete_idle_stall_and_exact_identity_authorize_recovery() {
        let plan = plan_icloud_file_provider_recovery(&health(false), daemon(), 501, 200);
        assert!(plan.eligible, "{:?}", plan.blockers);
        assert!(authorize_icloud_file_provider_recovery(
            &plan,
            &health(false),
            &daemon(),
            501,
            300,
            &plan.exact_approval_phrase,
            "정체된 시스템 데몬의 안전한 재시작"
        )
        .is_ok());

        let active = plan_icloud_file_provider_recovery(&health(true), daemon(), 501, 200);
        assert!(!active.eligible);
        assert!(active
            .blockers
            .contains(&"icloud-recovery-active-transfer-observed".into()));
        assert!(authorize_icloud_file_provider_recovery(
            &plan,
            &health(false),
            &daemon(),
            501,
            300,
            "wrong",
            "근거"
        )
        .is_err());
    }
}
