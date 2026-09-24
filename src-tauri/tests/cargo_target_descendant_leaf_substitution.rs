#![cfg(target_os = "linux")]

#[path = "../src/unix_capability_cleanup.rs"]
mod unix_capability_cleanup;
#[path = "../src/unix_holder_authority.rs"]
mod unix_holder_authority;
#[path = "../src/cargo_target_reclaim.rs"]
mod cargo_target_reclaim;

use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

#[derive(Clone, Copy)]
enum LeafKind {
    Regular,
    Symlink,
}

struct CaseObservation {
    hook_ran: bool,
    reviewed_stash_survived: bool,
    replacement_survived: bool,
    owner_failed_closed: bool,
}

fn run_substitution_case(kind: LeafKind, leaf_name: &str) -> CaseObservation {
    let temp = tempfile::tempdir().expect("temp root");
    let project = temp.path().join("project");
    let target = project.join("target");
    let reviewed_leaf = target.join(leaf_name);
    let reviewed_stash = project.join(format!("{leaf_name}-stash"));
    let reviewed_symlink_target = project.join("reviewed-symlink-target");
    let replacement_symlink_target = project.join("replacement-symlink-target");

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
            std::fs::write(&reviewed_symlink_target, b"reviewed-symlink-target")
                .expect("reviewed symlink target");
            std::fs::write(&replacement_symlink_target, b"replacement-symlink-target")
                .expect("replacement symlink target");
            symlink(&reviewed_symlink_target, &reviewed_leaf).expect("reviewed symlink");
        }
    }

    let fake_cargo = project.join("fake-cargo");
    std::fs::write(&fake_cargo, "#!/bin/sh\nexit 99\n").expect("fake cargo");
    let mut cargo_permissions = std::fs::metadata(&fake_cargo)
        .expect("fake cargo metadata")
        .permissions();
    cargo_permissions.set_mode(0o755);
    std::fs::set_permissions(&fake_cargo, cargo_permissions).expect("fake cargo executable");

    let replacement_inode = Arc::new(AtomicU64::new(0));
    let hook_replacement_inode = Arc::clone(&replacement_inode);
    let hook_leaf = reviewed_leaf.clone();
    let hook_stash = reviewed_stash.clone();
    let hook_replacement_symlink_target = replacement_symlink_target.clone();
    let _hook_guard = unix_capability_cleanup::install_before_final_unlink_hook_for_test(
        leaf_name.as_bytes(),
        move || {
            std::fs::rename(&hook_leaf, &hook_stash)
                .expect("move the reviewed leaf before its final unlink");
            match kind {
                LeafKind::Regular => {
                    std::fs::write(&hook_leaf, b"unreviewed-replacement")
                        .expect("insert same-name replacement file");
                }
                LeafKind::Symlink => {
                    symlink(&hook_replacement_symlink_target, &hook_leaf)
                        .expect("insert same-name replacement symlink");
                }
            }
            hook_replacement_inode.store(
                std::fs::symlink_metadata(&hook_leaf)
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

    let expected_replacement_inode = replacement_inode.load(Ordering::SeqCst);
    let replacement_survived = expected_replacement_inode != 0
        && std::fs::symlink_metadata(&reviewed_leaf)
            .is_ok_and(|metadata| metadata.ino() == expected_replacement_inode);
    let reviewed_stash_survived = match kind {
        LeafKind::Regular => reviewed_stash.is_file(),
        LeafKind::Symlink => std::fs::symlink_metadata(&reviewed_stash)
            .is_ok_and(|metadata| metadata.file_type().is_symlink()),
    };

    CaseObservation {
        hook_ran: expected_replacement_inode != 0,
        reviewed_stash_survived,
        replacement_survived,
        owner_failed_closed: outcome.is_err(),
    }
}

#[test]
fn production_cleanup_never_deletes_leaf_replacements_inserted_after_review() {
    assert_ne!(
        unsafe { libc::geteuid() },
        0,
        "substitution acceptance requires the unprivileged product execution boundary"
    );

    let regular = run_substitution_case(LeafKind::Regular, "reviewed-regular");
    let symlink = run_substitution_case(LeafKind::Symlink, "reviewed-symlink");

    assert!(regular.hook_ran, "regular-file final-unlink hook did not execute");
    assert!(symlink.hook_ran, "symlink final-unlink hook did not execute");
    assert!(
        regular.reviewed_stash_survived && symlink.reviewed_stash_survived,
        "the reviewed leaf objects must remain distinguishable after substitution"
    );
    assert!(
        regular.replacement_survived,
        "cleanup deleted an unreviewed same-name regular-file replacement"
    );
    assert!(
        symlink.replacement_survived,
        "cleanup deleted an unreviewed same-name symlink replacement"
    );
    assert!(
        regular.owner_failed_closed && symlink.owner_failed_closed,
        "post-review leaf substitution must fail closed instead of returning cleanup success"
    );
}
