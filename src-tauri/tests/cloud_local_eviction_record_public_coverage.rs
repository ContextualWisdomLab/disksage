//! Public-contract coverage for immutable local-eviction audit records.
//!
//! These tests exercise app-owned record-path and create-new durability boundaries only. They do
//! not request iCloud eviction or mutate user cloud content.

use disksage_lib::cloud_local_eviction::{
    prepare_immutable_record_directory, write_immutable_record,
};
use serde_json::json;
use std::fs;
use std::path::Path;

#[test]
fn record_directory_creation_rejects_unsafe_or_cloud_overlapping_paths() {
    let app_home = tempfile::tempdir().unwrap();
    let cloud_root = tempfile::tempdir().unwrap();
    let app_data = app_home.path().join("state");

    for (candidate, cloud, name) in [
        (Path::new("relative-state"), cloud_root.path(), "records"),
        (app_data.as_path(), Path::new("relative-cloud"), "records"),
        (app_data.as_path(), cloud_root.path(), ""),
        (app_data.as_path(), cloud_root.path(), "../records"),
        (app_data.as_path(), cloud_root.path(), "nested/records"),
    ] {
        assert_eq!(
            prepare_immutable_record_directory(candidate, cloud, name).unwrap_err(),
            "icloud-local-eviction-record-path-invalid"
        );
    }

    let overlapping = cloud_root.path().join("private-state");
    assert_eq!(
        prepare_immutable_record_directory(&overlapping, cloud_root.path(), "records").unwrap_err(),
        "icloud-local-eviction-record-dir-overlaps-cloud-data"
    );

    let records =
        prepare_immutable_record_directory(&app_data, cloud_root.path(), "records").unwrap();
    assert!(records.is_dir());
    assert_eq!(records, app_data.join("records"));

    let file_backed = app_home.path().join("file-backed-state");
    fs::create_dir(&file_backed).unwrap();
    fs::write(file_backed.join("records"), b"not-a-directory").unwrap();
    assert_eq!(
        prepare_immutable_record_directory(&file_backed, cloud_root.path(), "records").unwrap_err(),
        "icloud-local-eviction-record-dir-not-real-directory"
    );
}

#[test]
fn immutable_record_write_is_create_new_read_only_and_path_bounded() {
    let app_home = tempfile::tempdir().unwrap();
    let cloud_root = tempfile::tempdir().unwrap();
    let record_dir = prepare_immutable_record_directory(
        &app_home.path().join("state"),
        cloud_root.path(),
        "records",
    )
    .unwrap();

    for filename in ["", "record", "../record.json", "nested/record.json", "nested\\record.json"] {
        assert_eq!(
            write_immutable_record(&record_dir, filename, &json!({"ok": true})).unwrap_err(),
            "icloud-local-eviction-record-path-invalid"
        );
    }
    assert_eq!(
        write_immutable_record(
            Path::new("relative-record-dir"),
            "record.json",
            &json!({"ok": true}),
        )
        .unwrap_err(),
        "icloud-local-eviction-record-path-invalid"
    );

    let written = write_immutable_record(
        &record_dir,
        "approval.json",
        &json!({"version": 2, "approval_id": "abc"}),
    )
    .unwrap();
    assert_eq!(written, record_dir.join("approval.json"));
    assert!(fs::metadata(&written).unwrap().permissions().readonly());
    let bytes = fs::read(&written).unwrap();
    assert!(bytes.ends_with(b"\n"));
    let decoded: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(decoded["version"], 2);
    assert_eq!(decoded["approval_id"], "abc");

    assert!(write_immutable_record(
        &record_dir,
        "approval.json",
        &json!({"version": 2, "approval_id": "replacement"}),
    )
    .is_err());
    let persisted: serde_json::Value = serde_json::from_slice(&fs::read(&written).unwrap()).unwrap();
    assert_eq!(persisted["approval_id"], "abc");

    let file_dir = app_home.path().join("not-a-directory");
    fs::write(&file_dir, b"file").unwrap();
    assert_eq!(
        write_immutable_record(&file_dir, "record.json", &json!({"ok": true})).unwrap_err(),
        "record-dir-not-real-directory"
    );
    assert_eq!(
        write_immutable_record(
            &app_home.path().join("missing-directory"),
            "record.json",
            &json!({"ok": true}),
        )
        .unwrap_err(),
        "record-dir-unavailable"
    );
}

#[cfg(unix)]
#[test]
fn symlinked_record_directories_fail_closed() {
    use std::os::unix::fs::symlink;

    let app_home = tempfile::tempdir().unwrap();
    let cloud_root = tempfile::tempdir().unwrap();
    let real = app_home.path().join("real-records");
    fs::create_dir(&real).unwrap();
    let linked = app_home.path().join("linked-records");
    symlink(&real, &linked).unwrap();

    assert_eq!(
        write_immutable_record(&linked, "record.json", &json!({"ok": true})).unwrap_err(),
        "record-dir-not-real-directory"
    );

    let app_data = app_home.path().join("state-with-link");
    fs::create_dir(&app_data).unwrap();
    symlink(&real, app_data.join("records")).unwrap();
    assert_eq!(
        prepare_immutable_record_directory(&app_data, cloud_root.path(), "records").unwrap_err(),
        "icloud-local-eviction-record-dir-not-real-directory"
    );
}
