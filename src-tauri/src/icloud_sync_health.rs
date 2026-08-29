//! Read-only, path-free health evidence for the local macOS CloudDocs sync database.
//!
//! This module intentionally treats CloudDocs' private SQLite schema as supplementary local
//! evidence. A quiet global queue is not per-item remote-upload attestation and never authorizes
//! local eviction.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use std::io::Read;
#[cfg(not(coverage))]
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

const SQLITE3_PATH: &str = "/usr/bin/sqlite3";
#[cfg(target_os = "macos")]
const CP_PATH: &str = "/bin/cp";
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const SNAPSHOT_COPY_TIMEOUT: Duration = Duration::from_secs(5);
const SNAPSHOT_ATTEMPTS: usize = 3;
// CloudDocs' managed SQLite database can grow to many GiB. Never clone a database larger than
// this bounded amount during a read-only health probe: the immutable fallback below is slower and
// less complete, but it cannot unexpectedly consume the user's remaining disk while planning.
const MAX_SNAPSHOT_SOURCE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_STDOUT_BYTES: usize = 16 * 1024;
const MAX_STDERR_BYTES: usize = 4 * 1024;
const BRCTL_STATUS_PATH: &str = "/usr/bin/brctl";
const BRCTL_STATUS_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_BRCTL_STATUS_BYTES: usize = 64 * 1024;
#[cfg(target_os = "macos")]
const FILEPROVIDERCTL_PATH: &str = "/usr/bin/fileproviderctl";
#[cfg(target_os = "macos")]
// fileproviderctl prints global sync-engine progress after the per-item detail section. Keep the
// probe bounded, but allow enough time to observe that active-transfer evidence before failing.
const FILEPROVIDER_DUMP_TIMEOUT: Duration = Duration::from_secs(30);
#[cfg(target_os = "macos")]
// Keep the sync summary and a larger bounded provider-error window together; iCloud places
// filename/root exclusion diagnostics after the aggregate summary in large dumps.
const MAX_FILEPROVIDER_DUMP_BYTES: usize = 1024 * 1024;
const ITEM_ERROR_AGE_NOTICE_MS: u64 = 86_400_000;
const FILE_PROVIDER_STALE_ERROR_AGE_MS: u64 = 15 * 60 * 1_000;
static SNAPSHOT_NONCE: AtomicU64 = AtomicU64::new(0);

pub const ICLOUD_SYNC_HEALTH_SCHEMA_VERSION: u32 = 5;
pub const ICLOUD_NATIVE_STATUS_SCHEMA_VERSION: u32 = 1;
pub const ICLOUD_FILE_PROVIDER_ACTIVITY_SCHEMA_VERSION: u32 = 3;
pub const ICLOUD_SYNC_HEALTH_EVIDENCE_SCHEMA_VERSION: u32 = 1;
pub const ICLOUD_SYNC_HEALTH_EVIDENCE_DIRECTORY: &str = "icloud-sync-health-evidence";
const MAX_PERSISTED_HEALTH_SNAPSHOTS: usize = 128;
const MAX_PERSISTED_HEALTH_SNAPSHOT_BYTES: usize = 64 * 1024;

const QUEUE_QUERY: &str = r#"
PRAGMA query_only=ON;
SELECT 'scheduled_waiting', count(*), coalesce(sum(transfer_size),0)
FROM client_uploads INDEXED BY "client_uploads/scheduling_by_priority"
WHERE throttle_state=1 AND transfer_operation IS NULL;
SELECT 'scheduled_active', count(*), coalesce(sum(transfer_size),0)
FROM client_uploads INDEXED BY "client_uploads/transfer_operation"
WHERE transfer_operation IS NOT NULL;
SELECT 'blocked_sync_up', count(*), 0
FROM client_uploads INDEXED BY "client_uploads/blocked_on_sync_up_state_index"
WHERE throttle_state=31;
SELECT 'out_of_quota', count(*), coalesce(sum(transfer_size),0)
FROM client_uploads INDEXED BY "client_uploads/out_of_quota_index"
WHERE throttle_state=32;
SELECT 'gc', count(*), 0
FROM client_uploads INDEXED BY "client_uploads/gc_index"
WHERE throttle_state=0;
SELECT 'other_state', count(*), 0
FROM client_uploads INDEXED BY "client_uploads/throttle_state"
WHERE throttle_state NOT IN (0,1,31,32);
SELECT 'item_error', count(*), 0
FROM item_errors;
SELECT 'item_error_octagon_not_signed_in', count(*),
       coalesce(max(cast(strftime('%s', error_timestamp) as integer) * 1000),0)
FROM item_errors
WHERE error_domain='com.apple.security.octagon' AND error_code=25;
SELECT 'item_error_unclassified', count(*),
       coalesce(max(cast(strftime('%s', error_timestamp) as integer) * 1000),0)
FROM item_errors
WHERE NOT (error_domain='com.apple.security.octagon' AND error_code=25);
"#;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagedDatabaseFileEvidence {
    pub role: String,
    pub present: bool,
    pub logical_bytes: u64,
    pub allocated_bytes: u64,
    pub modified_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct IcloudUploadQueueSummary {
    pub scheduled_waiting_count: u64,
    pub scheduled_waiting_bytes: u64,
    pub scheduled_active_count: u64,
    pub scheduled_active_bytes: u64,
    pub blocked_on_sync_up_count: u64,
    pub out_of_quota_count: u64,
    pub out_of_quota_bytes: u64,
    pub garbage_collection_count: u64,
    pub other_state_count: u64,
    pub item_error_count: u64,
    pub item_error_octagon_not_signed_in_count: u64,
    pub item_error_unclassified_count: u64,
    pub newest_item_error_timestamp_ms: Option<u64>,
    pub scheduled_count: u64,
    pub scheduled_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IcloudNativeStatusEvidence {
    pub schema_version: u32,
    pub observed_at_ms: u64,
    pub command_succeeded: bool,
    pub timed_out: bool,
    pub output_truncated: bool,
    pub status_observed: bool,
    pub evidence_complete: bool,
    pub container_count: Option<u64>,
    pub client_state: Option<String>,
    pub server_state: Option<String>,
    pub sync_state: Option<String>,
    pub last_sync_present: bool,
    pub notices: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IcloudFileProviderActivityEvidence {
    pub schema_version: u32,
    pub observed_at_ms: u64,
    pub command_succeeded: bool,
    pub timed_out: bool,
    pub output_truncated: bool,
    pub no_progress_fetch_count: u64,
    #[serde(default)]
    pub no_progress_create_count: u64,
    #[serde(default)]
    pub materialization_failure_count: u64,
    #[serde(default)]
    pub staged_item_missing_count: u64,
    /// Aggregate provider errors where iCloud excludes an item because of its filename.
    #[serde(default)]
    pub sync_excluded_filename_count: u64,
    /// Aggregate provider errors where iCloud excludes an item under a sync root.
    #[serde(default)]
    pub sync_excluded_root_count: u64,
    #[serde(default)]
    pub active_upload_count: u64,
    #[serde(default)]
    pub active_download_count: u64,
    #[serde(default)]
    pub active_upload_progress_millionths: Option<u32>,
    #[serde(default)]
    pub active_download_progress_millionths: Option<u32>,
    pub notices: Vec<String>,
}

pub fn validate_file_provider_activity_evidence(
    evidence: &IcloudFileProviderActivityEvidence,
) -> Result<(), String> {
    if evidence.schema_version != ICLOUD_FILE_PROVIDER_ACTIVITY_SCHEMA_VERSION
        || evidence.notices.is_empty()
        || evidence.notices.iter().any(|notice| {
            notice.is_empty()
                || notice.len() > 128
                || !notice
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
        || evidence.active_upload_progress_millionths.is_some_and(|value| value > 1_000_000)
        || evidence
            .active_download_progress_millionths
            .is_some_and(|value| value > 1_000_000)
    {
        return Err("icloud-file-provider-activity-shape-invalid".into());
    }
    Ok(())
}

pub fn validate_native_status_evidence(
    evidence: &IcloudNativeStatusEvidence,
) -> Result<(), String> {
    if evidence.schema_version != ICLOUD_NATIVE_STATUS_SCHEMA_VERSION
        || evidence.notices.is_empty()
        || evidence.notices.iter().any(|notice| {
            notice.is_empty()
                || notice.len() > 128
                || !notice
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
        || [
            evidence.client_state.as_deref(),
            evidence.server_state.as_deref(),
            evidence.sync_state.as_deref(),
        ]
        .into_iter()
        .flatten()
        .any(|value| bounded_native_status_token(value).is_none())
    {
        return Err("icloud-native-status-shape-invalid".into());
    }
    let status_observed = evidence.client_state.is_some()
        || evidence.server_state.is_some()
        || evidence.sync_state.is_some();
    let summary_complete = evidence.container_count.is_some()
        && evidence.client_state.is_some()
        && evidence.server_state.is_some()
        && evidence.sync_state.is_some();
    if evidence.status_observed != status_observed || evidence.evidence_complete != summary_complete
    {
        return Err("icloud-native-status-completeness-invalid".into());
    }
    Ok(())
}

pub fn native_sync_up_pending(evidence: &IcloudNativeStatusEvidence) -> bool {
    evidence.status_observed
        && evidence
            .sync_state
            .as_deref()
            .is_some_and(|state| state.split('|').any(|value| value == "needs-sync-up"))
}

pub fn native_sync_down_pending(evidence: &IcloudNativeStatusEvidence) -> bool {
    evidence.status_observed
        && evidence
            .sync_state
            .as_deref()
            .is_some_and(|state| state.split('|').any(|value| value == "needs-sync-down"))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IcloudSyncHealthReport {
    pub schema_version: u32,
    pub output_mode: String,
    pub observed_at_ms: u64,
    pub provider: String,
    pub evidence_kind: String,
    pub evidence_complete: bool,
    pub database_snapshot_includes_wal: bool,
    /// Writes are never permitted against Apple's managed source database or its sidecars.
    /// SQLite may create or update sidecars only beside the private temporary snapshot.
    pub database_sidecar_write_permitted: bool,
    pub managed_database_files: Vec<ManagedDatabaseFileEvidence>,
    pub managed_database_allocated_bytes: u64,
    pub upload_queue: IcloudUploadQueueSummary,
    #[serde(default)]
    pub native_status: Option<IcloudNativeStatusEvidence>,
    #[serde(default)]
    pub file_provider_activity: Option<IcloudFileProviderActivityEvidence>,
    pub sync_backlog_present: bool,
    /// Admission state for adding a new local item to iCloud Drive.
    ///
    /// This is intentionally narrower than sync completion: a clear global queue only permits a
    /// new copy to proceed to the independent capacity, review, and integrity gates.
    pub new_copy_admission_state: String,
    pub new_copy_admission_blockers: Vec<String>,
    pub blockers: Vec<String>,
    pub notices: Vec<String>,
    pub paths_redacted: bool,
    pub user_filenames_read: bool,
    pub user_file_contents_read: bool,
    pub remote_capacity_verified: bool,
    pub provider_sync_attested: bool,
    pub local_eviction_authorized: bool,
    pub mutation_performed: bool,
}

/// Path-free, aggregate iCloud health evidence retained for cross-loop comparison.
///
/// This projection deliberately excludes managed database filenames, paths, item identifiers,
/// and raw provider output. It is a durable observation only: it never claims remote capacity,
/// per-item upload completion, cloud write, or source-eviction authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IcloudSyncHealthEvidenceSnapshot {
    pub schema_version: u32,
    pub observed_at_ms: u64,
    pub evidence_complete: bool,
    pub database_snapshot_includes_wal: bool,
    pub managed_database_allocated_bytes: u64,
    pub upload_queue: IcloudUploadQueueSummary,
    pub sync_backlog_present: bool,
    pub new_copy_admission_state: String,
    pub new_copy_admission_blockers: Vec<String>,
    pub blockers: Vec<String>,
    #[serde(default)]
    pub native_status: Option<IcloudNativeStatusEvidence>,
    #[serde(default)]
    pub file_provider_activity: Option<IcloudFileProviderActivityEvidence>,
    pub evidence_fingerprint_sha256: String,
}

fn health_evidence_code_is_valid(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || (byte == b'-' && index > 0)
        })
        && !value.ends_with('-')
        && !value.contains("--")
}

fn health_evidence_fingerprint(
    snapshot: &IcloudSyncHealthEvidenceSnapshot,
) -> Result<String, String> {
    let mut unsigned = snapshot.clone();
    unsigned.evidence_fingerprint_sha256.clear();
    let encoded = serde_json::to_vec(&unsigned)
        .map_err(|_| "icloud-sync-health-evidence-fingerprint-encode-failed".to_string())?;
    let digest = Sha256::digest(encoded);
    Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// Project a live report into the bounded, path-free durable evidence shape.
pub fn health_evidence_snapshot_from_report(
    report: &IcloudSyncHealthReport,
) -> Result<IcloudSyncHealthEvidenceSnapshot, String> {
    let managed_database_roles_valid = report.managed_database_files.iter().all(|file| {
        matches!(
            file.role.as_str(),
            "client.db"
                | "client.db-shm"
                | "client.db-wal"
                | "server.db"
                | "server.db-shm"
                | "server.db-wal"
        )
    });
    let admission_state_valid = matches!(
        report.new_copy_admission_state.as_str(),
        "clear" | "blocked"
    );
    let admission_blockers_consistent = (report.new_copy_admission_state == "clear")
        == report.new_copy_admission_blockers.is_empty();
    let blocker_codes_valid = report
        .new_copy_admission_blockers
        .iter()
        .chain(report.blockers.iter())
        .all(|code| health_evidence_code_is_valid(code));
    if report.schema_version != ICLOUD_SYNC_HEALTH_SCHEMA_VERSION
        || report.output_mode != "icloud-local-sync-health"
        || report.provider != "icloud"
        || report.evidence_kind != "supplementary-local-cloud-docs-private-schema"
        || report.observed_at_ms == 0
        || report.database_sidecar_write_permitted
        || !report.paths_redacted
        || report.user_filenames_read
        || report.user_file_contents_read
        || report.remote_capacity_verified
        || report.provider_sync_attested
        || report.local_eviction_authorized
        || report.mutation_performed
        || !managed_database_roles_valid
        || !admission_state_valid
        || !admission_blockers_consistent
        || !blocker_codes_valid
    {
        return Err("icloud-sync-health-evidence-claim-invalid".into());
    }
    let managed_database_allocated_bytes = report
        .managed_database_files
        .iter()
        .try_fold(0_u64, |total, file| {
            total
                .checked_add(file.allocated_bytes)
                .ok_or_else(|| "icloud-sync-health-evidence-bytes-overflow".to_string())
        })?;
    if managed_database_allocated_bytes != report.managed_database_allocated_bytes {
        return Err("icloud-sync-health-evidence-bytes-mismatch".into());
    }
    if let Some(native_status) = report.native_status.as_ref() {
        validate_native_status_evidence(native_status)
            .map_err(|_| "icloud-sync-health-evidence-native-status-invalid".to_string())?;
        if native_status.observed_at_ms != report.observed_at_ms {
            return Err("icloud-sync-health-evidence-native-status-time-mismatch".into());
        }
    }
    if let Some(activity) = report.file_provider_activity.as_ref() {
        validate_file_provider_activity_evidence(activity)
            .map_err(|_| "icloud-sync-health-evidence-file-provider-activity-invalid".to_string())?;
        if activity.observed_at_ms != report.observed_at_ms {
            return Err("icloud-sync-health-evidence-file-provider-activity-time-mismatch".into());
        }
    }
    let mut snapshot = IcloudSyncHealthEvidenceSnapshot {
        schema_version: ICLOUD_SYNC_HEALTH_EVIDENCE_SCHEMA_VERSION,
        observed_at_ms: report.observed_at_ms,
        evidence_complete: report.evidence_complete,
        database_snapshot_includes_wal: report.database_snapshot_includes_wal,
        managed_database_allocated_bytes,
        upload_queue: report.upload_queue.clone(),
        sync_backlog_present: report.sync_backlog_present,
        new_copy_admission_state: report.new_copy_admission_state.clone(),
        new_copy_admission_blockers: report.new_copy_admission_blockers.clone(),
        blockers: report.blockers.clone(),
        native_status: report.native_status.clone(),
        file_provider_activity: report.file_provider_activity.clone(),
        evidence_fingerprint_sha256: String::new(),
    };
    snapshot.evidence_fingerprint_sha256 = health_evidence_fingerprint(&snapshot)?;
    Ok(snapshot)
}

pub fn validate_icloud_sync_health_evidence_snapshot(
    snapshot: &IcloudSyncHealthEvidenceSnapshot,
) -> Result<(), String> {
    if snapshot.schema_version != ICLOUD_SYNC_HEALTH_EVIDENCE_SCHEMA_VERSION
        || snapshot.observed_at_ms == 0
        || !matches!(snapshot.new_copy_admission_state.as_str(), "clear" | "blocked")
        || ((snapshot.new_copy_admission_state == "clear")
            != snapshot.new_copy_admission_blockers.is_empty())
        || snapshot
            .new_copy_admission_blockers
            .iter()
            .chain(snapshot.blockers.iter())
            .any(|code| !health_evidence_code_is_valid(code))
    {
        return Err("icloud-sync-health-evidence-shape-invalid".into());
    }
    if let Some(native_status) = snapshot.native_status.as_ref() {
        validate_native_status_evidence(native_status)
            .map_err(|_| "icloud-sync-health-evidence-native-status-invalid".to_string())?;
        if native_status.observed_at_ms != snapshot.observed_at_ms {
            return Err("icloud-sync-health-evidence-native-status-time-mismatch".into());
        }
    }
    if let Some(activity) = snapshot.file_provider_activity.as_ref() {
        validate_file_provider_activity_evidence(activity)
            .map_err(|_| "icloud-sync-health-evidence-file-provider-activity-invalid".to_string())?;
        if activity.observed_at_ms != snapshot.observed_at_ms {
            return Err("icloud-sync-health-evidence-file-provider-activity-time-mismatch".into());
        }
    }
    let expected = health_evidence_fingerprint(snapshot)?;
    if snapshot.evidence_fingerprint_sha256 != expected {
        return Err("icloud-sync-health-evidence-fingerprint-invalid".into());
    }
    Ok(())
}

/// Persist one bounded, path-free iCloud health observation for later incident comparison.
#[cfg(not(coverage))]
pub fn write_icloud_sync_health_evidence(
    app_data_dir: &Path,
    report: &IcloudSyncHealthReport,
) -> Result<PathBuf, String> {
    let snapshot = health_evidence_snapshot_from_report(report)?;
    validate_icloud_sync_health_evidence_snapshot(&snapshot)?;
    let directory = health_evidence_directory(app_data_dir)?;
    let path = directory.join(format!(
        "{:020}-{}.json",
        snapshot.observed_at_ms, snapshot.evidence_fingerprint_sha256
    ));
    let encoded = serde_json::to_vec_pretty(&snapshot)
        .map_err(|_| "icloud-sync-health-evidence-encode-failed".to_string())?;
    if encoded.len() > MAX_PERSISTED_HEALTH_SNAPSHOT_BYTES {
        return Err("icloud-sync-health-evidence-too-large".into());
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
        .map_err(|_| "icloud-sync-health-evidence-create-failed".to_string())?;
    let result = (|| -> Result<(), String> {
        file.write_all(&encoded)
            .and_then(|_| file.sync_all())
            .map_err(|_| "icloud-sync-health-evidence-write-failed".to_string())?;
        #[cfg(not(unix))]
        file.set_len(encoded.len() as u64)
            .map_err(|_| "icloud-sync-health-evidence-write-failed".to_string())?;
        #[cfg(unix)]
        std::fs::File::open(&directory)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| "icloud-sync-health-evidence-directory-sync-failed".to_string())?;
        Ok(())
    })();
    if let Err(error) = result {
        drop(file);
        let _ = std::fs::remove_file(&path);
        return Err(error);
    }
    prune_health_evidence(&directory)?;
    Ok(path)
}

#[cfg(not(coverage))]
fn health_evidence_directory(app_data_dir: &Path) -> Result<PathBuf, String> {
    if !app_data_dir.is_absolute()
        || app_data_dir
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("icloud-sync-health-evidence-parent-invalid".into());
    }
    std::fs::create_dir_all(app_data_dir)
        .map_err(|_| "icloud-sync-health-evidence-parent-create-failed".to_string())?;
    let parent = std::fs::symlink_metadata(app_data_dir)
        .map_err(|_| "icloud-sync-health-evidence-parent-unavailable".to_string())?;
    if parent.file_type().is_symlink() || !parent.is_dir() {
        return Err("icloud-sync-health-evidence-parent-unsafe".into());
    }
    let directory = app_data_dir.join(ICLOUD_SYNC_HEALTH_EVIDENCE_DIRECTORY);
    std::fs::create_dir_all(&directory)
        .map_err(|_| "icloud-sync-health-evidence-directory-create-failed".to_string())?;
    let metadata = std::fs::symlink_metadata(&directory)
        .map_err(|_| "icloud-sync-health-evidence-directory-unavailable".to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("icloud-sync-health-evidence-directory-unsafe".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))
            .map_err(|_| "icloud-sync-health-evidence-directory-permissions-failed".to_string())?;
    }
    Ok(directory)
}

#[cfg(not(coverage))]
fn is_health_evidence_record_name(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(".json") else {
        return false;
    };
    let Some((timestamp, fingerprint)) = stem.split_once('-') else {
        return false;
    };
    timestamp.len() == 20
        && timestamp.bytes().all(|byte| byte.is_ascii_digit())
        && fingerprint.len() == 64
        && fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(not(coverage))]
fn prune_health_evidence(directory: &Path) -> Result<(), String> {
    let mut records = std::fs::read_dir(directory)
        .map_err(|_| "icloud-sync-health-evidence-directory-read-failed".to_string())?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            is_health_evidence_record_name(&name).then_some((name, entry.path()))
        })
        .collect::<Vec<_>>();
    records.sort_by(|left, right| left.0.cmp(&right.0));
    while records.len() > MAX_PERSISTED_HEALTH_SNAPSHOTS {
        let (_, path) = records.remove(0);
        std::fs::remove_file(path)
            .map_err(|_| "icloud-sync-health-evidence-retention-failed")?;
    }
    Ok(())
}

fn system_time_ms(time: SystemTime) -> Option<u64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
}

fn allocated_bytes(metadata: &fs::Metadata) -> u64 {
    #[cfg(unix)]
    {
        metadata.blocks().saturating_mul(512)
    }
    #[cfg(not(unix))]
    {
        metadata.len()
    }
}

fn database_file_evidence(
    db_dir: &Path,
    role: &str,
    required: bool,
) -> Result<ManagedDatabaseFileEvidence, String> {
    let path = db_dir.join(role);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if !required && error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ManagedDatabaseFileEvidence {
                role: role.into(),
                present: false,
                logical_bytes: 0,
                allocated_bytes: 0,
                modified_ms: None,
            });
        }
        Err(_) => return Err(format!("icloud-sync-health-{role}-unavailable")),
    };
    if metadata.file_type().is_symlink() {
        return Err(format!("icloud-sync-health-{role}-symlink-rejected"));
    }
    if !metadata.is_file() {
        return Err(format!("icloud-sync-health-{role}-not-regular-file"));
    }
    Ok(ManagedDatabaseFileEvidence {
        role: role.into(),
        present: true,
        logical_bytes: metadata.len(),
        allocated_bytes: allocated_bytes(&metadata),
        modified_ms: metadata.modified().ok().and_then(system_time_ms),
    })
}

fn sqlite_uri(client_db: &Path) -> Result<String, String> {
    if !client_db.is_absolute() {
        return Err("icloud-sync-health-client-db-not-absolute".into());
    }
    let text = client_db
        .to_str()
        .ok_or_else(|| "icloud-sync-health-client-db-not-unicode".to_string())?;
    if text
        .chars()
        .any(|character| matches!(character, '?' | '#' | '\0'))
    {
        return Err("icloud-sync-health-client-db-uri-unsafe".into());
    }
    Ok(format!("file:{text}?immutable=1&mode=ro"))
}

fn temporary_snapshot_uri(client_db: &Path) -> Result<String, String> {
    if !client_db.is_absolute() {
        return Err("icloud-sync-health-client-db-not-absolute".into());
    }
    let text = client_db
        .to_str()
        .ok_or_else(|| "icloud-sync-health-client-db-not-unicode".to_string())?;
    if text
        .chars()
        .any(|character| matches!(character, '?' | '#' | '\0'))
    {
        return Err("icloud-sync-health-client-db-uri-unsafe".into());
    }
    Ok(format!("file:{text}?mode=rw"))
}

fn run_bounded_child(mut child: std::process::Child, timeout: Duration) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(_)) => return Err("icloud-sync-health-child-failed".into()),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("icloud-sync-health-child-timeout".into());
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("icloud-sync-health-child-wait-failed".into());
            }
        }
    }
}

fn run_queue_probe_with_uri(
    client_db_uri: String,
    source_immutable: bool,
) -> Result<String, String> {
    let sqlite_metadata = fs::symlink_metadata(SQLITE3_PATH)
        .map_err(|_| "icloud-sync-health-sqlite3-unavailable".to_string())?;
    if sqlite_metadata.file_type().is_symlink() || !sqlite_metadata.is_file() {
        return Err("icloud-sync-health-sqlite3-not-regular-file".into());
    }
    let mut command = Command::new(SQLITE3_PATH);
    if source_immutable {
        command.arg("-readonly");
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            command.pre_exec(|| {
                if libc::setpgid(0, 0) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
    let mut child = command
        .arg(client_db_uri)
        .arg(QUEUE_QUERY)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| "icloud-sync-health-sqlite3-spawn-failed".to_string())?;
    let child_pid = child.id();
    #[cfg(unix)]
    let kill_group = || unsafe {
        let _ = libc::kill(-(child_pid as libc::pid_t), libc::SIGKILL);
    };
    #[cfg(not(unix))]
    let kill_group = || {};
    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            kill_group();
            let _ = child.kill();
            let _ = child.wait();
            return Err("icloud-sync-health-query-stdout-unavailable".into());
        }
    };
    let stderr = match child.stderr.take() {
        Some(stderr) => stderr,
        None => {
            kill_group();
            let _ = child.kill();
            let _ = child.wait();
            return Err("icloud-sync-health-query-stderr-unavailable".into());
        }
    };
    let stdout_reader = thread::spawn(move || {
        let mut output = Vec::new();
        let read_ok = stdout
            .take((MAX_STDOUT_BYTES + 1) as u64)
            .read_to_end(&mut output)
            .is_ok();
        (read_ok, output)
    });
    let stderr_reader = thread::spawn(move || {
        let mut output = Vec::new();
        let read_ok = stderr
            .take((MAX_STDERR_BYTES + 1) as u64)
            .read_to_end(&mut output)
            .is_ok();
        (read_ok, output)
    });
    let deadline = Instant::now() + PROBE_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                kill_group();
                break;
            }
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            Ok(None) => {
                kill_group();
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err("icloud-sync-health-query-timeout".into());
            }
            Err(_) => {
                kill_group();
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err("icloud-sync-health-query-wait-failed".into());
            }
        }
    }
    let (stdout_ok, stdout) = stdout_reader
        .join()
        .map_err(|_| "icloud-sync-health-query-output-failed".to_string())?;
    let (stderr_ok, stderr) = stderr_reader
        .join()
        .map_err(|_| "icloud-sync-health-query-output-failed".to_string())?;
    if !stdout_ok || !stderr_ok {
        return Err("icloud-sync-health-query-output-failed".into());
    }
    if stdout.len() > MAX_STDOUT_BYTES || stderr.len() > MAX_STDERR_BYTES {
        return Err("icloud-sync-health-query-output-oversized".into());
    }
    let status = child
        .try_wait()
        .map_err(|_| "icloud-sync-health-query-wait-failed".to_string())?
        .ok_or_else(|| "icloud-sync-health-query-wait-failed".to_string())?;
    if !status.success() {
        return Err("icloud-sync-health-schema-unsupported".into());
    }
    if !stderr.is_empty() {
        return Err("icloud-sync-health-query-stderr-present".into());
    }
    String::from_utf8(stdout).map_err(|_| "icloud-sync-health-query-output-not-utf8".into())
}

fn run_queue_probe(client_db: &Path) -> Result<String, String> {
    run_queue_probe_with_uri(sqlite_uri(client_db)?, true)
}

fn bounded_native_status_token(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'|' | b'-' | b'_' | b'.'))
    {
        return None;
    }
    Some(value.to_string())
}

fn strip_ansi_sequences(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut in_escape = false;
    for character in value.chars() {
        if in_escape {
            if character.is_ascii_alphabetic() {
                in_escape = false;
            }
            continue;
        }
        if character == '\u{1b}' {
            in_escape = true;
        } else {
            output.push(character);
        }
    }
    output
}

fn parse_native_status_output(
    output: &str,
    observed_at_ms: u64,
    command_succeeded: bool,
    timed_out: bool,
    output_truncated: bool,
) -> IcloudNativeStatusEvidence {
    let output = strip_ansi_sequences(output);
    let container_count = output.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let count = parts.next()?.parse::<u64>().ok()?;
        (parts.next() == Some("containers") && parts.next() == Some("matching")).then_some(count)
    });
    let summary = output.lines().find(|line| {
        line.contains("{client:") && line.contains(" server:") && line.contains(" sync:")
    });
    let (client_state, server_state, sync_state, last_sync_present) = summary
        .and_then(|line| line.split_once("{client:").map(|(_, value)| value))
        .map(|value| {
            let (client, value) = value.split_once(" server:").unwrap_or((value, ""));
            let (server, value) = value.split_once(" sync:").unwrap_or((value, ""));
            let (sync, last_sync_present) = match value.split_once(" last-sync:") {
                Some((sync, _)) => (sync, true),
                None => (value.split_once('}').map_or(value, |(sync, _)| sync), false),
            };
            (
                bounded_native_status_token(client),
                bounded_native_status_token(server),
                bounded_native_status_token(sync),
                last_sync_present,
            )
        })
        .unwrap_or((None, None, None, false));
    let status_observed = client_state.is_some() || server_state.is_some() || sync_state.is_some();
    let evidence_complete = container_count.is_some()
        && client_state.is_some()
        && server_state.is_some()
        && sync_state.is_some();
    let mut notices = Vec::new();
    if status_observed {
        notices.push("icloud-native-status-summary-observed".into());
    } else {
        notices.push("icloud-native-status-summary-unavailable".into());
    }
    if timed_out {
        notices.push("icloud-native-status-command-timeout".into());
    }
    if output_truncated {
        notices.push("icloud-native-status-output-truncated".into());
    }
    if !command_succeeded && !timed_out {
        notices.push("icloud-native-status-command-failed".into());
    }
    if status_observed && !evidence_complete {
        notices.push("icloud-native-status-summary-incomplete".into());
    }
    IcloudNativeStatusEvidence {
        schema_version: ICLOUD_NATIVE_STATUS_SCHEMA_VERSION,
        observed_at_ms,
        command_succeeded,
        timed_out,
        output_truncated,
        status_observed,
        evidence_complete,
        container_count,
        client_state,
        server_state,
        sync_state,
        last_sync_present,
        notices,
    }
}

fn native_status_summary_complete(output: &[u8]) -> bool {
    let output = String::from_utf8_lossy(output);
    let container_count = output.lines().any(|line| {
        let mut parts = line.split_whitespace();
        parts.next().and_then(|value| value.parse::<u64>().ok()).is_some()
            && parts.next() == Some("containers")
            && parts.next() == Some("matching")
    });
    let summary = output.lines().any(|line| {
        line.contains("{client:")
            && line.contains(" server:")
            && line.contains(" sync:")
    });
    container_count && summary
}

/// Converts bounded `fileproviderctl` text into path-free activity evidence and notices.
///
/// Relative operation ages are considered stalled only when paired with a provider operation and
/// an error marker, so unrelated diagnostic durations cannot block a copy by themselves.
fn parse_file_provider_activity_output(
    output: &str,
    observed_at_ms: u64,
    command_succeeded: bool,
    timed_out: bool,
    output_truncated: bool,
) -> IcloudFileProviderActivityEvidence {
    let no_progress_fetch_count = output
        .lines()
        .filter(|line| {
            let line = line.to_ascii_lowercase();
            line.contains("fetchcontentsforitemwithid") && line.contains("no progress")
        })
        .count() as u64;
    let no_progress_create_count = output
        .lines()
        .filter(|line| {
            let line = line.to_ascii_lowercase();
            line.contains("createitembasedontemplate") && line.contains("no progress")
        })
        .count() as u64;
    let materialization_failure_count = output
        .lines()
        .filter(|line| line.to_ascii_lowercase().contains("materializationfailed"))
        .count() as u64;
    let staged_item_missing_count = output
        .lines()
        .filter(|line| line.to_ascii_lowercase().contains("stageditemmissing"))
        .count() as u64;
    let item_locked = output.lines().any(|line| {
        line.to_ascii_lowercase()
            .contains("itemisflockedcannotpropagate")
    });
    // fileproviderctl includes a relative age on queued operation errors. Treat an old fetch/create
    // error as a stalled provider signal even when the current sample has no explicit "no progress"
    // marker; this survives app restarts and matches the user-visible Finder "preparing" stall.
    let provider_lines = output.lines().collect::<Vec<_>>();
    let is_provider_operation = |line: &&str| {
        let lower = line.to_ascii_lowercase();
        lower.contains("fetch-content")
            || lower.contains("fetchcontentsforitemwithid")
            || lower.contains("create-item")
            || lower.contains("createitembasedontemplate")
    };
    let is_stale_age = |line: &&str| {
        relative_age_ms(line).is_some_and(|age| age >= FILE_PROVIDER_STALE_ERROR_AGE_MS)
    };
    let is_provider_error = |line: &&str| {
        let lower = line.to_ascii_lowercase();
        lower.contains("error:")
            || lower.contains("nocontenttofetch")
            || lower.contains("itemnotfound")
            || lower.contains("materializationfailed")
            || lower.contains("stageditemmissing")
            || lower.contains("itemisflockedcannotpropagate")
    };
    let is_provider_record_start = |line: &&str| {
        let lower = line.to_ascii_lowercase();
        lower.contains("docid(") || is_provider_operation(line)
    };
    let stale_error_observed = provider_lines.iter().any(|line| {
        is_provider_operation(line) && is_stale_age(line) && is_provider_error(line)
    }) || provider_lines.windows(2).any(|record| {
        is_provider_operation(&record[0])
            && !is_provider_record_start(&record[1])
            && is_stale_age(&record[1])
            && (is_provider_error(&record[0]) || is_provider_error(&record[1]))
    });
    let sync_excluded_filename_count = output
        .lines()
        .filter(|line| {
            line.to_ascii_lowercase()
                .contains("excluded from sync due to filename")
        })
        .count() as u64;
    let sync_excluded_root_count = output
        .lines()
        .filter(|line| {
            line.to_ascii_lowercase()
                .contains("excluded from sync under root")
        })
        .count() as u64;
    let active_upload_count = output
        .lines()
        .filter(|line| line.to_ascii_lowercase().contains("upload progress:"))
        .count() as u64;
    let active_download_count = output
        .lines()
        .filter(|line| line.to_ascii_lowercase().contains("download progress:"))
        .count() as u64;
    let active_upload_progress_millionths = progress_millionths(output, "upload progress:");
    let active_download_progress_millionths = progress_millionths(output, "download progress:");
    let mut notices = if command_succeeded {
        vec!["icloud-file-provider-dump-observed".into()]
    } else {
        vec!["icloud-file-provider-dump-unavailable".into()]
    };
    if timed_out {
        notices.push("icloud-file-provider-dump-timeout".into());
    }
    if output_truncated {
        notices.push("icloud-file-provider-dump-output-truncated".into());
    }
    if no_progress_fetch_count > 0 {
        notices.push("icloud-file-provider-no-progress-fetch-observed".into());
    }
    if no_progress_create_count > 0 {
        notices.push("icloud-file-provider-no-progress-create-observed".into());
    }
    if materialization_failure_count > 0 {
        notices.push("icloud-file-provider-materialization-failed-observed".into());
    }
    if staged_item_missing_count > 0 {
        notices.push("icloud-file-provider-staged-item-missing-observed".into());
    }
    if item_locked {
        notices.push("icloud-file-provider-item-locked-observed".into());
    }
    if stale_error_observed {
        notices.push("icloud-file-provider-stale-error-observed".into());
    }
    if sync_excluded_filename_count > 0 {
        notices.push("icloud-file-provider-sync-filename-excluded-observed".into());
    }
    if sync_excluded_root_count > 0 {
        notices.push("icloud-file-provider-sync-root-excluded-observed".into());
    }
    if active_upload_count > 0 {
        notices.push("icloud-file-provider-active-upload".into());
    }
    if active_download_count > 0 {
        notices.push("icloud-file-provider-active-download".into());
    }
    IcloudFileProviderActivityEvidence {
        schema_version: ICLOUD_FILE_PROVIDER_ACTIVITY_SCHEMA_VERSION,
        observed_at_ms,
        command_succeeded,
        timed_out,
        output_truncated,
        no_progress_fetch_count,
        no_progress_create_count,
        materialization_failure_count,
        staged_item_missing_count,
        sync_excluded_filename_count,
        sync_excluded_root_count,
        active_upload_count,
        active_download_count,
        active_upload_progress_millionths,
        active_download_progress_millionths,
        notices,
    }
}

/// Extracts the oldest bounded relative age from a provider `last:` or `expired:` field.
fn relative_age_ms(line: &str) -> Option<u64> {
    ["last:'", "expired:'"].iter().filter_map(|marker| {
        let value_start = line.rfind(marker)?.saturating_add(marker.len());
        let value_end = value_start.checked_add(line[value_start..].find('\'')?)?;
        let value = &line[value_start..value_end];
        let age_start = value.rfind("(-")?.saturating_add(2);
        let age_end = age_start.checked_add(value[age_start..].find(')')?)?;
        parse_age_components(&value[age_start..age_end])
    })
    .max()
}

/// Parses compact provider age components such as `4h9min` without floating-point rounding.
fn parse_age_components(age: &str) -> Option<u64> {
    let bytes = age.as_bytes();
    let mut index = 0;
    let mut total = 0_u64;
    let mut saw_component = false;
    while index < bytes.len() {
        let number_start = index;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        if number_start == index {
            return None;
        }
        let value = age[number_start..index].parse::<u64>().ok()?;
        let (unit_ms, width) = if bytes[index..].starts_with(b"min") {
            (60_000, 3)
        } else if bytes[index..].starts_with(b"ms") {
            (1, 2)
        } else if bytes[index..].starts_with(b"d") {
            (86_400_000, 1)
        } else if bytes[index..].starts_with(b"h") {
            (3_600_000, 1)
        } else if bytes[index..].starts_with(b"s") {
            (1_000, 1)
        } else {
            return None;
        };
        total = total.checked_add(value.checked_mul(unit_ms)?)?;
        index += width;
        saw_component = true;
    }
    saw_component.then_some(total)
}

/// Parses one provider progress fraction into millionths while rejecting malformed values.
fn progress_millionths(output: &str, operation: &str) -> Option<u32> {
    output.lines().find_map(|line| {
        if !line.to_ascii_lowercase().contains(operation) {
            return None;
        }
        let value = line.split_once("Fraction completed:")?.1.trim_start();
        let value = value.split_whitespace().next()?;
        let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
        let whole = whole.parse::<u32>().ok()?;
        if whole > 1 || fraction.bytes().any(|byte| !byte.is_ascii_digit()) {
            return None;
        }
        let mut scaled = fraction.bytes().take(6).fold(0_u32, |value, byte| {
            value.saturating_mul(10).saturating_add(u32::from(byte - b'0'))
        });
        for _ in fraction.len().min(6)..6 {
            scaled = scaled.saturating_mul(10);
        }
        let result = whole.saturating_mul(1_000_000).saturating_add(scaled);
        (result <= 1_000_000).then_some(result)
    })
}

#[cfg(target_os = "macos")]
fn probe_file_provider_activity(observed_at_ms: u64) -> IcloudFileProviderActivityEvidence {
    use std::os::unix::process::CommandExt;

    let mut command = Command::new(FILEPROVIDERCTL_PATH);
    command
        .args(["dump", "com.apple.CloudDocs.iCloudDriveFileProvider", "-l"])
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    unsafe {
        command.pre_exec(|| {
            if libc::setpgid(0, 0) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(_) => {
            return parse_file_provider_activity_output("", observed_at_ms, false, false, false)
        }
    };
    let child_pid = child.id();
    let kill_group = || unsafe {
        let _ = libc::kill(-(child_pid as libc::pid_t), libc::SIGKILL);
    };
    let mut stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            kill_group();
            let _ = child.kill();
            let _ = child.wait();
            return parse_file_provider_activity_output("", observed_at_ms, false, false, false);
        }
    };
    use std::io::ErrorKind;
    use std::os::fd::AsRawFd;
    let fd = stdout.as_raw_fd();
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        kill_group();
        let _ = child.kill();
        let _ = child.wait();
        return parse_file_provider_activity_output("", observed_at_ms, false, false, false);
    }
    let mut output = Vec::new();
    let mut read_failed = false;
    let mut timed_out = false;
    let mut status = None;
    let deadline = Instant::now() + FILEPROVIDER_DUMP_TIMEOUT;
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        match stdout.read(&mut buffer) {
            Ok(read) if read > 0 => {
                let remaining = MAX_FILEPROVIDER_DUMP_BYTES
                    .saturating_add(1)
                    .saturating_sub(output.len());
                output.extend_from_slice(&buffer[..read.min(remaining)]);
            }
            Ok(_) => {}
            Err(error) if error.kind() == ErrorKind::WouldBlock => {}
            Err(_) => {
                read_failed = true;
                break;
            }
        }
        match child.try_wait() {
            Ok(Some(child_status)) => {
                status = Some(child_status);
                let drain_deadline = Instant::now() + Duration::from_secs(1);
                loop {
                    match stdout.read(&mut buffer) {
                        Ok(0) => break,
                        Ok(read) => {
                            let remaining = MAX_FILEPROVIDER_DUMP_BYTES
                                .saturating_add(1)
                                .saturating_sub(output.len());
                            output.extend_from_slice(&buffer[..read.min(remaining)]);
                            if read > remaining {
                                kill_group();
                                break;
                            }
                        }
                        Err(error)
                            if matches!(
                                error.kind(),
                                ErrorKind::WouldBlock | ErrorKind::Interrupted
                            ) =>
                        {
                            if Instant::now() >= drain_deadline {
                                kill_group();
                                break;
                            }
                            thread::sleep(Duration::from_millis(5));
                        }
                        Err(_) => {
                            read_failed = true;
                            break;
                        }
                    }
                }
                break;
            }
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(25)),
            Ok(None) => {
                timed_out = true;
                // fileproviderctl flushes its sync-engine summary during a graceful shutdown;
                // retain that bounded output before falling back to a process-group kill.
                unsafe {
                    let _ = libc::kill(-(child_pid as libc::pid_t), libc::SIGTERM);
                }
                let graceful_deadline = Instant::now() + Duration::from_secs(2);
                while status.is_none() && Instant::now() < graceful_deadline {
                    match child.try_wait() {
                        Ok(Some(child_status)) => status = Some(child_status),
                        Ok(None) => thread::sleep(Duration::from_millis(25)),
                        Err(_) => break,
                    }
                }
                if status.is_none() {
                    kill_group();
                    let _ = child.kill();
                }
                let _ = child.wait();
                break;
            }
            Err(_) => {
                kill_group();
                let _ = child.kill();
                let _ = child.wait();
                read_failed = true;
                break;
            }
        }
    }
    if timed_out || read_failed {
        if read_failed || status.is_none() {
            kill_group();
            let _ = child.wait();
        }
        let drain_deadline = Instant::now() + Duration::from_secs(1);
        loop {
            match stdout.read(&mut buffer) {
                Ok(read) => {
                    if read == 0 {
                        break;
                    }
                    let remaining = MAX_FILEPROVIDER_DUMP_BYTES
                        .saturating_add(1)
                        .saturating_sub(output.len());
                    output.extend_from_slice(&buffer[..read.min(remaining)]);
                    if read > remaining {
                        kill_group();
                        break;
                    }
                }
                Err(error)
                    if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted) =>
                {
                    if Instant::now() >= drain_deadline {
                        kill_group();
                        break;
                    }
                    thread::sleep(Duration::from_millis(5));
                }
                Err(_) => break,
            }
        }
    }
    let output_truncated = output.len() > MAX_FILEPROVIDER_DUMP_BYTES;
    let output = String::from_utf8_lossy(&output[..output.len().min(MAX_FILEPROVIDER_DUMP_BYTES)]);
    parse_file_provider_activity_output(
        &output,
        observed_at_ms,
        !timed_out && !read_failed && status.is_some_and(|status| status.success()),
        timed_out,
        output_truncated,
    )
}

#[cfg(target_os = "macos")]
fn probe_native_status(observed_at_ms: u64) -> IcloudNativeStatusEvidence {
    use std::io::ErrorKind;
    use std::os::fd::AsRawFd;
    use std::os::unix::process::CommandExt;

    let mut command = Command::new(BRCTL_STATUS_PATH);
    command
        .arg("status")
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    // Keep descendants in a private process group so brctl helpers cannot retain our pipe after
    // the bounded probe expires.
    unsafe {
        command.pre_exec(|| {
            if libc::setpgid(0, 0) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(_) => return parse_native_status_output("", observed_at_ms, false, false, false),
    };
    let child_pid = child.id();
    let kill_group = || unsafe {
        let _ = libc::kill(-(child_pid as libc::pid_t), libc::SIGKILL);
    };
    let mut stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            kill_group();
            let _ = child.kill();
            let _ = child.wait();
            return parse_native_status_output("", observed_at_ms, false, false, false);
        }
    };
    let fd = stdout.as_raw_fd();
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        kill_group();
        let _ = child.kill();
        let _ = child.wait();
        return parse_native_status_output("", observed_at_ms, false, false, false);
    }
    let mut output = Vec::new();
    let mut buffer = [0u8; 4096];
    let deadline = Instant::now() + BRCTL_STATUS_TIMEOUT;
    let mut timed_out = false;
    let mut output_truncated = false;
    let mut read_failed = false;
    let mut bounded_after_summary = false;
    let status = loop {
        match stdout.read(&mut buffer) {
            Ok(0) => {}
            Ok(read) => {
                let remaining = MAX_BRCTL_STATUS_BYTES.saturating_sub(output.len());
                output.extend_from_slice(&buffer[..read.min(remaining)]);
                if read > remaining || output.len() >= MAX_BRCTL_STATUS_BYTES {
                    output_truncated = true;
                    kill_group();
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
            }
            Err(error)
                if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted) => {}
            Err(_) => {
                read_failed = true;
                kill_group();
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
        if native_status_summary_complete(&output) {
            bounded_after_summary = true;
            kill_group();
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                // The process can exit while unread bytes remain in the pipe. Drain them before
                // parsing; otherwise a complete native summary can be mistaken for missing data.
                let drain_deadline = Instant::now() + Duration::from_secs(1);
                loop {
                    match stdout.read(&mut buffer) {
                        Ok(0) => break,
                        Ok(read) => {
                            let remaining = MAX_BRCTL_STATUS_BYTES.saturating_sub(output.len());
                            output.extend_from_slice(&buffer[..read.min(remaining)]);
                            if read > remaining || output.len() >= MAX_BRCTL_STATUS_BYTES {
                                output_truncated = true;
                                kill_group();
                                break;
                            }
                        }
                        Err(error)
                            if matches!(
                                error.kind(),
                                ErrorKind::WouldBlock | ErrorKind::Interrupted
                            ) => {
                                if Instant::now() >= drain_deadline {
                                    kill_group();
                                    break;
                                }
                                thread::sleep(Duration::from_millis(5));
                            }
                        Err(_) => {
                            read_failed = true;
                            break;
                        }
                    }
                }
                break Some(status);
            }
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            Ok(None) => {
                timed_out = true;
                kill_group();
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            Err(_) => {
                kill_group();
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let output = String::from_utf8_lossy(&output[..output.len().min(MAX_BRCTL_STATUS_BYTES)]);
    parse_native_status_output(
        &output,
        observed_at_ms,
        !read_failed
            && (bounded_after_summary || status.is_some_and(|status| status.success())),
        timed_out,
        output_truncated,
    )
}

#[cfg(not(target_os = "macos"))]
fn probe_native_status(observed_at_ms: u64) -> IcloudNativeStatusEvidence {
    parse_native_status_output("", observed_at_ms, false, false, false)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceFileIdentity {
    logical_bytes: u64,
    modified_ms: Option<u64>,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
    #[cfg(unix)]
    modified_seconds: i64,
    #[cfg(unix)]
    modified_nanoseconds: i64,
}

fn source_file_identity(path: &Path, required: bool) -> Result<Option<SourceFileIdentity>, String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if !required && error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("icloud-sync-health-snapshot-source-unavailable".into()),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("icloud-sync-health-snapshot-source-unsafe".into());
    }
    Ok(Some(SourceFileIdentity {
        logical_bytes: metadata.len(),
        modified_ms: metadata.modified().ok().and_then(system_time_ms),
        #[cfg(unix)]
        device: metadata.dev(),
        #[cfg(unix)]
        inode: metadata.ino(),
        #[cfg(unix)]
        modified_seconds: metadata.mtime(),
        #[cfg(unix)]
        modified_nanoseconds: metadata.mtime_nsec(),
    }))
}

struct TemporarySnapshotDirectory {
    path: PathBuf,
}

impl Drop for TemporarySnapshotDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn create_temporary_snapshot_directory() -> Result<TemporarySnapshotDirectory, String> {
    let root = std::env::temp_dir();
    if !root.is_absolute() {
        return Err("icloud-sync-health-snapshot-temp-root-not-absolute".into());
    }
    let metadata = fs::symlink_metadata(&root)
        .map_err(|_| "icloud-sync-health-snapshot-temp-root-unavailable".to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("icloud-sync-health-snapshot-temp-root-unsafe".into());
    }
    for _ in 0..16 {
        let nonce = SNAPSHOT_NONCE.fetch_add(1, Ordering::Relaxed);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "icloud-sync-health-snapshot-clock-invalid".to_string())?
            .as_nanos();
        let path = root.join(format!(
            "disksage-icloud-health-{}-{now}-{nonce}",
            std::process::id()
        ));
        #[cfg(unix)]
        let created = {
            use std::os::unix::fs::DirBuilderExt;
            let mut builder = fs::DirBuilder::new();
            builder.mode(0o700);
            builder.create(&path)
        };
        #[cfg(not(unix))]
        let created = fs::create_dir(&path);
        match created {
            Ok(()) => return Ok(TemporarySnapshotDirectory { path }),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err("icloud-sync-health-snapshot-temp-create-failed".into()),
        }
    }
    Err("icloud-sync-health-snapshot-temp-collision".into())
}

#[cfg(target_os = "macos")]
fn clone_snapshot_file(source: &Path, destination: &Path) -> Result<(), String> {
    ensure_snapshot_file_within_limit(source)?;
    let cp_metadata = fs::symlink_metadata(CP_PATH)
        .map_err(|_| "icloud-sync-health-clone-command-unavailable".to_string())?;
    if cp_metadata.file_type().is_symlink() || !cp_metadata.is_file() {
        return Err("icloud-sync-health-clone-command-unsafe".into());
    }
    let child = Command::new(CP_PATH)
        .arg("-c")
        .arg(source)
        .arg(destination)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "icloud-sync-health-clone-command-spawn-failed".to_string())?;
    if run_bounded_child(child, SNAPSHOT_COPY_TIMEOUT).is_err() {
        let _ = fs::remove_file(destination);
        return Err("icloud-sync-health-clone-command-failed".into());
    }
    ensure_snapshot_file_with_cleanup(destination)
}

#[cfg(not(target_os = "macos"))]
fn clone_snapshot_file(source: &Path, destination: &Path) -> Result<(), String> {
    ensure_snapshot_file_within_limit(source)?;
    let mut input = fs::File::open(source)
        .map_err(|_| "icloud-sync-health-snapshot-copy-failed".to_string())?;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|_| "icloud-sync-health-snapshot-copy-failed".to_string())?;
    let mut copied = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let remaining = MAX_SNAPSHOT_SOURCE_BYTES.saturating_sub(copied);
        let read_limit = remaining.saturating_add(1).min(buffer.len() as u64) as usize;
        let read = input
            .read(&mut buffer[..read_limit])
            .map_err(|_| "icloud-sync-health-snapshot-copy-failed".to_string());
        let read = match read {
            Ok(read) => read,
            Err(error) => {
                let _ = fs::remove_file(destination);
                return Err(error);
            }
        };
        if read == 0 {
            if output.sync_all().is_err() {
                let _ = fs::remove_file(destination);
                return Err("icloud-sync-health-snapshot-copy-failed".into());
            }
            break;
        }
        if copied.saturating_add(read as u64) > MAX_SNAPSHOT_SOURCE_BYTES {
            let _ = fs::remove_file(destination);
            return Err("icloud-sync-health-snapshot-source-too-large".into());
        }
        if output.write_all(&buffer[..read]).is_err() {
            let _ = fs::remove_file(destination);
            return Err("icloud-sync-health-snapshot-copy-failed".into());
        }
        copied = copied.saturating_add(read as u64);
    }
    ensure_snapshot_file_with_cleanup(destination)
}

fn ensure_snapshot_file_within_limit(path: &Path) -> Result<(), String> {
    let identity = source_file_identity(path, true)?;
    if identity
        .as_ref()
        .is_some_and(|identity| identity.logical_bytes > MAX_SNAPSHOT_SOURCE_BYTES)
    {
        return Err("icloud-sync-health-snapshot-source-too-large".into());
    }
    Ok(())
}

fn ensure_snapshot_file_with_cleanup(path: &Path) -> Result<(), String> {
    match ensure_snapshot_file_within_limit(path) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = fs::remove_file(path);
            Err(error)
        }
    }
}

struct ClientDatabaseSnapshot {
    _directory: TemporarySnapshotDirectory,
    client_db: PathBuf,
    includes_wal: bool,
}

fn clone_client_database_snapshot(db_dir: &Path) -> Result<ClientDatabaseSnapshot, String> {
    let source_db = db_dir.join("client.db");
    let source_wal = db_dir.join("client.db-wal");
    for _ in 0..SNAPSHOT_ATTEMPTS {
        let before_db = source_file_identity(&source_db, true)?;
        if before_db
            .as_ref()
            .is_some_and(|identity| identity.logical_bytes > MAX_SNAPSHOT_SOURCE_BYTES)
        {
            return Err("icloud-sync-health-snapshot-source-too-large".into());
        }
        let before_wal = source_file_identity(&source_wal, false)?;
        if before_wal
            .as_ref()
            .is_some_and(|identity| identity.logical_bytes > MAX_SNAPSHOT_SOURCE_BYTES)
        {
            return Err("icloud-sync-health-snapshot-source-too-large".into());
        }
        let directory = create_temporary_snapshot_directory()?;
        let client_db = directory.path.join("client.db");
        clone_snapshot_file(&source_db, &client_db)?;
        if before_wal.is_some() {
            clone_snapshot_file(&source_wal, &directory.path.join("client.db-wal"))?;
        }
        let after_db = source_file_identity(&source_db, true)?;
        let after_wal = source_file_identity(&source_wal, false)?;
        if before_db == after_db && before_wal == after_wal {
            source_file_identity(&client_db, true)?;
            if before_wal.is_some() {
                source_file_identity(&directory.path.join("client.db-wal"), true)?;
            }
            return Ok(ClientDatabaseSnapshot {
                _directory: directory,
                client_db,
                includes_wal: before_wal.is_some(),
            });
        }
    }
    Err("icloud-sync-health-snapshot-source-unstable".into())
}

#[cfg(target_os = "macos")]
fn bounded_native_status(db_dir: &Path, observed_at_ms: u64) -> Option<IcloudNativeStatusEvidence> {
    let identity = source_file_identity(&db_dir.join("client.db"), true).ok().flatten()?;
    (identity.logical_bytes <= MAX_SNAPSHOT_SOURCE_BYTES)
        .then(|| probe_native_status(observed_at_ms))
}

fn run_consistent_snapshot_queue_probe(db_dir: &Path) -> Result<(String, bool), String> {
    let snapshot = clone_client_database_snapshot(db_dir)?;
    let includes_wal = snapshot.includes_wal;
    let output = run_queue_probe_with_uri(temporary_snapshot_uri(&snapshot.client_db)?, false)?;
    Ok((output, includes_wal))
}

fn parse_queue_rows(output: &str) -> Result<IcloudUploadQueueSummary, String> {
    let mut rows = BTreeMap::<String, (u64, u64)>::new();
    for line in output.lines().filter(|line| !line.trim().is_empty()) {
        let fields = line.split('|').collect::<Vec<_>>();
        if fields.len() != 3 {
            return Err("icloud-sync-health-query-row-invalid".into());
        }
        let label = fields[0].to_string();
        let count = fields[1]
            .parse::<u64>()
            .map_err(|_| "icloud-sync-health-query-count-invalid".to_string())?;
        let bytes = fields[2]
            .parse::<u64>()
            .map_err(|_| "icloud-sync-health-query-bytes-invalid".to_string())?;
        if rows.insert(label, (count, bytes)).is_some() {
            return Err("icloud-sync-health-query-row-duplicate".into());
        }
    }
    let take = |label: &str| {
        rows.get(label)
            .copied()
            .ok_or_else(|| "icloud-sync-health-query-row-missing".to_string())
    };
    let (scheduled_waiting_count, scheduled_waiting_bytes) = take("scheduled_waiting")?;
    let (scheduled_active_count, scheduled_active_bytes) = take("scheduled_active")?;
    let (blocked_on_sync_up_count, _) = take("blocked_sync_up")?;
    let (out_of_quota_count, out_of_quota_bytes) = take("out_of_quota")?;
    let (garbage_collection_count, _) = take("gc")?;
    let (other_state_count, _) = take("other_state")?;
    let (item_error_count, _) = take("item_error")?;
    let (item_error_octagon_not_signed_in_count, octagon_newest_ms) =
        take("item_error_octagon_not_signed_in")?;
    let (item_error_unclassified_count, unclassified_newest_ms) = take("item_error_unclassified")?;
    if rows.len() != 9 {
        return Err("icloud-sync-health-query-row-unexpected".into());
    }
    let classified_item_error_count = item_error_octagon_not_signed_in_count
        .checked_add(item_error_unclassified_count)
        .ok_or_else(|| "icloud-sync-health-item-error-count-overflow".to_string())?;
    if classified_item_error_count != item_error_count {
        return Err("icloud-sync-health-item-error-classification-mismatch".into());
    }
    let newest_item_error_timestamp_ms = octagon_newest_ms.max(unclassified_newest_ms);
    let newest_item_error_timestamp_ms =
        (newest_item_error_timestamp_ms > 0).then_some(newest_item_error_timestamp_ms);
    let scheduled_count = scheduled_waiting_count
        .checked_add(scheduled_active_count)
        .ok_or_else(|| "icloud-sync-health-scheduled-count-overflow".to_string())?;
    let scheduled_bytes = scheduled_waiting_bytes
        .checked_add(scheduled_active_bytes)
        .ok_or_else(|| "icloud-sync-health-scheduled-bytes-overflow".to_string())?;
    Ok(IcloudUploadQueueSummary {
        scheduled_waiting_count,
        scheduled_waiting_bytes,
        scheduled_active_count,
        scheduled_active_bytes,
        blocked_on_sync_up_count,
        out_of_quota_count,
        out_of_quota_bytes,
        garbage_collection_count,
        other_state_count,
        item_error_count,
        item_error_octagon_not_signed_in_count,
        item_error_unclassified_count,
        newest_item_error_timestamp_ms,
        scheduled_count,
        scheduled_bytes,
    })
}

fn build_report(
    observed_at_ms: u64,
    managed_database_files: Vec<ManagedDatabaseFileEvidence>,
    upload_queue: IcloudUploadQueueSummary,
    evidence_complete: bool,
    database_snapshot_includes_wal: bool,
) -> Result<IcloudSyncHealthReport, String> {
    let managed_database_allocated_bytes =
        managed_database_files
            .iter()
            .try_fold(0u64, |total, file| {
                total
                    .checked_add(file.allocated_bytes)
                    .ok_or_else(|| "icloud-sync-health-database-bytes-overflow".to_string())
            })?;
    let sync_backlog_present = upload_queue.scheduled_count > 0
        || upload_queue.blocked_on_sync_up_count > 0
        || upload_queue.out_of_quota_count > 0
        || upload_queue.other_state_count > 0
        || upload_queue.item_error_count > 0;
    let mut new_copy_admission_blockers = Vec::new();
    if !evidence_complete {
        new_copy_admission_blockers.push("icloud-sync-health-evidence-incomplete".into());
    }
    if upload_queue.scheduled_waiting_count > 0 {
        new_copy_admission_blockers.push("icloud-upload-queue-nonempty".into());
    }
    if upload_queue.scheduled_active_count > 0 {
        new_copy_admission_blockers.push("icloud-upload-in-flight".into());
    }
    if upload_queue.blocked_on_sync_up_count > 0 {
        new_copy_admission_blockers.push("icloud-upload-blocked-on-sync-up".into());
    }
    if upload_queue.out_of_quota_count > 0 {
        new_copy_admission_blockers.push("icloud-upload-out-of-quota".into());
    }
    if upload_queue.other_state_count > 0 {
        new_copy_admission_blockers.push("icloud-upload-queue-state-unclassified".into());
    }
    if upload_queue.item_error_count > 0 {
        new_copy_admission_blockers.push("icloud-local-sync-item-error-present".into());
    }
    let new_copy_admission_state = if new_copy_admission_blockers.is_empty() {
        "clear"
    } else {
        "blocked"
    };
    let mut blockers = new_copy_admission_blockers.clone();
    blockers.push("provider-native-per-item-sync-attestation-required-before-eviction".into());
    let mut notices = if evidence_complete {
        vec![
            "read-only-source-copy-on-write-snapshot".into(),
            "source-snapshot-identities-stable".into(),
            if database_snapshot_includes_wal {
                "source-sqlite-wal-included"
            } else {
                "source-sqlite-wal-absent"
            }
            .into(),
            "source-database-sidecar-write-prohibited".into(),
            "temporary-snapshot-sidecar-write-permitted".into(),
            "temporary-snapshot-removed-after-query".into(),
        ]
    } else {
        vec![
            "read-only-immutable-main-database-snapshot".into(),
            "sqlite-wal-not-applied-to-avoid-sidecar-writes".into(),
            "snapshot-may-lag-active-cloud-docs-state".into(),
        ]
    };
    notices.extend([
        "cloud-docs-private-schema-is-supplementary-evidence".into(),
        "queue-bytes-are-transfer-size-not-remote-capacity".into(),
        "global-queue-state-is-not-per-item-upload-attestation".into(),
        "paths-and-filenames-redacted".into(),
        "no-cloud-write".into(),
        "no-local-eviction".into(),
    ]);
    if upload_queue.item_error_octagon_not_signed_in_count > 0 {
        notices.push("icloud-item-error-octagon-not-signed-in".into());
    }
    if upload_queue.item_error_unclassified_count > 0 {
        notices.push("icloud-item-error-unclassified".into());
    }
    if upload_queue
        .newest_item_error_timestamp_ms
        .and_then(|timestamp| observed_at_ms.checked_sub(timestamp))
        .is_some_and(|age_ms| age_ms >= ITEM_ERROR_AGE_NOTICE_MS)
    {
        notices.push("icloud-item-error-older-than-24h".into());
    }
    Ok(IcloudSyncHealthReport {
        schema_version: ICLOUD_SYNC_HEALTH_SCHEMA_VERSION,
        output_mode: "icloud-local-sync-health".into(),
        observed_at_ms,
        provider: "icloud".into(),
        evidence_kind: "supplementary-local-cloud-docs-private-schema".into(),
        evidence_complete,
        database_snapshot_includes_wal,
        database_sidecar_write_permitted: false,
        managed_database_files,
        managed_database_allocated_bytes,
        upload_queue,
        native_status: None,
        file_provider_activity: None,
        sync_backlog_present,
        new_copy_admission_state: new_copy_admission_state.into(),
        new_copy_admission_blockers,
        blockers,
        notices,
        paths_redacted: true,
        user_filenames_read: false,
        user_file_contents_read: false,
        remote_capacity_verified: false,
        provider_sync_attested: false,
        local_eviction_authorized: false,
        mutation_performed: false,
    })
}

/// Projects native iCloud observations into fail-closed copy-admission blockers.
fn attach_native_status_admission(report: &mut IcloudSyncHealthReport) {
    if report
        .native_status
        .as_ref()
        .is_some_and(|status| !status.evidence_complete)
        && !report
            .new_copy_admission_blockers
            .iter()
            .any(|blocker| blocker == "icloud-native-status-evidence-incomplete")
    {
        report
            .new_copy_admission_blockers
            .push("icloud-native-status-evidence-incomplete".into());
        report.new_copy_admission_state = "blocked".into();
    }
    if report
        .native_status
        .as_ref()
        .is_some_and(|status| status.timed_out)
        && !report
            .new_copy_admission_blockers
            .iter()
            .any(|blocker| blocker == "icloud-native-status-command-timeout")
    {
        report.sync_backlog_present = true;
        report
            .new_copy_admission_blockers
            .push("icloud-native-status-command-timeout".into());
        report.new_copy_admission_state = "blocked".into();
        report
            .blockers
            .insert(0, "icloud-native-status-command-timeout".into());
    }
    if report
        .native_status
        .as_ref()
        .is_some_and(native_sync_up_pending)
        && !report
            .new_copy_admission_blockers
            .iter()
            .any(|blocker| blocker == "icloud-native-sync-up-pending")
    {
        report.sync_backlog_present = true;
        report
            .new_copy_admission_blockers
            .push("icloud-native-sync-up-pending".into());
        report.new_copy_admission_state = "blocked".into();
        report
            .blockers
            .insert(0, "icloud-native-sync-up-pending".into());
    }
    if report
        .native_status
        .as_ref()
        .is_some_and(native_sync_down_pending)
        && !report
            .new_copy_admission_blockers
            .iter()
            .any(|blocker| blocker == "icloud-native-sync-down-pending")
    {
        report.sync_backlog_present = true;
        report
            .new_copy_admission_blockers
            .push("icloud-native-sync-down-pending".into());
        report.new_copy_admission_state = "blocked".into();
        report
            .blockers
            .insert(0, "icloud-native-sync-down-pending".into());
    }
    if let Some(activity) = report.file_provider_activity.as_ref() {
        let no_progress = activity.no_progress_fetch_count > 0
            || activity.no_progress_create_count > 0;
        let materialization_failed = activity.materialization_failure_count > 0
            || activity.staged_item_missing_count > 0;
        let mut add_blocker = |blocker: &str| {
            if !report
                .new_copy_admission_blockers
                .iter()
                .any(|existing| existing == blocker)
            {
                report.new_copy_admission_blockers.push(blocker.into());
                report.new_copy_admission_state = "blocked".into();
                report.sync_backlog_present = true;
                report.blockers.insert(0, blocker.into());
            }
        };
        if no_progress {
            add_blocker("icloud-file-provider-no-progress");
        }
        if materialization_failed {
            add_blocker("icloud-file-provider-materialization-failed");
        }
        if activity
            .notices
            .iter()
            .any(|notice| notice == "icloud-file-provider-item-locked-observed")
        {
            add_blocker("icloud-file-provider-item-locked");
        }
        if activity
            .notices
            .iter()
            .any(|notice| notice == "icloud-file-provider-stale-error-observed")
        {
            add_blocker("icloud-file-provider-stalled");
        }
        if activity.sync_excluded_filename_count > 0 {
            add_blocker("icloud-file-provider-filename-excluded");
        }
        if activity.sync_excluded_root_count > 0 {
            add_blocker("icloud-file-provider-root-excluded");
        }
        if !no_progress && !materialization_failed {
            if activity.active_upload_count > 0 || activity.active_download_count > 0 {
                add_blocker("icloud-file-provider-transfer-active");
            } else if activity.timed_out {
                add_blocker("icloud-file-provider-dump-timeout");
            } else if activity.output_truncated {
                add_blocker("icloud-file-provider-dump-output-truncated");
            } else if !activity.command_succeeded {
                add_blocker("icloud-file-provider-evidence-unavailable");
            }
        }
    }
}

/// Require a quiet local iCloud upload queue before adding another local copy.
///
/// This gate does not claim remote capacity or synchronization. The source remains retained and a
/// copied item still needs provider-native per-item evidence before any later local eviction.
pub fn require_new_copy_admission(report: &IcloudSyncHealthReport) -> Result<(), String> {
    if !report.evidence_complete {
        Err("icloud-sync-health-evidence-incomplete".into())
    } else if report.new_copy_admission_state == "clear"
        && report.new_copy_admission_blockers.is_empty()
    {
        Ok(())
    } else if report.new_copy_admission_blockers.is_empty() {
        Err("icloud-new-copy-admission-invalid".into())
    } else {
        Err(report.new_copy_admission_blockers.join(","))
    }
}

pub fn attach_new_copy_admission_notice(
    notices: &mut Vec<String>,
    report: Option<&IcloudSyncHealthReport>,
) {
    notices.retain(|notice| {
        !matches!(
            notice.as_str(),
            "icloud-new-copy-admission-clear"
                | "icloud-new-copy-admission-blocked"
                | "icloud-new-copy-admission-evidence-unavailable"
        )
    });
    notices.push(
        match report {
            Some(report)
                if report.evidence_complete
                    && report.new_copy_admission_state == "clear"
                    && report.new_copy_admission_blockers.is_empty() =>
            {
                "icloud-new-copy-admission-clear"
            }
            Some(_) => "icloud-new-copy-admission-blocked",
            None => "icloud-new-copy-admission-evidence-unavailable",
        }
        .into(),
    );
}

pub fn inspect_new_copy_admission(
    home: &Path,
    observed_at_ms: u64,
) -> Result<IcloudSyncHealthReport, String> {
    probe_icloud_sync_health(&default_cloud_docs_db_dir(home), observed_at_ms)
}

pub fn probe_icloud_sync_health(
    db_dir: &Path,
    observed_at_ms: u64,
) -> Result<IcloudSyncHealthReport, String> {
    if !db_dir.is_absolute() {
        return Err("icloud-sync-health-db-dir-not-absolute".into());
    }
    let dir_metadata = fs::symlink_metadata(db_dir)
        .map_err(|_| "icloud-sync-health-db-dir-unavailable".to_string())?;
    if dir_metadata.file_type().is_symlink() || !dir_metadata.is_dir() {
        return Err("icloud-sync-health-db-dir-unsafe".into());
    }
    let roles = [
        ("client.db", true),
        ("client.db-shm", false),
        ("client.db-wal", false),
        ("server.db", false),
        ("server.db-shm", false),
        ("server.db-wal", false),
    ];
    let managed_database_files = roles
        .iter()
        .map(|(role, required)| database_file_evidence(db_dir, role, *required))
        .collect::<Result<Vec<_>, _>>()?;
    #[cfg(target_os = "macos")]
    let native_status = bounded_native_status(db_dir, observed_at_ms);
    #[cfg(not(target_os = "macos"))]
    let native_status = Some(probe_native_status(observed_at_ms));
    let source_database_too_large = managed_database_files
        .iter()
        .any(|file| file.role == "client.db" && file.logical_bytes > MAX_SNAPSHOT_SOURCE_BYTES);
    match run_consistent_snapshot_queue_probe(db_dir) {
        Ok((output, includes_wal)) => {
            let mut report = build_report(
                observed_at_ms,
                managed_database_files,
                parse_queue_rows(&output)?,
                true,
                includes_wal,
            )?;
            report.native_status = native_status;
            #[cfg(target_os = "macos")]
            {
                report.file_provider_activity = Some(probe_file_provider_activity(observed_at_ms));
            }
            attach_native_status_admission(&mut report);
            Ok(report)
        }
        Err(_) if source_database_too_large => {
            let mut report = build_report(
                observed_at_ms,
                managed_database_files,
                IcloudUploadQueueSummary::default(),
                false,
                false,
            )?;
            report
                .notices
                .push("icloud-sync-health-source-database-too-large".into());
            report.native_status = native_status;
            #[cfg(target_os = "macos")]
            {
                report.file_provider_activity = Some(probe_file_provider_activity(observed_at_ms));
            }
            attach_native_status_admission(&mut report);
            Ok(report)
        }
        Err(_) => {
            let client_db = db_dir.join("client.db");
            let upload_queue = parse_queue_rows(&run_queue_probe(&client_db)?)?;
            let mut report = build_report(
                observed_at_ms,
                managed_database_files,
                upload_queue,
                false,
                false,
            )?;
            report
                .notices
                .push("consistent-copy-on-write-snapshot-unavailable".into());
            report.native_status = native_status;
            #[cfg(target_os = "macos")]
            {
                report.file_provider_activity = Some(probe_file_provider_activity(observed_at_ms));
            }
            attach_native_status_admission(&mut report);
            Ok(report)
        }
    }
}

pub fn default_cloud_docs_db_dir(home: &Path) -> PathBuf {
    home.join("Library")
        .join("Application Support")
        .join("CloudDocs")
        .join("session")
        .join("db")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn queue_output() -> &'static str {
        "scheduled_waiting|690905|65166294557\n\
         scheduled_active|372|329502\n\
         blocked_sync_up|240449|0\n\
         out_of_quota|0|0\n\
         gc|18595|0\n\
         other_state|1|0\n\
         item_error|1|0\n\
         item_error_octagon_not_signed_in|1|1000\n\
         item_error_unclassified|0|0\n"
    }

    #[test]
    fn parses_queue_rows_and_uses_checked_pending_arithmetic() {
        let queue = parse_queue_rows(queue_output()).unwrap();
        assert_eq!(queue.scheduled_count, 691_277);
        assert_eq!(queue.scheduled_bytes, 65_166_624_059);
        assert_eq!(queue.blocked_on_sync_up_count, 240_449);
        assert_eq!(queue.out_of_quota_count, 0);
        assert_eq!(queue.garbage_collection_count, 18_595);
        assert_eq!(queue.other_state_count, 1);
        assert_eq!(queue.item_error_count, 1);
        assert_eq!(queue.item_error_octagon_not_signed_in_count, 1);
        assert_eq!(queue.item_error_unclassified_count, 0);
        assert_eq!(queue.newest_item_error_timestamp_ms, Some(1000));
    }

    #[test]
    fn parses_bounded_brctl_summary_without_retaining_paths_or_item_ids() {
        let evidence = parse_native_status_output(
            "1 containers matching '*'\n\
             c{1}m.a{3}e.C{7}s[1] foreground {client:needs-sync server:full-sync|fetched-recents|ever-full-sync sync:needs-sync-up|in-sync-up|has-synced-down|0x100 last-sync:2026-08-14 01:57:54 +0000 requestID:7}\n",
            42,
            false,
            true,
            true,
        );
        assert!(evidence.status_observed);
        assert!(evidence.evidence_complete);
        assert_eq!(evidence.container_count, Some(1));
        assert_eq!(evidence.client_state.as_deref(), Some("needs-sync"));
        assert_eq!(
            evidence.server_state.as_deref(),
            Some("full-sync|fetched-recents|ever-full-sync")
        );
        assert_eq!(
            evidence.sync_state.as_deref(),
            Some("needs-sync-up|in-sync-up|has-synced-down|0x100")
        );
        assert!(evidence.last_sync_present);
        assert!(evidence.timed_out);
        assert!(evidence.output_truncated);
        assert!(!serde_json::to_string(&evidence)
            .unwrap()
            .contains("requestID"));
    }

    #[test]
    fn parses_brctl_summary_when_last_sync_is_absent() {
        let evidence = parse_native_status_output(
            "1 containers matching '*'\n\
             foreground {client:needs-sync server:full-sync sync:needs-sync-up}\n",
            42,
            true,
            false,
            false,
        );
        assert!(evidence.status_observed);
        assert!(evidence.evidence_complete);
        assert_eq!(evidence.container_count, Some(1));
        assert_eq!(evidence.sync_state.as_deref(), Some("needs-sync-up"));
        assert!(!evidence.last_sync_present);
        assert!(native_sync_up_pending(&evidence));
    }

    #[test]
    fn native_sync_down_pending_is_detected_from_bounded_summary() {
        let evidence = parse_native_status_output(
            "1 containers matching '*'\n\
             foreground {client:needs-sync server:full-sync sync:needs-sync-down last-sync:now}\n",
            42,
            false,
            true,
            false,
        );
        assert!(native_sync_down_pending(&evidence));
        assert!(!native_sync_up_pending(&evidence));
    }

    #[test]
    fn stops_native_probe_after_summary_before_detail_stream() {
        let summary = b"1 containers matching '*'\\nforeground {client:needs-sync server:full-sync sync:needs-sync-up last-sync:now}\\n";
        assert!(native_status_summary_complete(summary));
        assert!(!native_status_summary_complete(b"1 containers matching '*'\\n"));
    }

    #[test]
    fn native_status_parser_fails_closed_without_a_summary() {
        let evidence =
            parse_native_status_output("unexpected /Users/private/path\n", 42, false, false, false);
        assert!(!evidence.status_observed);
        assert!(!evidence.evidence_complete);
        assert!(evidence.container_count.is_none());
        assert!(evidence.client_state.is_none());
        assert!(evidence
            .notices
            .contains(&"icloud-native-status-summary-unavailable".into()));
        assert!(!serde_json::to_string(&evidence)
            .unwrap()
            .contains("/Users/"));
    }

    #[test]
    fn file_provider_parser_counts_redacted_no_progress_fetches() {
        let evidence = parse_file_provider_activity_output(
            "fetchContentsForItemWithID: (no timeout), no progress\nfetchContentsForItemWithID: (no timeout), no progress\n",
            42,
            false,
            true,
            false,
        );
        assert_eq!(evidence.no_progress_fetch_count, 2);
        assert_eq!(evidence.no_progress_create_count, 0);
        assert!(evidence.timed_out);
        assert!(evidence
            .notices
            .contains(&"icloud-file-provider-no-progress-fetch-observed".to_string()));
        assert!(validate_file_provider_activity_evidence(&evidence).is_ok());
    }

    #[test]
    fn file_provider_parser_counts_redacted_no_progress_creates() {
        let evidence = parse_file_provider_activity_output(
            "createItemBasedOnTemplate: (no timeout), no progress\n",
            42,
            true,
            false,
            false,
        );
        assert_eq!(evidence.no_progress_fetch_count, 0);
        assert_eq!(evidence.no_progress_create_count, 1);
        assert!(evidence
            .notices
            .contains(&"icloud-file-provider-no-progress-create-observed".to_string()));
        assert!(validate_file_provider_activity_evidence(&evidence).is_ok());
    }

    #[test]
    fn file_provider_parser_records_active_transfer_progress() {
        let evidence = parse_file_provider_activity_output(
            "sync engine state:\n\
             + upload progress: <progress> Fraction completed: 0.9524\n\
             + download progress: <progress> Fraction completed: 0.0000\n",
            42,
            true,
            false,
            false,
        );
        assert_eq!(evidence.active_upload_count, 1);
        assert_eq!(evidence.active_download_count, 1);
        assert_eq!(evidence.active_upload_progress_millionths, Some(952_400));
        assert_eq!(evidence.active_download_progress_millionths, Some(0));
        assert!(evidence
            .notices
            .contains(&"icloud-file-provider-active-upload".to_string()));
        assert!(evidence
            .notices
            .contains(&"icloud-file-provider-active-download".to_string()));
        assert!(validate_file_provider_activity_evidence(&evidence).is_ok());
    }

    #[test]
    fn file_provider_parser_records_materialization_failures_without_paths() {
        let evidence = parse_file_provider_activity_output(
            "itemMaterializationFailed(... stagedItemMissing ...)\nmaterializationFailed stagedItemMissing\n",
            42,
            true,
            false,
            false,
        );
        assert_eq!(evidence.materialization_failure_count, 2);
        assert_eq!(evidence.staged_item_missing_count, 2);
        assert!(evidence
            .notices
            .contains(&"icloud-file-provider-materialization-failed-observed".to_string()));
        assert!(evidence
            .notices
            .contains(&"icloud-file-provider-staged-item-missing-observed".to_string()));
        assert!(validate_file_provider_activity_evidence(&evidence).is_ok());
        assert!(!serde_json::to_string(&evidence).unwrap().contains("stagedItemMissing"));
    }

    #[test]
    fn file_provider_parser_records_locked_item_without_provider_identifiers() {
        let evidence = parse_file_provider_activity_output(
            "fetch-content: itemIsFlockedCanNotPropagate\n",
            42,
            true,
            false,
            false,
        );
        assert!(evidence
            .notices
            .contains(&"icloud-file-provider-item-locked-observed".to_string()));
        assert!(validate_file_provider_activity_evidence(&evidence).is_ok());
        assert!(!serde_json::to_string(&evidence)
            .unwrap()
            .contains("itemIsFlockedCanNotPropagate"));
    }

    #[test]
    fn file_provider_parser_detects_old_fetch_create_errors_as_stalled() {
        let evidence = parse_file_provider_activity_output(
            "doc fetch-content: last:'1787622820 (-4h9min)' error:'noContentToFetch'\n\
             doc create-item: last:'1787635515 (-37min30s)' error:'itemNotFound'\n",
            42,
            true,
            false,
            false,
        );
        assert!(evidence
            .notices
            .contains(&"icloud-file-provider-stale-error-observed".to_string()));
        let mut report = build_report(1, vec![], IcloudUploadQueueSummary::default(), true, true)
            .unwrap();
        report.file_provider_activity = Some(evidence);
        attach_native_status_admission(&mut report);
        assert!(report
            .new_copy_admission_blockers
            .contains(&"icloud-file-provider-stalled".to_string()));
    }

    #[test]
    fn file_provider_parser_detects_stale_error_age_on_adjacent_dump_row() {
        let evidence = parse_file_provider_activity_output(
            "doc fetch-content: error:'noContentToFetch'\n\
             last:'1787622820 (-4h9min)' expired:'1787622820 (-4h9min)'\n",
            42,
            true,
            false,
            false,
        );
        assert!(evidence
            .notices
            .contains(&"icloud-file-provider-stale-error-observed".to_string()));
    }

    #[test]
    fn file_provider_parser_ignores_unrelated_negative_duration() {
        let evidence = parse_file_provider_activity_output(
            "doc fetch-content: retry budget (-4h9min)\n",
            42,
            true,
            false,
            false,
        );
        assert!(!evidence
            .notices
            .contains(&"icloud-file-provider-stale-error-observed".to_string()));
    }

    #[test]
    fn file_provider_parser_ignores_adjacent_operation_age_from_another_record() {
        let evidence = parse_file_provider_activity_output(
            "i:docID(1) fetch-content: request\n\
             i:docID(2) last:'1787622820 (-4h9min)'\n",
            42,
            true,
            false,
            false,
        );
        assert!(!evidence
            .notices
            .contains(&"icloud-file-provider-stale-error-observed".to_string()));
    }

    #[test]
    fn file_provider_parser_ignores_old_healthy_operation_timestamp() {
        let evidence = parse_file_provider_activity_output(
            "i:docID(1) fetch-content: last:'1787622820 (-4h9min)' state:complete\n",
            42,
            true,
            false,
            false,
        );
        assert!(!evidence
            .notices
            .contains(&"icloud-file-provider-stale-error-observed".to_string()));
    }

    #[test]
    fn file_provider_parser_uses_expired_age_when_last_is_fresh() {
        let evidence = parse_file_provider_activity_output(
            "doc fetch-content: last:'1787622820 (-1min)' expired:'1787622820 (-16min)' error:'noContentToFetch'\n",
            42,
            true,
            false,
            false,
        );
        assert!(evidence
            .notices
            .contains(&"icloud-file-provider-stale-error-observed".to_string()));
    }

    #[test]
    fn locked_file_provider_item_blocks_new_copy_admission() {
        let mut report = build_report(1, vec![], IcloudUploadQueueSummary::default(), true, true)
            .unwrap();
        report.file_provider_activity = Some(parse_file_provider_activity_output(
            "fetch-content: itemIsFlockedCanNotPropagate\n",
            1,
            true,
            false,
            false,
        ));
        attach_native_status_admission(&mut report);
        assert!(report
            .new_copy_admission_blockers
            .contains(&"icloud-file-provider-item-locked".to_string()));
        assert!(report.sync_backlog_present);
        assert_eq!(report.new_copy_admission_state, "blocked");
    }

    #[test]
    fn file_provider_parser_records_sync_exclusions_without_paths() {
        let evidence = parse_file_provider_activity_output(
            "error: Excluded From Sync Due To Filename\nerror: Excluded From Sync Under Root\n",
            42,
            true,
            false,
            false,
        );
        assert_eq!(evidence.sync_excluded_filename_count, 1);
        assert_eq!(evidence.sync_excluded_root_count, 1);
        assert!(evidence
            .notices
            .contains(&"icloud-file-provider-sync-filename-excluded-observed".to_string()));
        assert!(evidence
            .notices
            .contains(&"icloud-file-provider-sync-root-excluded-observed".to_string()));
        assert!(validate_file_provider_activity_evidence(&evidence).is_ok());
        assert!(!serde_json::to_string(&evidence)
            .unwrap()
            .contains("Excluded From Sync"));
    }

    #[test]
    fn parser_rejects_missing_duplicate_and_unexpected_rows() {
        assert!(parse_queue_rows("scheduled_waiting|1|2\n").is_err());
        assert!(parse_queue_rows(&format!("{}scheduled_waiting|1|2\n", queue_output())).is_err());
        assert!(parse_queue_rows(&format!("{}other|1|2\n", queue_output())).is_err());
        assert!(parse_queue_rows(&queue_output().replace(
            "item_error_octagon_not_signed_in|1|1000",
            "item_error_octagon_not_signed_in|0|0"
        ))
        .is_err());
    }

    #[test]
    fn report_is_fail_closed_and_path_free() {
        let files = vec![ManagedDatabaseFileEvidence {
            role: "client.db".into(),
            present: true,
            logical_bytes: 100,
            allocated_bytes: 128,
            modified_ms: Some(1),
        }];
        let report = build_report(
            ITEM_ERROR_AGE_NOTICE_MS + 1000,
            files,
            parse_queue_rows(queue_output()).unwrap(),
            false,
            false,
        )
        .unwrap();
        assert!(report.sync_backlog_present);
        assert_eq!(report.new_copy_admission_state, "blocked");
        assert_eq!(report.managed_database_allocated_bytes, 128);
        assert!(!report.provider_sync_attested);
        assert!(!report.local_eviction_authorized);
        assert!(!report.mutation_performed);
        assert!(!report.evidence_complete);
        assert!(!report.database_snapshot_includes_wal);
        assert!(!report.database_sidecar_write_permitted);
        assert!(report.paths_redacted);
        assert!(!report.user_filenames_read);
        assert!(report
            .blockers
            .contains(&"icloud-upload-queue-nonempty".to_string()));
        assert!(report
            .blockers
            .contains(&"icloud-upload-blocked-on-sync-up".to_string()));
        assert!(report
            .blockers
            .contains(&"icloud-upload-queue-state-unclassified".to_string()));
        assert!(report
            .blockers
            .contains(&"icloud-local-sync-item-error-present".to_string()));
        assert!(report
            .new_copy_admission_blockers
            .contains(&"icloud-upload-queue-nonempty".to_string()));
        assert!(report
            .new_copy_admission_blockers
            .contains(&"icloud-sync-health-evidence-incomplete".to_string()));
        assert!(report
            .notices
            .contains(&"icloud-item-error-octagon-not-signed-in".to_string()));
        assert!(report
            .notices
            .contains(&"icloud-item-error-older-than-24h".to_string()));
        assert_eq!(
            require_new_copy_admission(&report).unwrap_err(),
            "icloud-sync-health-evidence-incomplete"
        );
        let encoded = serde_json::to_string(&report).unwrap();
        assert!(!encoded.contains("/Users/"));
    }

    #[test]
    fn quiet_global_queue_still_requires_per_item_attestation() {
        let report =
            build_report(1, vec![], IcloudUploadQueueSummary::default(), true, true).unwrap();
        assert!(!report.sync_backlog_present);
        assert_eq!(report.new_copy_admission_state, "clear");
        assert!(report.new_copy_admission_blockers.is_empty());
        assert!(require_new_copy_admission(&report).is_ok());
        assert_eq!(
            report.blockers,
            ["provider-native-per-item-sync-attestation-required-before-eviction"]
        );
        assert!(!report.local_eviction_authorized);
        assert!(report.evidence_complete);
        assert!(report.database_snapshot_includes_wal);
        assert!(!report.database_sidecar_write_permitted);
        assert!(report
            .notices
            .contains(&"source-sqlite-wal-included".to_string()));
    }

    #[test]
    fn incomplete_native_status_blocks_new_copy_admission() {
        let mut report =
            build_report(1, vec![], IcloudUploadQueueSummary::default(), true, true).unwrap();
        report.native_status = Some(parse_native_status_output("", 1, false, true, false));
        attach_native_status_admission(&mut report);

        assert_eq!(report.new_copy_admission_state, "blocked");
        assert_eq!(
            report.new_copy_admission_blockers,
            [
                "icloud-native-status-evidence-incomplete",
                "icloud-native-status-command-timeout"
            ]
        );
        assert_eq!(
            require_new_copy_admission(&report).unwrap_err(),
            "icloud-native-status-evidence-incomplete,icloud-native-status-command-timeout"
        );
    }

    #[test]
    fn native_status_timeout_blocks_new_copy_even_with_bounded_summary() {
        let mut report =
            build_report(1, vec![], IcloudUploadQueueSummary::default(), true, true).unwrap();
        report.native_status = Some(parse_native_status_output(
            "1 containers matching '*'\n\
             foreground {client:needs-sync server:full-sync sync:needs-sync-down last-sync:now}\n",
            1,
            false,
            true,
            false,
        ));
        attach_native_status_admission(&mut report);

        assert_eq!(
            report.new_copy_admission_blockers,
            [
                "icloud-native-status-command-timeout",
                "icloud-native-sync-down-pending"
            ]
        );
        assert!(require_new_copy_admission(&report).is_err());
    }

    #[test]
    fn sqlite_uri_rejects_relative_and_query_delimiters() {
        assert!(sqlite_uri(Path::new("relative/client.db")).is_err());
        assert!(sqlite_uri(Path::new("/tmp/client?.db")).is_err());
        assert_eq!(
            sqlite_uri(Path::new("/tmp/client.db")).unwrap(),
            "file:/tmp/client.db?immutable=1&mode=ro"
        );
        assert_eq!(
            temporary_snapshot_uri(Path::new("/tmp/client.db")).unwrap(),
            "file:/tmp/client.db?mode=rw"
        );
    }

    #[test]
    fn default_path_is_scoped_under_the_supplied_home() {
        assert_eq!(
            default_cloud_docs_db_dir(Path::new("/home/test")),
            PathBuf::from("/home/test/Library/Application Support/CloudDocs/session/db")
        );
    }

    #[test]
    fn oversized_cloud_docs_database_fails_closed_before_snapshot_copy() {
        let source = tempfile::tempdir().unwrap();
        let client_db = source.path().join("client.db");
        fs::File::create(&client_db)
            .unwrap()
            .set_len(MAX_SNAPSHOT_SOURCE_BYTES + 1)
            .unwrap();

        let error = match clone_client_database_snapshot(source.path()) {
            Ok(_) => panic!("oversized database must not be snapshotted"),
            Err(error) => error,
        };
        assert_eq!(error, "icloud-sync-health-snapshot-source-too-large");

        fs::write(&client_db, b"within-limit").unwrap();
        let client_db_wal = source.path().join("client.db-wal");
        fs::File::create(&client_db_wal)
            .unwrap()
            .set_len(MAX_SNAPSHOT_SOURCE_BYTES + 1)
            .unwrap();

        let error = match clone_client_database_snapshot(source.path()) {
            Ok(_) => panic!("oversized WAL must not be snapshotted"),
            Err(error) => error,
        };
        assert_eq!(error, "icloud-sync-health-snapshot-source-too-large");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn oversized_cloud_docs_database_skips_expensive_native_status_probe() {
        let source = tempfile::tempdir().unwrap();
        fs::File::create(source.path().join("client.db"))
            .unwrap()
            .set_len(MAX_SNAPSHOT_SOURCE_BYTES + 1)
            .unwrap();
        assert!(bounded_native_status(source.path(), 1).is_none());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn copy_on_write_snapshot_clones_main_and_wal_then_removes_temporary_files() {
        let source = tempfile::tempdir().unwrap();
        fs::write(source.path().join("client.db"), b"main-database").unwrap();
        fs::write(source.path().join("client.db-wal"), b"wal-records").unwrap();

        let snapshot = clone_client_database_snapshot(source.path()).unwrap();
        let temporary_root = snapshot._directory.path.clone();
        assert!(snapshot.includes_wal);
        assert_eq!(fs::read(&snapshot.client_db).unwrap(), b"main-database");
        assert_eq!(
            fs::read(temporary_root.join("client.db-wal")).unwrap(),
            b"wal-records"
        );
        drop(snapshot);
        assert!(!temporary_root.exists());
    }

    #[test]
    fn admission_notice_is_replaced_without_disturbing_other_plan_notices() {
        let blocked = build_report(
            1,
            vec![],
            parse_queue_rows(queue_output()).unwrap(),
            false,
            false,
        )
        .unwrap();
        let clear =
            build_report(2, vec![], IcloudUploadQueueSummary::default(), true, false).unwrap();
        let mut notices = vec![
            "dry-run-only".into(),
            "icloud-new-copy-admission-evidence-unavailable".into(),
            "cloud-sync-unverified".into(),
        ];

        attach_new_copy_admission_notice(&mut notices, Some(&blocked));
        assert_eq!(
            notices,
            [
                "dry-run-only",
                "cloud-sync-unverified",
                "icloud-new-copy-admission-blocked"
            ]
        );
        attach_new_copy_admission_notice(&mut notices, Some(&clear));
        assert_eq!(
            notices,
            [
                "dry-run-only",
                "cloud-sync-unverified",
                "icloud-new-copy-admission-clear"
            ]
        );
        attach_new_copy_admission_notice(&mut notices, None);
        assert_eq!(
            notices,
            [
                "dry-run-only",
                "cloud-sync-unverified",
                "icloud-new-copy-admission-evidence-unavailable"
            ]
        );
    }

    #[test]
    fn health_evidence_projection_is_path_free_and_integrity_bound() {
        let report = build_report(
            1,
            vec![ManagedDatabaseFileEvidence {
                role: "client.db".into(),
                present: true,
                logical_bytes: 4,
                allocated_bytes: 8,
                modified_ms: Some(1),
            }],
            parse_queue_rows(queue_output()).unwrap(),
            false,
            false,
        )
        .unwrap();
        let snapshot = health_evidence_snapshot_from_report(&report).unwrap();
        validate_icloud_sync_health_evidence_snapshot(&snapshot).unwrap();
        let encoded = serde_json::to_string(&snapshot).unwrap();
        assert!(!encoded.contains("/Users/"));
        assert!(!encoded.contains("client.db"));

        let mut tampered = snapshot;
        tampered.sync_backlog_present = !tampered.sync_backlog_present;
        assert_eq!(
            validate_icloud_sync_health_evidence_snapshot(&tampered).unwrap_err(),
            "icloud-sync-health-evidence-fingerprint-invalid"
        );
    }

    #[cfg(not(coverage))]
    #[test]
    fn health_evidence_is_create_only_and_bounded() {
        let directory = tempfile::tempdir().unwrap();
        let report = build_report(1, vec![], IcloudUploadQueueSummary::default(), true, true)
            .unwrap();
        let first = write_icloud_sync_health_evidence(directory.path(), &report).unwrap();
        assert!(first.exists());
        assert!(write_icloud_sync_health_evidence(directory.path(), &report).is_err());

        for observed_at_ms in 2..=(MAX_PERSISTED_HEALTH_SNAPSHOTS as u64 + 1) {
            let report = build_report(
                observed_at_ms,
                vec![],
                IcloudUploadQueueSummary::default(),
                true,
                true,
            )
            .unwrap();
            write_icloud_sync_health_evidence(directory.path(), &report).unwrap();
        }
        let records = std::fs::read_dir(
            directory
                .path()
                .join(ICLOUD_SYNC_HEALTH_EVIDENCE_DIRECTORY),
        )
        .unwrap()
        .filter_map(Result::ok)
        .count();
        assert_eq!(records, MAX_PERSISTED_HEALTH_SNAPSHOTS);
        assert!(!first.exists());
    }

    #[test]
    fn health_evidence_rejects_unsafe_report_claims() {
        let mut report =
            build_report(1, vec![], IcloudUploadQueueSummary::default(), true, true).unwrap();
        report.paths_redacted = false;
        assert_eq!(
            health_evidence_snapshot_from_report(&report).unwrap_err(),
            "icloud-sync-health-evidence-claim-invalid"
        );

        report.paths_redacted = true;
        report.new_copy_admission_blockers.push("test-blocker".into());
        assert_eq!(
            health_evidence_snapshot_from_report(&report).unwrap_err(),
            "icloud-sync-health-evidence-claim-invalid"
        );
    }
}
