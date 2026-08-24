//! Deterministic public-contract coverage for iCloud copy-admission projection.
//!
//! These tests do not invoke sqlite3 or inspect a real CloudDocs database. They exercise path
//! derivation, fail-closed missing-evidence behavior, and stable privacy-safe notice projection.

use disksage_lib::icloud_sync_health::{
    attach_new_copy_admission_notice, default_cloud_docs_db_dir, inspect_new_copy_admission,
    IcloudSyncHealthReport, IcloudUploadQueueSummary, ICLOUD_SYNC_HEALTH_SCHEMA_VERSION,
};

fn admission_report(state: &str, blockers: &[&str]) -> IcloudSyncHealthReport {
    IcloudSyncHealthReport {
        schema_version: ICLOUD_SYNC_HEALTH_SCHEMA_VERSION,
        output_mode: "icloud-local-sync-health".into(),
        observed_at_ms: 7,
        provider: "icloud".into(),
        evidence_kind: "supplementary-local-cloud-docs-private-schema".into(),
        evidence_complete: true,
        database_snapshot_includes_wal: false,
        database_sidecar_write_permitted: false,
        managed_database_files: Vec::new(),
        managed_database_allocated_bytes: 0,
        upload_queue: IcloudUploadQueueSummary::default(),
        sync_backlog_present: !blockers.is_empty(),
        new_copy_admission_state: state.into(),
        new_copy_admission_blockers: blockers.iter().map(|value| (*value).into()).collect(),
        blockers: Vec::new(),
        notices: Vec::new(),
        paths_redacted: true,
        user_filenames_read: false,
        user_file_contents_read: false,
        remote_capacity_verified: false,
        provider_sync_attested: false,
        local_eviction_authorized: false,
        mutation_performed: false,
    }
}

#[test]
fn default_database_path_is_derived_without_filesystem_access() {
    let home = std::path::Path::new("/Users/coverage");
    assert_eq!(
        default_cloud_docs_db_dir(home),
        home.join("Library")
            .join("Application Support")
            .join("CloudDocs")
            .join("session")
            .join("db")
    );
}

#[test]
fn home_level_inspection_fails_closed_when_cloud_docs_database_is_absent() {
    let temp = tempfile::tempdir().unwrap();
    assert_eq!(
        inspect_new_copy_admission(temp.path(), 9).unwrap_err(),
        "icloud-sync-health-db-dir-unavailable"
    );
}

#[test]
fn admission_notice_projection_replaces_only_icloud_admission_state() {
    let clear = admission_report("clear", &[]);
    let blocked = admission_report("blocked", &["icloud-upload-in-flight"]);
    let baseline = vec![
        "dry-run-only".to_string(),
        "cloud-sync-unverified".to_string(),
        "icloud-new-copy-admission-evidence-unavailable".to_string(),
        "icloud-new-copy-admission-blocked".to_string(),
        "icloud-new-copy-admission-clear".to_string(),
    ];

    let mut notices = baseline.clone();
    attach_new_copy_admission_notice(&mut notices, Some(&clear));
    assert_eq!(
        notices,
        [
            "dry-run-only",
            "cloud-sync-unverified",
            "icloud-new-copy-admission-clear"
        ]
    );

    let mut notices = baseline.clone();
    attach_new_copy_admission_notice(&mut notices, Some(&blocked));
    assert_eq!(
        notices,
        [
            "dry-run-only",
            "cloud-sync-unverified",
            "icloud-new-copy-admission-blocked"
        ]
    );

    let mut notices = baseline;
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
