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
fn linux_owner_refuses_before_descendant_final_disposition_can_be_selected_by_name() {
    assert_ne!(
        unsafe { libc::geteuid() },
        0,
        "Linux destructive-authority acceptance requires the unprivileged product boundary"
    );

    let temp = tempfile::tempdir().expect("temp root");
    let project = temp.path().join("project");
    let target = project.join("target");
    let reviewed_child = target.join("reviewed-child");
    let artifact = reviewed_child.join("artifact.bin");
    std::fs::create_dir_all(&reviewed_child).expect("reviewed child");
    std::fs::write(&artifact, b"reviewed-artifact").expect("artifact");
    std::fs::write(
        project.join("Cargo.toml"),
        "[package]\nname=\"descendant-substitution\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )
    .expect("manifest");

    let fake_cargo = project.join("fake-cargo");
    std::fs::write(&fake_cargo, "#!/bin/sh\nexit 99\n").expect("fake cargo");
    let mut cargo_permissions = std::fs::metadata(&fake_cargo)
        .expect("fake cargo metadata")
        .permissions();
    cargo_permissions.set_mode(0o755);
    std::fs::set_permissions(&fake_cargo, cargo_permissions).expect("fake cargo executable");

    let reviewed_child_inode = std::fs::symlink_metadata(&reviewed_child)
        .expect("reviewed child metadata")
        .ino();
    let reviewed_artifact_inode = std::fs::symlink_metadata(&artifact)
        .expect("reviewed artifact metadata")
        .ino();
    let reviewed_artifact_content = std::fs::read(&artifact).expect("reviewed artifact content");

    // This is the exact seam where the earlier exploit fixture could rename the already-open
    // child and insert an unreviewed same-name replacement. Under #170's Linux contract the
    // Cargo-target owner must never reach this name-selected mutation boundary.
    let final_unlink_boundary_reached = Arc::new(AtomicBool::new(false));
    let hook_reached = Arc::clone(&final_unlink_boundary_reached);
    let _hook_guard = unix_capability_cleanup::install_before_final_unlink_hook_for_test(
        b"reviewed-child",
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
        "Linux Cargo-target reclaim must refuse before descendant final disposition",
    );
    assert_eq!(
        error.to_string(),
        "cargo-target-linux-final-object-authority-unproven"
    );
    assert!(
        !final_unlink_boundary_reached.load(Ordering::SeqCst),
        "Linux owner reached descendant final unlink despite unproven final-object authority"
    );
    assert_eq!(
        std::fs::symlink_metadata(&reviewed_child)
            .expect("reviewed child survives")
            .ino(),
        reviewed_child_inode,
        "reviewed child identity changed before fail-closed"
    );
    assert_eq!(
        std::fs::symlink_metadata(&artifact)
            .expect("reviewed artifact survives")
            .ino(),
        reviewed_artifact_inode,
        "reviewed artifact identity changed before fail-closed"
    );
    assert_eq!(
        std::fs::read(&artifact).expect("reviewed artifact content after refusal"),
        reviewed_artifact_content,
        "reviewed artifact content changed before fail-closed"
    );
}
