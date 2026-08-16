//! Real SQLite coverage for the public iCloud sync-health boundary on Linux.
//!
//! The production probe shells out only to the fixed `/usr/bin/sqlite3` path. These tests build a
//! minimal CloudDocs-shaped database with that same binary, then exercise the copy-on-write
//! snapshot, queue parser, report projection, privacy contract, and copy-admission decision through
//! the shipped public API. No managed source database, network, provider credential, or mutation
//! authority is involved.

#![cfg(target_os = "linux")]

use disksage_lib::icloud_sync_health::{probe_icloud_sync_health, require_new_copy_admission};
use std::path::Path;
use std::process::{Command, Stdio};

const SQLITE3_PATH: &str = "/usr/bin/sqlite3";

fn create_cloud_docs_database(client_db: &Path, inserts: &str) {
    assert!(Path::new(SQLITE3_PATH).is_file(), "coverage runner must provide {SQLITE3_PATH}");
    let schema = format!(
        r#"
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
        {inserts}
        "#
    );
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
fn public_probe_projects_real_queue_and_error_rows_without_mutating_source() {
    let temp = tempfile::tempdir().unwrap();
    let db_dir = temp.path().join("db");
    std::fs::create_dir(&db_dir).unwrap();
    let client_db = db_dir.join("client.db");
    create_cloud_docs_database(
        &client_db,
        r#"
        INSERT INTO client_uploads VALUES (10, 1, NULL);
        INSERT INTO client_uploads VALUES (20, 1, 'active-upload');
        INSERT INTO client_uploads VALUES (0, 31, NULL);
        INSERT INTO client_uploads VALUES (30, 32, NULL);
        INSERT INTO client_uploads VALUES (0, 0, NULL);
        INSERT INTO client_uploads VALUES (0, 7, NULL);
        INSERT INTO item_errors VALUES ('1970-01-01 00:00:01', 'com.apple.security.octagon', 25);
        INSERT INTO item_errors VALUES ('1970-01-01 00:00:02', 'example.other', 7);
        "#,
    );
    let before = std::fs::read(&client_db).unwrap();
    let before_entries = std::fs::read_dir(&db_dir).unwrap().count();

    let report = probe_icloud_sync_health(&db_dir, 200_000_000).unwrap();

    assert!(report.evidence_complete);
    assert!(!report.database_snapshot_includes_wal);
    assert!(!report.database_sidecar_write_permitted);
    assert_eq!(report.managed_database_files.len(), 6);
    assert_eq!(report.managed_database_files[0].role, "client.db");
    assert!(report.managed_database_files[0].present);
    assert!(report.managed_database_allocated_bytes > 0);
    assert_eq!(report.upload_queue.scheduled_waiting_count, 1);
    assert_eq!(report.upload_queue.scheduled_waiting_bytes, 10);
    assert_eq!(report.upload_queue.scheduled_active_count, 1);
    assert_eq!(report.upload_queue.scheduled_active_bytes, 20);
    assert_eq!(report.upload_queue.blocked_on_sync_up_count, 1);
    assert_eq!(report.upload_queue.out_of_quota_count, 1);
    assert_eq!(report.upload_queue.out_of_quota_bytes, 30);
    assert_eq!(report.upload_queue.garbage_collection_count, 1);
    assert_eq!(report.upload_queue.other_state_count, 1);
    assert_eq!(report.upload_queue.item_error_count, 2);
    assert_eq!(report.upload_queue.item_error_octagon_not_signed_in_count, 1);
    assert_eq!(report.upload_queue.item_error_unclassified_count, 1);
    assert_eq!(report.upload_queue.newest_item_error_timestamp_ms, Some(2_000));
    assert_eq!(report.upload_queue.scheduled_count, 2);
    assert_eq!(report.upload_queue.scheduled_bytes, 30);
    assert!(report.sync_backlog_present);
    assert_eq!(report.new_copy_admission_state, "blocked");
    assert_eq!(
        report.new_copy_admission_blockers,
        vec![
            "icloud-upload-queue-nonempty",
            "icloud-upload-in-flight",
            "icloud-upload-blocked-on-sync-up",
            "icloud-upload-out-of-quota",
            "icloud-upload-queue-state-unclassified",
            "icloud-local-sync-item-error-present",
        ]
    );
    assert!(report
        .blockers
        .contains(&"provider-native-per-item-sync-attestation-required-before-eviction".into()));
    assert!(report.notices.contains(&"read-only-source-copy-on-write-snapshot".into()));
    assert!(report.notices.contains(&"source-sqlite-wal-absent".into()));
    assert!(report.notices.contains(&"icloud-item-error-octagon-not-signed-in".into()));
    assert!(report.notices.contains(&"icloud-item-error-unclassified".into()));
    assert!(report.notices.contains(&"icloud-item-error-older-than-24h".into()));
    assert!(require_new_copy_admission(&report).is_err());
    assert!(report.paths_redacted);
    assert!(!report.user_filenames_read);
    assert!(!report.user_file_contents_read);
    assert!(!report.remote_capacity_verified);
    assert!(!report.provider_sync_attested);
    assert!(!report.local_eviction_authorized);
    assert!(!report.mutation_performed);

    assert_eq!(std::fs::read(&client_db).unwrap(), before);
    assert_eq!(std::fs::read_dir(&db_dir).unwrap().count(), before_entries);
    assert!(!db_dir.join("client.db-shm").exists());
    assert!(!db_dir.join("client.db-wal").exists());
}

#[test]
fn public_probe_projects_a_quiet_real_queue_as_copy_admission_clear_only() {
    let temp = tempfile::tempdir().unwrap();
    let db_dir = temp.path().join("db");
    std::fs::create_dir(&db_dir).unwrap();
    let client_db = db_dir.join("client.db");
    create_cloud_docs_database(&client_db, "");

    let report = probe_icloud_sync_health(&db_dir, 42).unwrap();

    assert!(report.evidence_complete);
    assert!(!report.sync_backlog_present);
    assert_eq!(report.upload_queue.scheduled_count, 0);
    assert_eq!(report.upload_queue.scheduled_bytes, 0);
    assert_eq!(report.upload_queue.item_error_count, 0);
    assert_eq!(report.upload_queue.newest_item_error_timestamp_ms, None);
    assert_eq!(report.new_copy_admission_state, "clear");
    assert!(report.new_copy_admission_blockers.is_empty());
    assert_eq!(
        report.blockers,
        vec!["provider-native-per-item-sync-attestation-required-before-eviction"]
    );
    assert!(require_new_copy_admission(&report).is_ok());
    assert!(!report
        .notices
        .iter()
        .any(|notice| notice.starts_with("icloud-item-error-")));
    assert!(!report.provider_sync_attested);
    assert!(!report.local_eviction_authorized);
    assert!(!report.mutation_performed);
}
