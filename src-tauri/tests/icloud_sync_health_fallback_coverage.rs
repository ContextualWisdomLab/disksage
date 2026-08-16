//! Coverage for the bounded iCloud snapshot fallback contract.
//!
//! A CloudDocs database can be much larger than the bounded temporary-copy budget. The public
//! health probe must then avoid cloning the source, fall back to the immutable read-only query,
//! mark the resulting evidence incomplete, and keep copy admission fail-closed. The fixture below
//! is sparse: its logical size crosses the production copy limit without consuming that space.

#![cfg(target_os = "linux")]

use disksage_lib::icloud_sync_health::{probe_icloud_sync_health, require_new_copy_admission};
use std::path::Path;
use std::process::{Command, Stdio};

const SQLITE3_PATH: &str = "/usr/bin/sqlite3";
const OVER_SNAPSHOT_LIMIT_BYTES: u64 = 512 * 1024 * 1024 + 4096;

fn create_quiet_cloud_docs_database(client_db: &Path) {
    assert!(Path::new(SQLITE3_PATH).is_file(), "coverage runner must provide {SQLITE3_PATH}");
    let schema = r#"
        PRAGMA journal_mode=DELETE;
        CREATE TABLE client_uploads (
            transfer_size INTEGER NOT NULL,
            throttle_state INTEGER NOT NULL,
            transfer_operation TEXT
        );
        CREATE INDEX "client_uploads/scheduling_by_priority"
            ON client_uploads(throttle_state, transfer_operation);
        CREATE INDEX "client_uploads/transfer_operation"
            ON client_uploads(transfer_operation);
        CREATE INDEX "client_uploads/blocked_on_sync_up_state_index"
            ON client_uploads(throttle_state);
        CREATE INDEX "client_uploads/out_of_quota_index"
            ON client_uploads(throttle_state);
        CREATE INDEX "client_uploads/gc_index"
            ON client_uploads(throttle_state);
        CREATE INDEX "client_uploads/throttle_state"
            ON client_uploads(throttle_state);
        CREATE TABLE item_errors (
            error_timestamp TEXT NOT NULL,
            error_domain TEXT NOT NULL,
            error_code INTEGER NOT NULL
        );
    "#;
    let status = Command::new(SQLITE3_PATH)
        .arg(client_db)
        .arg(schema)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("launch sqlite3 fixture builder");
    assert!(status.success(), "sqlite3 must create the bounded CloudDocs fixture");
}

#[test]
fn oversized_sparse_source_falls_back_to_incomplete_immutable_evidence() {
    let temp = tempfile::tempdir().unwrap();
    let db_dir = temp.path().join("db");
    std::fs::create_dir(&db_dir).unwrap();
    let client_db = db_dir.join("client.db");
    create_quiet_cloud_docs_database(&client_db);

    let file = std::fs::OpenOptions::new().write(true).open(&client_db).unwrap();
    file.set_len(OVER_SNAPSHOT_LIMIT_BYTES).unwrap();
    drop(file);
    assert_eq!(std::fs::metadata(&client_db).unwrap().len(), OVER_SNAPSHOT_LIMIT_BYTES);
    let source_entries = std::fs::read_dir(&db_dir).unwrap().count();

    let report = probe_icloud_sync_health(&db_dir, 42).unwrap();

    assert!(!report.evidence_complete);
    assert!(!report.database_snapshot_includes_wal);
    assert!(!report.database_sidecar_write_permitted);
    assert_eq!(report.upload_queue.scheduled_count, 0);
    assert_eq!(report.upload_queue.scheduled_bytes, 0);
    assert!(!report.sync_backlog_present);
    assert_eq!(report.new_copy_admission_state, "blocked");
    assert_eq!(
        report.new_copy_admission_blockers,
        vec!["icloud-sync-health-evidence-incomplete"]
    );
    assert_eq!(
        require_new_copy_admission(&report).unwrap_err(),
        "icloud-sync-health-evidence-incomplete"
    );
    assert!(report.notices.contains(&"read-only-immutable-main-database-snapshot".into()));
    assert!(report.notices.contains(&"sqlite-wal-not-applied-to-avoid-sidecar-writes".into()));
    assert!(report.notices.contains(&"snapshot-may-lag-active-cloud-docs-state".into()));
    assert!(report.notices.contains(&"consistent-copy-on-write-snapshot-unavailable".into()));
    assert!(report
        .blockers
        .contains(&"provider-native-per-item-sync-attestation-required-before-eviction".into()));
    assert!(report.paths_redacted);
    assert!(!report.remote_capacity_verified);
    assert!(!report.provider_sync_attested);
    assert!(!report.local_eviction_authorized);
    assert!(!report.mutation_performed);

    assert_eq!(std::fs::metadata(&client_db).unwrap().len(), OVER_SNAPSHOT_LIMIT_BYTES);
    assert_eq!(std::fs::read_dir(&db_dir).unwrap().count(), source_entries);
    assert!(!db_dir.join("client.db-shm").exists());
    assert!(!db_dir.join("client.db-wal").exists());
}
