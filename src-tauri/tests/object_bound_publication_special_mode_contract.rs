//! Behavioral contract for requested private-record modes while replacement is fail closed.
//!
//! Replacement currently performs no staging or publication. Mode admission must still reject
//! setuid/setgid/sticky requests before the source-identity capability boundary, and an admitted
//! private mode must continue to fail closed without touching the filesystem.

#![cfg(unix)]

#[path = "../src/object_bound_publication.rs"]
mod object_bound_publication;

use object_bound_publication::{replace_object_bound_bytes, ObjectBoundReplaceError};

#[test]
fn replacement_rejects_special_bits_before_source_identity_capability_evaluation() {
    let root = tempfile::tempdir().expect("tempdir");
    let record = root.path().join("connections.json");

    for invalid_mode in [0o1600, 0o2600, 0o4600] {
        let error = replace_object_bound_bytes(&record, b"private", invalid_mode)
            .expect_err("special-bit mode must fail before replacement capability evaluation");
        assert_eq!(error, ObjectBoundReplaceError::ModeInvalid);
        assert!(!record.exists(), "invalid mode must not create a record");
        assert_eq!(
            std::fs::read_dir(root.path()).expect("read root").count(),
            0,
            "invalid mode must not create staging names"
        );
    }

    let error = replace_object_bound_bytes(&record, b"private", 0o600)
        .expect_err("admitted mode must still fail while exact-source replacement is unavailable");
    assert_eq!(error, ObjectBoundReplaceError::SourceIdentityUnavailable);
    assert!(!record.exists());
}
