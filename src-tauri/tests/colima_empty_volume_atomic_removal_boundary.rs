#![cfg(unix)]

use disksage_lib::colima_reclaim::execute_colima_empty_volumes;
use std::fs;
use std::os::unix::fs::PermissionsExt;

#[test]
fn colima_empty_volume_cleanup_refuses_before_any_non_atomic_provider_probe() {
    let temp = tempfile::tempdir().expect("temporary test directory should be creatable");
    let fake_colima = temp.path().join("colima");
    let invocation_marker = temp.path().join("colima-invoked");
    fs::write(
        &fake_colima,
        format!(
            "#!/bin/sh\nprintf invoked > '{}'\nexit 1\n",
            invocation_marker.display()
        ),
    )
    .expect("fake Colima should be writable");
    let mut permissions = fs::metadata(&fake_colima)
        .expect("fake Colima metadata should be readable")
        .permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&fake_colima, permissions).expect("fake Colima should be executable");

    let error = execute_colima_empty_volumes(
        &fake_colima,
        "default",
        "historical confirmation",
        "reviewed reclaim",
        1,
    )
    .expect_err("non-atomic Colima empty-volume deletion must be unavailable");

    assert_eq!(error, "colima-empty-volume-atomic-removal-unavailable");
    assert!(
        !invocation_marker.exists(),
        "the public contract must reject before a check-then-remove provider call"
    );
}
