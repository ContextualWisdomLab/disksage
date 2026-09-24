#[cfg(unix)]
use std::ffi::CString;
#[cfg(unix)]
use std::fs::File;
#[cfg(unix)]
use std::os::fd::AsRawFd;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

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

    // Real filesystem proof: a reviewed object can be renamed away and the later
    // dirfd+basename unlink can select an unreviewed replacement at the same name.
    std::fs::rename(&child, &reviewed_stash).expect("stash reviewed artifact");
    std::fs::write(&child, b"replacement-must-not-be-selected").expect("replacement artifact");

    let name = CString::new("artifact").expect("child name");
    let rc = unsafe { libc::unlinkat(parent.as_raw_fd(), name.as_ptr(), 0) };
    assert_eq!(rc, 0, "stale name-selected unlink must reproduce the hazard");
    assert!(reviewed_stash.is_file(), "reviewed object remains pinned elsewhere");
    assert!(!child.exists(), "unreviewed replacement was selected by stale child name");
    assert_eq!(std::fs::read(&reviewed_stash).expect("reviewed survives"), b"reviewed");
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

    std::fs::rename(&child, &reviewed_stash).expect("stash reviewed child");
    std::fs::create_dir(&child).expect("replacement child");

    let name = CString::new("child").expect("child name");
    let rc = unsafe { libc::unlinkat(parent.as_raw_fd(), name.as_ptr(), libc::AT_REMOVEDIR) };
    assert_eq!(rc, 0, "stale directory-name unlink must reproduce the hazard");
    assert!(reviewed_stash.is_dir(), "reviewed directory remains pinned elsewhere");
    assert!(!child.exists(), "unreviewed replacement directory was selected by stale name");
}

#[cfg(target_os = "linux")]
#[test]
fn linux_owner_must_refuse_before_reaching_name_selected_descendant_unlink() {
    let owner = include_str!("../src/cargo_target_reclaim.rs");
    let flow_start = owner
        .find("fn clean_cargo_target_with_active_use_and_opened_hook")
        .expect("Cargo-target owner flow");
    let flow_end = owner[flow_start..]
        .find("/// Buyer-visible reclaim credit")
        .map(|offset| flow_start + offset)
        .expect("owner flow boundary");
    let flow = &owner[flow_start..flow_end];

    let holder = flow
        .find("unix_holder_authority::ensure_opened_target_has_no_active_holders")
        .expect("exact-object holder authorization");
    let cutoff = flow
        .find("cargo-target-linux-final-object-authority-unproven")
        .expect("Linux pre-mutation final-object-authority cutoff");
    let mutation = flow
        .find("unix_capability_cleanup::remove_contents")
        .expect("non-Linux Unix mutation path remains explicit");

    assert!(cutoff > holder, "Linux refusal must follow exact-object holder authorization");
    assert!(cutoff < mutation, "Linux refusal must occur before descendant mutation");
}
