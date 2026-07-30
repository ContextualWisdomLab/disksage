//! Read-only, path-free health evidence for the local macOS CloudDocs sync database.
//!
//! This module intentionally treats CloudDocs' private SQLite schema as supplementary local
//! evidence. A quiet global queue is not per-item remote-upload attestation and never authorizes
//! local eviction.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

const SQLITE3_PATH: &str = "/usr/bin/sqlite3";
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_STDOUT_BYTES: usize = 16 * 1024;
const MAX_STDERR_BYTES: usize = 4 * 1024;

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
    pub scheduled_count: u64,
    pub scheduled_bytes: u64,
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
    pub database_sidecar_write_permitted: bool,
    pub managed_database_files: Vec<ManagedDatabaseFileEvidence>,
    pub managed_database_allocated_bytes: u64,
    pub upload_queue: IcloudUploadQueueSummary,
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

fn run_queue_probe(client_db: &Path) -> Result<String, String> {
    let sqlite_metadata = fs::symlink_metadata(SQLITE3_PATH)
        .map_err(|_| "icloud-sync-health-sqlite3-unavailable".to_string())?;
    if sqlite_metadata.file_type().is_symlink() || !sqlite_metadata.is_file() {
        return Err("icloud-sync-health-sqlite3-not-regular-file".into());
    }
    let mut child = Command::new(SQLITE3_PATH)
        .arg("-readonly")
        .arg(sqlite_uri(client_db)?)
        .arg(QUEUE_QUERY)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| "icloud-sync-health-sqlite3-spawn-failed".to_string())?;
    let deadline = Instant::now() + PROBE_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("icloud-sync-health-query-timeout".into());
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("icloud-sync-health-query-wait-failed".into());
            }
        }
    }
    let output = child
        .wait_with_output()
        .map_err(|_| "icloud-sync-health-query-output-failed".to_string())?;
    if output.stdout.len() > MAX_STDOUT_BYTES || output.stderr.len() > MAX_STDERR_BYTES {
        return Err("icloud-sync-health-query-output-oversized".into());
    }
    if !output.status.success() {
        return Err("icloud-sync-health-schema-unsupported".into());
    }
    if !output.stderr.is_empty() {
        return Err("icloud-sync-health-query-stderr-present".into());
    }
    String::from_utf8(output.stdout).map_err(|_| "icloud-sync-health-query-output-not-utf8".into())
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
    if rows.len() != 7 {
        return Err("icloud-sync-health-query-row-unexpected".into());
    }
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
        scheduled_count,
        scheduled_bytes,
    })
}

fn build_report(
    observed_at_ms: u64,
    managed_database_files: Vec<ManagedDatabaseFileEvidence>,
    upload_queue: IcloudUploadQueueSummary,
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
    Ok(IcloudSyncHealthReport {
        schema_version: 1,
        output_mode: "icloud-local-sync-health".into(),
        observed_at_ms,
        provider: "icloud".into(),
        evidence_kind: "supplementary-local-cloud-docs-private-schema".into(),
        evidence_complete: false,
        database_snapshot_includes_wal: false,
        database_sidecar_write_permitted: false,
        managed_database_files,
        managed_database_allocated_bytes,
        upload_queue,
        sync_backlog_present,
        new_copy_admission_state: new_copy_admission_state.into(),
        new_copy_admission_blockers,
        blockers,
        notices: vec![
            "read-only-immutable-main-database-snapshot".into(),
            "sqlite-wal-not-applied-to-avoid-sidecar-writes".into(),
            "snapshot-may-lag-active-cloud-docs-state".into(),
            "cloud-docs-private-schema-is-supplementary-evidence".into(),
            "queue-bytes-are-transfer-size-not-remote-capacity".into(),
            "global-queue-state-is-not-per-item-upload-attestation".into(),
            "paths-and-filenames-redacted".into(),
            "no-cloud-write".into(),
            "no-local-eviction".into(),
        ],
        paths_redacted: true,
        user_filenames_read: false,
        user_file_contents_read: false,
        remote_capacity_verified: false,
        provider_sync_attested: false,
        local_eviction_authorized: false,
        mutation_performed: false,
    })
}

/// Require a quiet local iCloud upload queue before adding another local copy.
///
/// This gate does not claim remote capacity or synchronization. The source remains retained and a
/// copied item still needs provider-native per-item evidence before any later local eviction.
pub fn require_new_copy_admission(report: &IcloudSyncHealthReport) -> Result<(), String> {
    if report.new_copy_admission_state == "clear" && report.new_copy_admission_blockers.is_empty() {
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
                if report.new_copy_admission_state == "clear"
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
    let client_db = db_dir.join("client.db");
    let upload_queue = parse_queue_rows(&run_queue_probe(&client_db)?)?;
    build_report(observed_at_ms, managed_database_files, upload_queue)
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
         item_error|1|0\n"
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
    }

    #[test]
    fn parser_rejects_missing_duplicate_and_unexpected_rows() {
        assert!(parse_queue_rows("scheduled_waiting|1|2\n").is_err());
        assert!(parse_queue_rows(&format!("{}scheduled_waiting|1|2\n", queue_output())).is_err());
        assert!(parse_queue_rows(&format!("{}other|1|2\n", queue_output())).is_err());
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
        let report = build_report(2, files, parse_queue_rows(queue_output()).unwrap()).unwrap();
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
        assert_eq!(
            require_new_copy_admission(&report).unwrap_err(),
            report.new_copy_admission_blockers.join(",")
        );
        let encoded = serde_json::to_string(&report).unwrap();
        assert!(!encoded.contains("/Users/"));
    }

    #[test]
    fn quiet_global_queue_still_requires_per_item_attestation() {
        let report = build_report(1, vec![], IcloudUploadQueueSummary::default()).unwrap();
        assert!(!report.sync_backlog_present);
        assert_eq!(report.new_copy_admission_state, "clear");
        assert!(report.new_copy_admission_blockers.is_empty());
        assert!(require_new_copy_admission(&report).is_ok());
        assert_eq!(
            report.blockers,
            ["provider-native-per-item-sync-attestation-required-before-eviction"]
        );
        assert!(!report.local_eviction_authorized);
    }

    #[test]
    fn sqlite_uri_rejects_relative_and_query_delimiters() {
        assert!(sqlite_uri(Path::new("relative/client.db")).is_err());
        assert!(sqlite_uri(Path::new("/tmp/client?.db")).is_err());
        assert_eq!(
            sqlite_uri(Path::new("/tmp/client.db")).unwrap(),
            "file:/tmp/client.db?immutable=1&mode=ro"
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
    fn admission_notice_is_replaced_without_disturbing_other_plan_notices() {
        let blocked = build_report(1, vec![], parse_queue_rows(queue_output()).unwrap()).unwrap();
        let clear = build_report(2, vec![], IcloudUploadQueueSummary::default()).unwrap();
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
}
