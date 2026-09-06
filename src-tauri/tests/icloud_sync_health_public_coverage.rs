use disksage_lib::icloud_sync_health::{
    attach_new_copy_admission_notice, default_cloud_docs_db_dir, inspect_new_copy_admission,
    probe_icloud_sync_health, require_new_copy_admission, IcloudSyncHealthReport,
    IcloudUploadQueueSummary, ICLOUD_SYNC_HEALTH_SCHEMA_VERSION,
};
use std::fs;
use std::path::Path;

fn admission_report() -> IcloudSyncHealthReport {
    IcloudSyncHealthReport {
        schema_version: ICLOUD_SYNC_HEALTH_SCHEMA_VERSION,
        output_mode: "icloud-local-sync-health".into(),
        observed_at_ms: 1,
        provider: "icloud".into(),
        evidence_kind: "supplementary-local-cloud-docs-private-schema".into(),
        evidence_complete: true,
        database_snapshot_includes_wal: false,
        database_sidecar_write_permitted: false,
        managed_database_files: Vec::new(),
        managed_database_allocated_bytes: 0,
        upload_queue: IcloudUploadQueueSummary::default(),
        native_status: None,
        file_provider_activity: None,
        sync_backlog_present: false,
        new_copy_admission_state: "clear".into(),
        new_copy_admission_blockers: Vec::new(),
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

fn sorted_entry_names(directory: &Path) -> Vec<String> {
    let mut names = fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    names.sort();
    names
}

#[test]
fn public_probe_rejects_unsafe_database_directory_shapes_before_sqlite() {
    assert_eq!(
        probe_icloud_sync_health(Path::new("relative/cloud-docs-db"), 1).unwrap_err(),
        "icloud-sync-health-db-dir-not-absolute"
    );

    let temp = tempfile::tempdir().unwrap();
    let missing = temp.path().join("missing");
    assert_eq!(
        probe_icloud_sync_health(&missing, 1).unwrap_err(),
        "icloud-sync-health-db-dir-unavailable"
    );

    let regular_file = temp.path().join("not-a-directory");
    fs::write(&regular_file, b"not a database directory").unwrap();
    assert_eq!(
        probe_icloud_sync_health(&regular_file, 1).unwrap_err(),
        "icloud-sync-health-db-dir-unsafe"
    );

    #[cfg(unix)]
    {
        let real_directory = temp.path().join("real-db-directory");
        fs::create_dir(&real_directory).unwrap();
        let linked_directory = temp.path().join("linked-db-directory");
        std::os::unix::fs::symlink(&real_directory, &linked_directory).unwrap();
        assert_eq!(
            probe_icloud_sync_health(&linked_directory, 1).unwrap_err(),
            "icloud-sync-health-db-dir-unsafe"
        );
    }
}

#[test]
fn public_probe_rejects_missing_or_non_regular_managed_database_files() {
    let temp = tempfile::tempdir().unwrap();
    let db_dir = temp.path().join("db");
    fs::create_dir(&db_dir).unwrap();

    assert_eq!(
        probe_icloud_sync_health(&db_dir, 1).unwrap_err(),
        "icloud-sync-health-client.db-unavailable"
    );

    let client_db = db_dir.join("client.db");
    fs::create_dir(&client_db).unwrap();
    assert_eq!(
        probe_icloud_sync_health(&db_dir, 1).unwrap_err(),
        "icloud-sync-health-client.db-not-regular-file"
    );
    fs::remove_dir(&client_db).unwrap();
    fs::write(&client_db, b"sqlite fixture placeholder").unwrap();

    let original_client_db = fs::read(&client_db).unwrap();
    let source_entries = sorted_entry_names(&db_dir);
    let error = probe_icloud_sync_health(&db_dir, 1).unwrap_err();
    assert!(error.starts_with("icloud-sync-health-"), "{error}");
    assert_eq!(fs::read(&client_db).unwrap(), original_client_db);
    assert_eq!(sorted_entry_names(&db_dir), source_entries);

    #[cfg(unix)]
    {
        let real_client_db = temp.path().join("real-client.db");
        fs::write(&real_client_db, b"sqlite fixture target").unwrap();
        fs::remove_file(&client_db).unwrap();
        std::os::unix::fs::symlink(&real_client_db, &client_db).unwrap();
        assert_eq!(
            probe_icloud_sync_health(&db_dir, 1).unwrap_err(),
            "icloud-sync-health-client.db-symlink-rejected"
        );
        fs::remove_file(&client_db).unwrap();
        fs::write(&client_db, b"sqlite fixture placeholder").unwrap();
    }

    let optional_sidecar = db_dir.join("client.db-shm");
    fs::create_dir(&optional_sidecar).unwrap();
    assert_eq!(
        probe_icloud_sync_health(&db_dir, 1).unwrap_err(),
        "icloud-sync-health-client.db-shm-not-regular-file"
    );

    #[cfg(unix)]
    {
        fs::remove_dir(&optional_sidecar).unwrap();
        std::os::unix::fs::symlink(&client_db, &optional_sidecar).unwrap();
        assert_eq!(
            probe_icloud_sync_health(&db_dir, 1).unwrap_err(),
            "icloud-sync-health-client.db-shm-symlink-rejected"
        );
    }
}

#[test]
fn public_admission_gate_rejects_inconsistent_or_explicitly_blocked_reports() {
    let clear = admission_report();
    assert!(require_new_copy_admission(&clear).is_ok());

    let mut inconsistent = admission_report();
    inconsistent.new_copy_admission_state = "blocked".into();
    assert_eq!(
        require_new_copy_admission(&inconsistent).unwrap_err(),
        "icloud-new-copy-admission-invalid"
    );

    let mut blocked = admission_report();
    blocked.new_copy_admission_state = "blocked".into();
    blocked.new_copy_admission_blockers = vec![
        "icloud-upload-out-of-quota".into(),
        "icloud-upload-in-flight".into(),
    ];
    assert_eq!(
        require_new_copy_admission(&blocked).unwrap_err(),
        "icloud-upload-out-of-quota,icloud-upload-in-flight"
    );

    let mut incomplete = admission_report();
    incomplete.evidence_complete = false;
    assert_eq!(
        require_new_copy_admission(&incomplete).unwrap_err(),
        "icloud-sync-health-evidence-incomplete"
    );
}

#[test]
fn public_copy_admission_helpers_replace_only_owned_notices() {
    let clear = admission_report();
    let mut blocked = admission_report();
    blocked.new_copy_admission_state = "blocked".into();
    blocked.new_copy_admission_blockers = vec!["icloud-upload-in-flight".into()];

    let mut notices = vec![
        "unrelated-evidence".into(),
        "icloud-new-copy-admission-blocked".into(),
        "icloud-new-copy-admission-evidence-unavailable".into(),
    ];
    attach_new_copy_admission_notice(&mut notices, Some(&clear));
    assert_eq!(
        notices,
        vec!["unrelated-evidence", "icloud-new-copy-admission-clear"]
    );

    attach_new_copy_admission_notice(&mut notices, Some(&blocked));
    assert_eq!(
        notices,
        vec!["unrelated-evidence", "icloud-new-copy-admission-blocked"]
    );

    attach_new_copy_admission_notice(&mut notices, None);
    assert_eq!(
        notices,
        vec![
            "unrelated-evidence",
            "icloud-new-copy-admission-evidence-unavailable"
        ]
    );
}

#[test]
fn public_default_database_path_and_home_inspection_preserve_absolute_path_authority() {
    let home = Path::new("/Users/disk-sage-coverage");
    assert_eq!(
        default_cloud_docs_db_dir(home),
        home.join("Library")
            .join("Application Support")
            .join("CloudDocs")
            .join("session")
            .join("db")
    );

    assert_eq!(
        inspect_new_copy_admission(Path::new("relative-home"), 123).unwrap_err(),
        "icloud-sync-health-db-dir-not-absolute"
    );
}
