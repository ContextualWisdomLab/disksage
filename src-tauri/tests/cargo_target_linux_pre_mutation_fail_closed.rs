#![cfg(target_os = "linux")]

#[path = "../src/unix_capability_cleanup.rs"]
mod unix_capability_cleanup;
#[path = "../src/unix_holder_authority.rs"]
mod unix_holder_authority;
#[path = "../src/cargo_target_reclaim.rs"]
mod cargo_target_reclaim;

use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

#[test]
fn linux_cargo_target_reclaim_fails_closed_before_any_destructive_mutation() {
    assert_ne!(
        unsafe { libc::geteuid() },
        0,
        "Linux destructive-authority acceptance requires the unprivileged product boundary"
    );

    let temp = tempfile::tempdir().expect("temp root");
    let project = temp.path().join("project");
    let target = project.join("target");
    let artifact = target.join("reviewed-artifact.bin");

    std::fs::create_dir_all(&target).expect("target");
    std::fs::write(
        project.join("Cargo.toml"),
        "[package]\nname=\"linux-pre-mutation-fail-closed\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )
    .expect("manifest");
    std::fs::write(&artifact, b"reviewed-artifact-must-survive").expect("artifact");

    let reviewed_metadata = std::fs::symlink_metadata(&artifact).expect("reviewed metadata");
    let reviewed_inode = reviewed_metadata.ino();
    let reviewed_content = std::fs::read(&artifact).expect("reviewed content");

    let fake_cargo = project.join("fake-cargo");
    std::fs::write(&fake_cargo, "#!/bin/sh\nexit 99\n").expect("fake cargo");
    let mut cargo_permissions = std::fs::metadata(&fake_cargo)
        .expect("fake cargo metadata")
        .permissions();
    cargo_permissions.set_mode(0o755);
    std::fs::set_permissions(&fake_cargo, cargo_permissions).expect("fake cargo executable");

    // #170's accepted Linux threat model does not permit reaching a pathname-based
    // final mutation boundary until an exact-object primitive is proven. The hook is
    // therefore a sentinel only: secure production code must fail before it can fire.
    let final_unlink_boundary_reached = Arc::new(AtomicBool::new(false));
    let hook_reached = Arc::clone(&final_unlink_boundary_reached);
    let _hook_guard = unix_capability_cleanup::install_before_final_unlink_hook_for_test(
        b"reviewed-artifact.bin",
        move || {
            hook_reached.store(true, Ordering::SeqCst);
        },
    );

    let outcome = cargo_target_reclaim::clean_cargo_target_with_active_use(
        &project,
        &target,
        &fake_cargo,
        |_| Ok(()),
    );

    let error = outcome.expect_err(
        "Linux reclaim must fail closed before mutation until final-object authority is proven",
    );
    assert_eq!(
        error.to_string(),
        "cargo-target-linux-final-object-authority-unproven",
        "Linux fail-closed must expose the stable final-object-authority reason"
    );
    assert!(
        !final_unlink_boundary_reached.load(Ordering::SeqCst),
        "Linux cleanup reached the pathname-based final unlink boundary"
    );

    let after = std::fs::symlink_metadata(&artifact)
        .expect("reviewed artifact must remain after pre-mutation fail-closed");
    assert_eq!(
        after.ino(), reviewed_inode,
        "the exact reviewed artifact identity changed before fail-closed"
    );
    assert_eq!(
        std::fs::read(&artifact).expect("artifact content after fail-closed"),
        reviewed_content,
        "the reviewed artifact content changed before fail-closed"
    );
}
