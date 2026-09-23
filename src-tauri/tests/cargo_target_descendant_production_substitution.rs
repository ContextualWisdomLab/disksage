#![cfg(target_os = "linux")]

#[path = "../src/unix_capability_cleanup.rs"]
mod unix_capability_cleanup;
#[path = "../src/unix_holder_authority.rs"]
mod unix_holder_authority;
#[path = "../src/cargo_target_reclaim.rs"]
mod cargo_target_reclaim;

use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

#[test]
fn production_cleanup_never_deletes_a_replacement_inserted_after_descendant_open() {
    assert_ne!(
        unsafe { libc::geteuid() },
        0,
        "substitution acceptance requires the unprivileged product execution boundary"
    );

    let temp = tempfile::tempdir().expect("temp root");
    let project = temp.path().join("project");
    let target = project.join("target");
    let reviewed_child = target.join("reviewed-child");
    let reviewed_stash = project.join("reviewed-child-stash");
    std::fs::create_dir_all(&reviewed_child).expect("reviewed child");
    std::fs::write(reviewed_child.join("artifact.bin"), b"reviewed-artifact")
        .expect("artifact");
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

    // The test-only scheduler chooses the exact interleaving; the namespace mutation
    // itself is a real rename + replacement creation on the product filesystem path.
    // The artifact is already unlinked when this hook runs, so a secure owner must
    // report typed partial evidence rather than deleting the unreviewed replacement.
    let replacement_inode = Arc::new(AtomicU64::new(0));
    let hook_replacement_inode = Arc::clone(&replacement_inode);
    let hook_child = reviewed_child.clone();
    let hook_stash = reviewed_stash.clone();
    let _hook_guard = unix_capability_cleanup::install_before_final_unlink_hook_for_test(
        b"reviewed-child",
        move || {
            std::fs::rename(&hook_child, &hook_stash)
                .expect("move the already-open reviewed child out of the target namespace");
            std::fs::create_dir(&hook_child).expect("insert same-name replacement directory");
            hook_replacement_inode.store(
                std::fs::symlink_metadata(&hook_child)
                    .expect("replacement metadata")
                    .ino(),
                Ordering::SeqCst,
            );
        },
    );

    let outcome = cargo_target_reclaim::clean_cargo_target_with_active_use(
        &project,
        &target,
        &fake_cargo,
        |_| Ok(()),
    );
    let replacement_inode = replacement_inode.load(Ordering::SeqCst);
    assert_ne!(
        replacement_inode, 0,
        "the deterministic final-unlink hook must execute on the reviewed child"
    );

    assert!(
        reviewed_stash.is_dir(),
        "the originally reviewed directory capability must remain distinguishable after substitution"
    );
    assert!(
        reviewed_child.is_dir(),
        "cleanup must fail closed instead of deleting an unreviewed same-name replacement"
    );
    assert_eq!(
        std::fs::symlink_metadata(&reviewed_child)
            .expect("surviving replacement metadata")
            .ino(),
        replacement_inode,
        "the exact replacement inserted after descendant review must survive"
    );

    let failure = outcome.expect_err("post-open descendant substitution must fail closed");
    let cargo_target_reclaim::CargoTargetReclaimError::PartialCleanup(receipt) = failure else {
        panic!("deletion before substitution means the owner must retain typed partial evidence");
    };
    assert!(receipt.entries_removed() > 0);
    assert_eq!(receipt.ledger_reclaim_bytes(), 0);
}
