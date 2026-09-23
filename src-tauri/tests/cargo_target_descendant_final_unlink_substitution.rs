#[cfg(unix)]
use std::ffi::CString;
#[cfg(unix)]
use std::fs::File;
#[cfg(unix)]
use std::os::fd::AsRawFd;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

fn unix_walk_slice() -> &'static str {
    let source = include_str!("../src/unix_capability_cleanup.rs");
    let start = source
        .find("fn walk_directory")
        .expect("Unix capability walk");
    let end = source[start..]
        .find("fn root_device")
        .map(|offset| start + offset)
        .expect("root-device boundary after Unix capability walk");
    &source[start..end]
}

#[cfg(unix)]
#[test]
fn stale_leaf_name_unlink_can_delete_an_unreviewed_replacement() {
    let root = tempfile::tempdir().expect("temp root");
    let target = root.path().join("target");
    std::fs::create_dir(&target).expect("target");
    let child = target.join("artifact");
    let reviewed_stash = target.join("reviewed-artifact-stash");
    std::fs::write(&child, b"reviewed").expect("reviewed artifact");

    let parent = File::open(&target).expect("open target");
    let reviewed = File::open(&child).expect("open reviewed artifact");
    let expected = reviewed.metadata().expect("reviewed metadata");
    let checked = std::fs::symlink_metadata(&child).expect("checked metadata");
    assert_eq!((checked.dev(), checked.ino()), (expected.dev(), expected.ino()));

    // Reproduce the current stat/open -> final unlinkat(name) authority gap.
    std::fs::rename(&child, &reviewed_stash).expect("stash reviewed artifact");
    std::fs::write(&child, b"replacement-must-not-be-selected").expect("replacement artifact");

    let name = CString::new("artifact").expect("child name");
    let rc = unsafe { libc::unlinkat(parent.as_raw_fd(), name.as_ptr(), 0) };
    assert_eq!(rc, 0, "stale name-selected unlink must reproduce the hazard");
    assert!(reviewed_stash.is_file(), "reviewed object remains pinned elsewhere");
    assert!(!child.exists(), "unreviewed replacement was selected by stale child name");
    assert_eq!(std::fs::read(&reviewed_stash).expect("reviewed survives"), b"reviewed");

    let walk = unix_walk_slice();
    assert!(
        !walk.contains("unlink_at(dir_fd, &name, 0)"),
        "P1: regular-file final disposition still re-selects the mutable readdir name after review"
    );
}

#[cfg(unix)]
#[test]
fn stale_directory_name_unlink_can_remove_an_unreviewed_replacement() {
    let root = tempfile::tempdir().expect("temp root");
    let target = root.path().join("target");
    let child = target.join("child");
    let reviewed_stash = target.join("reviewed-child-stash");
    std::fs::create_dir_all(&child).expect("reviewed child");

    let parent = File::open(&target).expect("open target");
    let reviewed = File::open(&child).expect("open reviewed child");
    let expected = reviewed.metadata().expect("reviewed metadata");
    let checked = std::fs::symlink_metadata(&child).expect("checked metadata");
    assert_eq!((checked.dev(), checked.ino()), (expected.dev(), expected.ino()));

    // The reviewed directory remains reachable by its retained handle after the name is swapped.
    std::fs::rename(&child, &reviewed_stash).expect("stash reviewed child");
    std::fs::create_dir(&child).expect("replacement child");

    let name = CString::new("child").expect("child name");
    let rc = unsafe { libc::unlinkat(parent.as_raw_fd(), name.as_ptr(), libc::AT_REMOVEDIR) };
    assert_eq!(rc, 0, "stale directory-name unlink must reproduce the hazard");
    assert!(reviewed_stash.is_dir(), "reviewed directory remains pinned elsewhere");
    assert!(!child.exists(), "unreviewed replacement directory was selected by stale name");

    let walk = unix_walk_slice();
    assert!(
        !walk.contains("unlink_at(dir_fd, &name, libc::AT_REMOVEDIR)"),
        "P1: directory final disposition still re-selects the mutable readdir name after review"
    );
}
