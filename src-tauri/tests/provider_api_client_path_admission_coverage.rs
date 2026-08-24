//! Credential-free path-admission coverage for provider metadata locators.
//!
//! These regressions prove that local cloud destinations cannot smuggle parent traversal,
//! control bytes, oversized provider paths, or non-Unicode host bytes into fixed-host metadata
//! requests. No provider network call or credential is required.

use disksage_lib::provider_api_client::{google_drive_path_locator, onedrive_path_locator};

#[test]
fn lexical_parent_traversal_and_control_bytes_fail_closed() {
    let root = tempfile::tempdir().unwrap();

    let traversal = root.path().join("folder").join("..").join("secret.txt");
    assert_eq!(
        onedrive_path_locator(root.path(), &traversal).unwrap_err(),
        "provider-path-invalid"
    );

    let control = root.path().join("folder\nname").join("file.txt");
    assert_eq!(
        google_drive_path_locator(root.path(), &control, "file-id").unwrap_err(),
        "provider-path-invalid"
    );
}

#[test]
fn provider_relative_path_length_is_bounded_before_request_construction() {
    let root = tempfile::tempdir().unwrap();
    let destination = root.path().join("x".repeat(4_097));

    assert_eq!(
        onedrive_path_locator(root.path(), &destination).unwrap_err(),
        "provider-path-invalid"
    );
}

#[cfg(unix)]
#[test]
fn non_utf8_destination_segment_fails_closed_before_request_construction() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let root = tempfile::tempdir().unwrap();
    let destination = root
        .path()
        .join(OsString::from_vec(vec![b'f', b'i', b'l', b'e', 0xff]));

    assert_eq!(
        google_drive_path_locator(root.path(), &destination, "file-id").unwrap_err(),
        "provider-path-not-unicode"
    );
}
