use std::fs;
use std::process::Command;

#[cfg(unix)]
use std::fs::File;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

fn unix_owner_mutation_slice() -> &'static str {
    let owner_source = include_str!("../src/cargo_target_reclaim.rs");
    let flow_start = owner_source
        .find("fn clean_cargo_target_with_active_use_and_opened_hook")
        .expect("cargo target owner flow");
    let flow_end = owner_source[flow_start..]
        .find("/// Buyer-visible reclaim credit")
        .map(|offset| flow_start + offset)
        .expect("ledger boundary after cargo target owner flow");
    let flow = &owner_source[flow_start..flow_end];
    let holder = flow
        .find("unix_holder_authority::ensure_opened_target_has_no_active_holders")
        .expect("Unix exact-object holder authorization");
    let windows = flow[holder..]
        .find("#[cfg(windows)]")
        .map(|offset| holder + offset)
        .expect("Windows branch after Unix retained-capability branch");
    &flow[holder..windows]
}

fn assert_unix_owner_no_longer_delegates_destructive_clean_to_mutable_path() {
    let unix_mutation = unix_owner_mutation_slice();
    assert!(
        !unix_mutation.contains("--target-dir") && !unix_mutation.contains("Command::new(cargo)"),
        "P1: Unix DiskSage still delegates destructive cleanup to an external child selected by a mutable pathname"
    );
    assert!(
        unix_mutation.contains("unix_capability_cleanup::measure_allocated_bytes(&opened_target.file)")
            && unix_mutation.contains("unix_capability_cleanup::remove_contents(&opened_target.file)"),
        "P1: Unix cleanup must keep measurement and deletion on the retained reviewed filesystem capability"
    );
}

fn assert_unix_owner_keeps_recovery_and_receipts_on_the_reviewed_object() {
    let unix_mutation = unix_owner_mutation_slice();
    assert!(
        !unix_mutation.contains("detach_verified_target_dir")
            && !unix_mutation.contains("bounded_dir_size")
            && !unix_mutation.contains("std::fs::rename"),
        "P1: Unix reclaim still re-selects mutable pathname state after exact-object authorization"
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
    assert_unix_owner_no_longer_delegates_destructive_clean_to_mutable_path();
}

#[cfg(unix)]
#[test]
fn path_checked_rollback_can_move_an_unreviewed_replacement() {
    let root = tempfile::tempdir().expect("temp root");
    let quarantine = root.path().join(".disksage-cargo-clean-rollback-race");
    let clean_path = quarantine.join("target");
    let original_path = root.path().join("target");
    let reviewed_stash = root.path().join("reviewed-object");
    fs::create_dir_all(&clean_path).expect("reviewed target");
    fs::write(clean_path.join("reviewed-artifact"), b"reviewed").expect("reviewed artifact");

    let reviewed_handle = File::open(&clean_path).expect("open reviewed target");
    let reviewed_identity = reviewed_handle.metadata().expect("reviewed metadata");
    let checked = fs::metadata(&clean_path).expect("pre-rollback metadata");
    assert_eq!(checked.dev(), reviewed_identity.dev());
    assert_eq!(checked.ino(), reviewed_identity.ino());

    // Reproduce the stale path-authority hazard independently of production code.
    fs::rename(&clean_path, &reviewed_stash).expect("stash reviewed object after check");
    fs::create_dir(&clean_path).expect("replacement target");
    fs::write(clean_path.join("REPLACEMENT_SENTINEL"), b"must-survive").expect("replacement sentinel");

    fs::rename(&clean_path, &original_path).expect("simulate stale path-selected rollback");
    assert!(reviewed_stash.join("reviewed-artifact").is_file());
    assert_eq!(
        fs::read(original_path.join("REPLACEMENT_SENTINEL")).expect("replacement survives stale rollback"),
        b"must-survive"
    );
    assert_unix_owner_keeps_recovery_and_receipts_on_the_reviewed_object();
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
}
