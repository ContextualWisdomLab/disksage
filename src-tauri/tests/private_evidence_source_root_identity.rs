#![cfg(unix)]

#[path = "../src/private_evidence.rs"]
mod private_evidence;

use private_evidence::write_object_bound_bytes_create_new_with_hooks;
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::Path;

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

#[test]
fn source_root_alias_retarget_before_object_binding_fails_closed() {
    let fixture = tempfile::tempdir().expect("tempdir");
    let source = fixture.path().join("source-real");
    let decoy = fixture.path().join("source-decoy");
    let source_alias = fixture.path().join("source-alias");
    fs::create_dir(&source).expect("create source root");
    fs::create_dir(&decoy).expect("create decoy root");
    fs::set_permissions(&source, fs::Permissions::from_mode(0o700)).expect("set source mode");
    fs::set_permissions(&decoy, fs::Permissions::from_mode(0o700)).expect("set decoy mode");
    symlink(&source, &source_alias).expect("create source alias");

    let target = source.join("audit.json");
    let alias_for_hook = source_alias.clone();
    let decoy_for_hook = decoy.clone();

    let error = write_object_bound_bytes_create_new_with_hooks(
        &target,
        b"private",
        0o600,
        Some(&source_alias),
        move || {
            fs::remove_file(&alias_for_hook).expect("remove admitted source alias");
            symlink(&decoy_for_hook, &alias_for_hook).expect("retarget source alias");
        },
        || {},
        || {},
    )
    .expect_err("retargeting the supplied source-root authority before object binding must fail closed");

    assert_eq!(format!("{error:?}"), "ForbiddenRootIdentityDrift");
    assert!(
        !target.exists(),
        "retargeted source-root authority must not permit publication inside the originally supplied root"
    );
}

#[test]
fn relative_source_root_is_rejected_before_publication() {
    let fixture = tempfile::tempdir().expect("tempdir");
    let private = fixture.path().join("private");
    fs::create_dir(&private).expect("create private destination");
    fs::set_permissions(&private, fs::Permissions::from_mode(0o700))
        .expect("set destination mode");
    let target = private.join("audit.json");

    let error = private_evidence::write_private_json_create_new(
        Path::new("."),
        &target,
        &serde_json::json!({"private": true}),
    )
    .expect_err("relative source-root authority must not depend on ambient process CWD");

    assert_eq!(error, "private-evidence-source-root-invalid");
    assert!(
        !target.exists(),
        "relative source-root authority must fail before private record publication"
    );
}
