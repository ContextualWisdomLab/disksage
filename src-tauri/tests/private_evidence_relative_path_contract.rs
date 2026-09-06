#![cfg(unix)]

#[path = "../src/private_evidence.rs"]
mod private_evidence_core;
#[path = "../src/private_directory_publication.rs"]
mod private_directory_publication;
#[path = "../src/private_evidence_publication.rs"]
mod private_evidence;

use private_evidence::{write_object_bound_bytes_create_new, ObjectBoundPublicationError};
use std::path::Path;

#[test]
fn no_policy_relative_destination_is_rejected_as_invalid_name_authority() {
    let error = write_object_bound_bytes_create_new(
        Path::new("relative/private-evidence.json"),
        b"private evidence",
        0o600,
        None,
    )
    .expect_err("relative destination authority must fail closed before filesystem lookup");

    assert_eq!(error, ObjectBoundPublicationError::NameInvalid);
}

#[test]
fn forbidden_root_relative_destination_is_rejected_before_hooks_or_lookup() {
    use std::cell::Cell;

    let hook_calls = Cell::new(0_u8);
    let error = private_evidence_core::write_object_bound_bytes_create_new_with_hooks(
        Path::new("relative/private-evidence.json"),
        b"private evidence",
        0o600,
        Some(Path::new("relative/source-root")),
        || hook_calls.set(hook_calls.get() + 1),
        || hook_calls.set(hook_calls.get() + 1),
        || hook_calls.set(hook_calls.get() + 1),
    )
    .expect_err("relative destination authority must fail before hooks or filesystem lookup");

    assert_eq!(error, ObjectBoundPublicationError::NameInvalid);
    assert_eq!(hook_calls.get(), 0);
}

#[test]
fn forbidden_root_trailing_separator_destination_is_rejected_before_hooks_or_lookup() {
    use std::cell::Cell;
    use std::path::PathBuf;

    let destination = tempfile::tempdir().unwrap();
    let forbidden = tempfile::tempdir().unwrap();
    let target = destination.path().join("private-evidence.json");
    let trailing = PathBuf::from(format!("{}/", target.display()));
    let hook_calls = Cell::new(0_u8);

    let error = private_evidence_core::write_object_bound_bytes_create_new_with_hooks(
        &trailing,
        b"private evidence",
        0o600,
        Some(forbidden.path()),
        || hook_calls.set(hook_calls.get() + 1),
        || hook_calls.set(hook_calls.get() + 1),
        || hook_calls.set(hook_calls.get() + 1),
    )
    .expect_err("directory-looking destination authority must fail before hooks or filesystem lookup");

    assert_eq!(error, ObjectBoundPublicationError::NameInvalid);
    assert_eq!(hook_calls.get(), 0);
    assert!(!target.exists(), "trailing separator must not normalize into a file mutation");
}
