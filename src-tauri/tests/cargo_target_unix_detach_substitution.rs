#![cfg(unix)]

use std::fs::{self, File};
use std::os::unix::fs::MetadataExt;

#[test]
fn pathname_selected_detach_can_move_replacement_after_reviewed_handle_open() {
    let root = tempfile::tempdir().expect("temp root");
    let target = root.path().join("target");
    let reviewed_stash = root.path().join("reviewed-object");
    let quarantine = root.path().join(".disksage-cargo-clean-detach-race");
    let clean_path = quarantine.join("target");

    fs::create_dir(&target).expect("reviewed target");
    fs::write(target.join("reviewed-artifact"), b"reviewed").expect("reviewed artifact");
    let reviewed_handle = File::open(&target).expect("open reviewed target");
    let reviewed_identity = reviewed_handle.metadata().expect("reviewed metadata");

    // Deterministically interpose a same-user replacement after DiskSage has
    // opened the reviewed object but before the current pathname-selected detach.
    fs::rename(&target, &reviewed_stash).expect("stash reviewed object after open");
    fs::create_dir(&target).expect("replacement target");
    fs::write(target.join("REPLACEMENT_SENTINEL"), b"must-survive")
        .expect("replacement sentinel");
    fs::create_dir(&quarantine).expect("quarantine");

    // This mirrors the current Unix detach mutation. The already-open handle
    // still identifies the reviewed object, but the pathname now selects the
    // unreviewed replacement and moves it into the quarantine.
    fs::rename(&target, &clean_path).expect("simulate pathname-selected detach");

    let opened_after_swap = reviewed_handle.metadata().expect("opened identity after swap");
    let stashed = fs::metadata(&reviewed_stash).expect("stashed reviewed object");
    assert_eq!(opened_after_swap.dev(), reviewed_identity.dev());
    assert_eq!(opened_after_swap.ino(), reviewed_identity.ino());
    assert_eq!(stashed.dev(), reviewed_identity.dev());
    assert_eq!(stashed.ino(), reviewed_identity.ino());
    assert!(reviewed_stash.join("reviewed-artifact").is_file());
    assert_eq!(
        fs::read(clean_path.join("REPLACEMENT_SENTINEL"))
            .expect("replacement became transient detach subject"),
        b"must-survive"
    );

    let owner_source = include_str!("../src/cargo_target_reclaim.rs");
    let unix_detach_start = owner_source
        .find("#[cfg(unix)]\nfn detach_verified_target_dir(")
        .expect("Unix detach boundary");
    let windows_boundary = owner_source[unix_detach_start..]
        .find("#[cfg(windows)]\nstruct DetachedTargetDir")
        .map(|offset| unix_detach_start + offset)
        .expect("Windows boundary after Unix detach");
    let unix_detach = &owner_source[unix_detach_start..windows_boundary];

    assert!(
        !unix_detach.contains("std::fs::rename(target_dir"),
        "P1: Unix detach re-selects the requested target by pathname after opening the reviewed directory capability; a same-path replacement can become a transient mutation subject before post-move dev+ino detection"
    );
}
