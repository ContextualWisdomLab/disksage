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
