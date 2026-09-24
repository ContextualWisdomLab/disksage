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

    // Reproduce why pathname detach is not an admissible Unix mutation boundary:
    // a same-user replacement can occupy the reviewed path after the descriptor is open.
    fs::rename(&target, &reviewed_stash).expect("stash reviewed object after open");
    fs::create_dir(&target).expect("replacement target");
    fs::write(target.join("REPLACEMENT_SENTINEL"), b"must-survive")
        .expect("replacement sentinel");
    fs::create_dir(&quarantine).expect("quarantine");
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
    let flow_start = owner_source
        .find("fn clean_cargo_target_with_active_use_and_opened_hook")
        .expect("cargo target owner flow");
    let flow_end = owner_source[flow_start..]
        .find("/// Buyer-visible reclaim credit")
        .map(|offset| flow_start + offset)
        .expect("ledger boundary after owner flow");
    let flow = &owner_source[flow_start..flow_end];
    let holder = flow
        .find("unix_holder_authority::ensure_opened_target_has_no_active_holders")
        .expect("Unix exact-object holder authorization");
    let windows = flow[holder..]
        .find("#[cfg(windows)]")
        .map(|offset| holder + offset)
        .expect("Windows branch after Unix retained-capability branch");
    let unix_mutation = &flow[holder..windows];

    assert!(
        unix_mutation.contains("unix_capability_cleanup::measure_allocated_bytes(&opened_target.file)"),
        "Unix reclaim must measure through the retained reviewed descriptor"
    );
    assert!(
        unix_mutation.contains("unix_capability_cleanup::remove_contents(&opened_target.file)"),
        "Unix reclaim must remove descendants through the retained reviewed descriptor"
    );
    assert!(
        !unix_mutation.contains("detach_verified_target_dir"),
        "P1: Unix owner flow must not re-select the requested target by pathname for detach after opening the reviewed capability"
    );
    assert!(
        !unix_mutation.contains("Command::new(cargo)"),
        "P1: Unix destructive authority must not be delegated to an external child selected by a mutable pathname"
    );
}
