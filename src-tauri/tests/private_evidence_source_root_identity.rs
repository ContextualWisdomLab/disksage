#![cfg(unix)]

#[path = "../src/private_evidence.rs"]
mod private_evidence;

use private_evidence::write_object_bound_bytes_create_new_with_hooks;
use std::fs;
use std::os::unix::fs::PermissionsExt;

#[test]
fn source_root_replacement_before_create_fails_closed() {
    let fixture = tempfile::tempdir().expect("tempdir");
    let source = fixture.path().join("source");
    let moved_source = fixture.path().join("source-moved");
    let private = fixture.path().join("private");
    fs::create_dir(&source).expect("create source root");
    fs::create_dir(&private).expect("create private destination");
    fs::set_permissions(&source, fs::Permissions::from_mode(0o700)).expect("set source mode");
    fs::set_permissions(&private, fs::Permissions::from_mode(0o700))
        .expect("set destination mode");
    let target = private.join("audit.json");
    let source_for_hook = source.clone();
    let moved_for_hook = moved_source.clone();

    let error = write_object_bound_bytes_create_new_with_hooks(
        &target,
        b"private",
        0o600,
        Some(&source),
        || {},
        move || {
            fs::rename(&source_for_hook, &moved_for_hook).expect("move admitted source root");
            fs::create_dir(&source_for_hook).expect("replace source-root pathname");
            fs::set_permissions(&source_for_hook, fs::Permissions::from_mode(0o700))
                .expect("set replacement source mode");
        },
        || {},
    )
    .expect_err("source-root identity drift must fail before record creation");

    assert_eq!(format!("{error:?}"), "ForbiddenRootIdentityDrift");
    assert!(!target.exists(), "source-root drift must not publish a record");
}
