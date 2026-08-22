//! Real SQLite coverage for the non-stale iCloud item-error notice branch.
//!
//! The production report distinguishes a recent local CloudDocs item error from one whose newest
//! timestamp is already at least 24 hours old. This regression reaches that distinction through the
//! shipped public probe while preserving the same read-only snapshot and privacy boundaries.

#![cfg(target_os = "linux")]

use disksage_lib::icloud_sync_health::{probe_icloud_sync_health, require_new_copy_admission};
use std::path::Path;
use std::process::{Command, Stdio};

const SQLITE3_PATH: &str = "/usr/bin/sqlite3";

fn create_recent_error_database(client_db: &Path) {
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
        INSERT INTO item_errors VALUES ('1970-01-01 00:01:39', 'example.recent', 7);
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
fn recent_item_error_blocks_copy_without_being_reported_as_older_than_24_hours() {
    let temp = tempfile::tempdir().unwrap();
    let db_dir = temp.path().join("db");
    std::fs::create_dir(&db_dir).unwrap();
    let client_db = db_dir.join("client.db");
    create_recent_error_database(&client_db);
    let before = std::fs::read(&client_db).unwrap();

    let report = probe_icloud_sync_health(&db_dir, 100_000).unwrap();

    assert!(report.evidence_complete);
    assert_eq!(report.upload_queue.item_error_count, 1);
    assert_eq!(report.upload_queue.item_error_octagon_not_signed_in_count, 0);
    assert_eq!(report.upload_queue.item_error_unclassified_count, 1);
    assert_eq!(report.upload_queue.newest_item_error_timestamp_ms, Some(99_000));
    assert!(report.sync_backlog_present);
    assert_eq!(report.new_copy_admission_state, "blocked");
    assert_eq!(
        report.new_copy_admission_blockers,
        vec!["icloud-local-sync-item-error-present"]
    );
    assert_eq!(
        require_new_copy_admission(&report).unwrap_err(),
        "icloud-local-sync-item-error-present"
    );
    assert!(report.notices.contains(&"icloud-item-error-unclassified".into()));
    assert!(!report.notices.contains(&"icloud-item-error-octagon-not-signed-in".into()));
    assert!(!report.notices.contains(&"icloud-item-error-older-than-24h".into()));
    assert!(report
        .blockers
        .contains(&"provider-native-per-item-sync-attestation-required-before-eviction".into()));
    assert!(!report.remote_capacity_verified);
    assert!(!report.provider_sync_attested);
    assert!(!report.local_eviction_authorized);
    assert!(!report.mutation_performed);

    assert_eq!(std::fs::read(&client_db).unwrap(), before);
    assert!(!db_dir.join("client.db-shm").exists());
    assert!(!db_dir.join("client.db-wal").exists());
}
