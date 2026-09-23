#![cfg(target_os = "linux")]

#[path = "../src/unix_capability_cleanup.rs"]
mod unix_capability_cleanup;
#[path = "../src/unix_holder_authority.rs"]
mod unix_holder_authority;
#[path = "../src/cargo_target_reclaim.rs"]
mod cargo_target_reclaim;

use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::thread;

const ARTIFACT_COUNT: usize = 8_192;
const ATTACK_TIMEOUT_MS: libc::c_int = 15_000;

fn arm_delete_observer(path: &std::path::Path) -> libc::c_int {
    let fd = unsafe { libc::inotify_init1(libc::IN_CLOEXEC) };
    assert!(fd >= 0, "inotify_init1 failed: {}", std::io::Error::last_os_error());
    let path = CString::new(path.as_os_str().as_bytes()).expect("watch path");
    let watch = unsafe { libc::inotify_add_watch(fd, path.as_ptr(), libc::IN_DELETE) };
    assert!(
        watch >= 0,
        "inotify_add_watch failed: {}",
        std::io::Error::last_os_error()
    );
    fd
}

fn wait_for_first_delete(fd: libc::c_int) {
    let mut descriptor = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    let ready = unsafe { libc::poll(&mut descriptor, 1, ATTACK_TIMEOUT_MS) };
    assert!(ready > 0, "cleanup did not begin descendant deletion within timeout");

    let mut buffer = [0u8; 4096];
    let read = unsafe {
        libc::read(
            fd,
            buffer.as_mut_ptr().cast::<libc::c_void>(),
            buffer.len(),
        )
    };
    assert!(read > 0, "failed to consume inotify delete event");
}

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
    std::fs::write(
        project.join("Cargo.toml"),
        "[package]\nname=\"descendant-substitution\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )
    .expect("manifest");

    // The watcher is armed before cleanup. Once the production walker has
    // irreversibly deleted its first reviewed descendant, thousands of entries
    // remain behind the retained child fd, giving the attacker a deterministic
    // interval before the current name-based final directory unlink.
    for index in 0..ARTIFACT_COUNT {
        std::fs::write(
            reviewed_child.join(format!("artifact-{index:05}.bin")),
            b"reviewed-artifact",
        )
        .expect("artifact");
    }

    let fake_cargo = project.join("fake-cargo");
    std::fs::write(&fake_cargo, "#!/bin/sh\nexit 99\n").expect("fake cargo");
    let mut cargo_permissions = std::fs::metadata(&fake_cargo)
        .expect("fake cargo metadata")
        .permissions();
    cargo_permissions.set_mode(0o755);
    std::fs::set_permissions(&fake_cargo, cargo_permissions).expect("fake cargo executable");

    let observer = arm_delete_observer(&reviewed_child);
    let attacker_child = reviewed_child.clone();
    let attacker_stash = reviewed_stash.clone();
    let attacker = thread::spawn(move || {
        wait_for_first_delete(observer);
        std::fs::rename(&attacker_child, &attacker_stash)
            .expect("move the already-open reviewed child out of the target namespace");
        std::fs::create_dir(&attacker_child).expect("insert same-name replacement directory");
        let replacement_inode = std::fs::symlink_metadata(&attacker_child)
            .expect("replacement metadata")
            .ino();
        unsafe {
            libc::close(observer);
        }
        replacement_inode
    });

    let outcome = cargo_target_reclaim::clean_cargo_target_with_active_use(
        &project,
        &target,
        &fake_cargo,
        |_| Ok(()),
    );
    let replacement_inode = attacker.join().expect("attacker thread");

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
