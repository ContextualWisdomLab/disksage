#![cfg(unix)]

use disksage_lib::cloud_local_eviction::write_immutable_record;
use std::os::unix::fs::PermissionsExt;

#[test]
fn shared_writable_local_eviction_record_directory_fails_closed() {
    for unsafe_write_bit in [0o020, 0o002] {
        let directory = tempfile::tempdir().unwrap();
        std::fs::set_permissions(
            directory.path(),
            std::fs::Permissions::from_mode(0o700 | unsafe_write_bit),
        )
        .unwrap();

        let error = write_immutable_record(
            directory.path(),
            "approval.json",
            &serde_json::json!({"authority": "human-approved-local-eviction"}),
        )
        .expect_err("shared-writable local-eviction authority must fail closed");

        assert_eq!(error, "icloud-local-eviction-record-dir-writable-by-others");
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
    }
}

#[test]
fn local_eviction_record_is_private_from_creation_and_object_bound_for_hardening() {
    let source = std::fs::read_to_string(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/cloud_local_eviction.rs"),
    )
    .expect("cloud local eviction source must be readable");

    assert!(
        source.contains("options.mode(0o400);"),
        "local eviction approval/result records must be owner-read-only from create_new"
    );
    assert!(
        source.contains("file.set_permissions(permissions)"),
        "post-write local eviction hardening must stay bound to the opened record object"
    );
    assert!(
        !source.contains("std::fs::set_permissions(&path, permissions)"),
        "local eviction record hardening must not re-resolve a replaceable pathname"
    );
}
