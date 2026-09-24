#![cfg(target_os = "linux")]

#[path = "../src/unix_capability_cleanup.rs"]
mod unix_capability_cleanup;
#[path = "../src/unix_holder_authority.rs"]
mod unix_holder_authority;
#[path = "../src/cargo_target_reclaim.rs"]
mod cargo_target_reclaim;

use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

#[derive(Clone, Copy)]
enum LeafKind {
    Regular,
    Symlink,
}

fn run_pre_mutation_case(kind: LeafKind, leaf_name: &str) {
    let temp = tempfile::tempdir().expect("temp root");
    let project = temp.path().join("project");
    let target = project.join("target");
    let reviewed_leaf = target.join(leaf_name);
    let symlink_target = project.join("reviewed-symlink-target");

    std::fs::create_dir_all(&target).expect("target");
    std::fs::write(
        project.join("Cargo.toml"),
        "[package]\nname=\"leaf-substitution\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )
    .expect("manifest");

    match kind {
        LeafKind::Regular => {
            std::fs::write(&reviewed_leaf, b"reviewed-regular-file").expect("reviewed file");
        }
        LeafKind::Symlink => {
            std::fs::write(&symlink_target, b"reviewed-symlink-target")
                .expect("reviewed symlink target");
            symlink(&symlink_target, &reviewed_leaf).expect("reviewed symlink");
        }
    }

    let fake_cargo = project.join("fake-cargo");
    std::fs::write(&fake_cargo, "#!/bin/sh\nexit 99\n").expect("fake cargo");
    let mut cargo_permissions = std::fs::metadata(&fake_cargo)
        .expect("fake cargo metadata")
        .permissions();
    cargo_permissions.set_mode(0o755);
    std::fs::set_permissions(&fake_cargo, cargo_permissions).expect("fake cargo executable");

    let before = std::fs::symlink_metadata(&reviewed_leaf).expect("reviewed leaf metadata");
    let reviewed_inode = before.ino();
    let reviewed_regular_content = matches!(kind, LeafKind::Regular)
        .then(|| std::fs::read(&reviewed_leaf).expect("reviewed regular content"));

    // The old exploit fixture performed the namespace substitution here. Under the accepted
    // Linux contract this seam is a must-not-reach sentinel: the owner refuses before any
    // name-selected final unlink can make an unreviewed replacement the mutation subject.
    let final_unlink_boundary_reached = Arc::new(AtomicBool::new(false));
    let hook_reached = Arc::clone(&final_unlink_boundary_reached);
    let _hook_guard = unix_capability_cleanup::install_before_final_unlink_hook_for_test(
        leaf_name.as_bytes(),
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

    let error = outcome.expect_err("Linux leaf cleanup must refuse before final unlink");
    assert_eq!(
        error.to_string(),
        "cargo-target-linux-final-object-authority-unproven"
    );
    assert!(
        !final_unlink_boundary_reached.load(Ordering::SeqCst),
        "Linux owner reached the final-unlink seam for {leaf_name}"
    );

    let after = std::fs::symlink_metadata(&reviewed_leaf).expect("reviewed leaf survives");
    assert_eq!(after.ino(), reviewed_inode, "reviewed leaf identity changed");
    match kind {
        LeafKind::Regular => assert_eq!(
            std::fs::read(&reviewed_leaf).expect("reviewed regular content after refusal"),
            reviewed_regular_content.expect("regular fixture content"),
        ),
        LeafKind::Symlink => {
            assert!(after.file_type().is_symlink(), "reviewed symlink type changed");
            assert_eq!(
                std::fs::read_link(&reviewed_leaf).expect("reviewed symlink target after refusal"),
                symlink_target,
            );
        }
    }
}

#[test]
fn linux_owner_refuses_before_regular_and_symlink_final_disposition() {
    assert_ne!(
        unsafe { libc::geteuid() },
        0,
        "Linux destructive-authority acceptance requires the unprivileged product boundary"
    );

    run_pre_mutation_case(LeafKind::Regular, "reviewed-regular");
    run_pre_mutation_case(LeafKind::Symlink, "reviewed-symlink");
}
