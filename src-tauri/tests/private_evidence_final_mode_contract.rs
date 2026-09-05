#![cfg(unix)]

#[path = "../src/private_evidence.rs"]
mod private_evidence;

use private_evidence::{
    write_object_bound_bytes_create_new_with_hooks, ObjectBoundPublicationError,
};
use std::fs;
use std::os::unix::fs::PermissionsExt;

fn assert_final_mode_drift_fails_closed(drift_mode: u32) {
    let root = tempfile::tempdir().expect("tempdir");
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700))
        .expect("set private parent mode");
    let record = root.path().join(format!("record-{drift_mode:o}.json"));
    let hook_record = record.clone();

    let error = write_object_bound_bytes_create_new_with_hooks(
        &record,
        b"authorized",
        0o600,
        None,
        || {},
        || {},
        move || {
            fs::set_permissions(&hook_record, fs::Permissions::from_mode(drift_mode))
                .expect("drift final mode");
        },
    )
    .expect_err("final mode drift must fail closed");

    assert_eq!(error, ObjectBoundPublicationError::ModeInvalid);
    let metadata = fs::metadata(&record).expect("record metadata");
    assert_eq!(
        metadata.len(),
        0,
        "post-create failure must invalidate only the exact opened record"
    );
    assert_eq!(
        metadata.permissions().mode() & 0o7777,
        0o600,
        "invalidated tombstone must return to the requested private mode"
    );
}

#[test]
fn final_private_evidence_mode_widening_or_special_bits_fail_closed() {
    for drift_mode in [0o644, 0o1600, 0o2600, 0o4600] {
        assert_final_mode_drift_fails_closed(drift_mode);
    }
}
