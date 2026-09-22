use std::fs;
use std::process::Command;

#[cfg(unix)]
use std::fs::File;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

fn assert_owner_no_longer_delegates_destructive_clean_to_mutable_path() {
    let owner_source = include_str!("../src/cargo_target_reclaim.rs");
    assert!(
        !(owner_source.contains(".arg(\"--target-dir\")")
            && owner_source.contains(".arg(&detached_target.clean_path)")),
        "P1: DiskSage still passes the replaceable detached pathname to an external destructive Cargo child; a post-authorization replacement can become the mutation subject"
    );
}

#[cfg(unix)]
#[test]
fn path_selected_destructive_child_can_mutate_an_unreviewed_replacement() {
    let root = tempfile::tempdir().expect("temp root");
    let quarantine = root.path().join(".disksage-cargo-clean-race");
    let clean_path = quarantine.join("target");
    let reviewed_stash = root.path().join("reviewed-object");
    fs::create_dir_all(&clean_path).expect("reviewed target");
    fs::write(clean_path.join("reviewed-artifact"), b"reviewed").expect("reviewed artifact");

    let reviewed_handle = File::open(&clean_path).expect("open reviewed target");
    let reviewed_identity = reviewed_handle.metadata().expect("reviewed metadata");
    fs::rename(&clean_path, &reviewed_stash).expect("move reviewed object aside");
    fs::create_dir(&clean_path).expect("replacement target");
    fs::write(clean_path.join("REPLACEMENT_SENTINEL"), b"must-survive").expect("replacement sentinel");

    let opened_after_swap = reviewed_handle.metadata().expect("opened identity after swap");
    let stashed = fs::metadata(&reviewed_stash).expect("stashed reviewed object");
    assert_eq!(opened_after_swap.dev(), reviewed_identity.dev());
    assert_eq!(opened_after_swap.ino(), reviewed_identity.ino());
    assert_eq!(stashed.dev(), reviewed_identity.dev());
    assert_eq!(stashed.ino(), reviewed_identity.ino());

    let status = Command::new("/bin/sh")
        .arg("-c")
        .arg("rm -rf -- \"$1\"")
        .arg("disksage-path-selected-child")
        .arg(&clean_path)
        .status()
        .expect("run destructive path-selected child");
    assert!(status.success(), "hazard reproduction child must complete");
    assert!(reviewed_stash.join("reviewed-artifact").is_file());
    assert!(!clean_path.exists());
    assert_owner_no_longer_delegates_destructive_clean_to_mutable_path();
}

#[cfg(windows)]
#[test]
fn windows_path_selected_destructive_child_can_mutate_an_unreviewed_replacement() {
    let root = tempfile::tempdir().expect("temp root");
    let quarantine = root.path().join(".disksage-cargo-clean-race");
    let clean_path = quarantine.join("target");
    let reviewed_stash = root.path().join("reviewed-object");
    fs::create_dir_all(&clean_path).expect("reviewed target");
    fs::write(clean_path.join("reviewed-artifact"), b"reviewed").expect("reviewed artifact");

    fs::rename(&clean_path, &reviewed_stash).expect("move reviewed object aside");
    fs::create_dir(&clean_path).expect("replacement target");
    fs::write(clean_path.join("REPLACEMENT_SENTINEL"), b"must-survive").expect("replacement sentinel");

    let status = Command::new("cmd.exe")
        .arg("/D")
        .arg("/C")
        .arg("rmdir")
        .arg("/S")
        .arg("/Q")
        .arg(&clean_path)
        .status()
        .expect("run destructive path-selected child");
    assert!(status.success(), "hazard reproduction child must complete");
    assert!(reviewed_stash.join("reviewed-artifact").is_file());
    assert!(!clean_path.exists());
    assert_owner_no_longer_delegates_destructive_clean_to_mutable_path();
}
