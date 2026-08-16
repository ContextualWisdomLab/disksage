//! Credential-free provider OAuth client-ID validation coverage.
//!
//! The tests exercise only deterministic production validators and requested-scope selection. No
//! browser, loopback listener, credential store, token exchange, or network request is started.

use disksage_lib::cloud::CloudProvider;
use disksage_lib::provider_oauth::{requested_scope, validate_client_id};

#[test]
fn requested_scope_is_provider_specific_and_icloud_fails_closed() {
    assert_eq!(
        requested_scope(CloudProvider::Onedrive).unwrap(),
        "Files.Read offline_access"
    );
    assert_eq!(
        requested_scope(CloudProvider::GoogleDrive).unwrap(),
        "https://www.googleapis.com/auth/drive.metadata.readonly"
    );
    assert_eq!(
        requested_scope(CloudProvider::Icloud).unwrap_err(),
        "icloud-oauth-not-supported"
    );
}

#[test]
fn onedrive_client_id_requires_exact_guid_shape() {
    assert!(validate_client_id(
        CloudProvider::Onedrive,
        "01234567-89ab-cdef-0123-456789abcdef"
    )
    .is_ok());

    for invalid in [
        "",
        "01234567-89ab-cdef-0123-456789abcde",
        "01234567-89ab-cdef-0123-456789abcdeg",
        "0123456789ab-cdef-0123-456789abcdef",
        " 01234567-89ab-cdef-0123-456789abcdef",
        "01234567-89ab-cdef-0123-456789abcdef ",
        "01234567-89ab-cdef-0123-456789abcdef\n",
        "클라이언트-id",
    ] {
        assert!(validate_client_id(CloudProvider::Onedrive, invalid).is_err(), "{invalid:?}");
    }

    let oversized = "a".repeat(513);
    assert_eq!(
        validate_client_id(CloudProvider::Onedrive, &oversized).unwrap_err(),
        "oauth-client-id-invalid"
    );
}

#[test]
fn google_client_id_requires_nonempty_safe_apps_domain_prefix() {
    assert!(validate_client_id(
        CloudProvider::GoogleDrive,
        "1234567890-abcDEF.apps.googleusercontent.com"
    )
    .is_ok());

    for invalid in [
        ".apps.googleusercontent.com",
        "abc_123.apps.googleusercontent.com",
        "abc.example.com",
        "abc 123.apps.googleusercontent.com",
    ] {
        assert_eq!(
            validate_client_id(CloudProvider::GoogleDrive, invalid).unwrap_err(),
            "oauth-client-id-provider-format-invalid",
            "{invalid:?}"
        );
    }
}

#[test]
fn icloud_client_id_validation_is_not_silently_accepted() {
    assert_eq!(
        validate_client_id(CloudProvider::Icloud, "client-id").unwrap_err(),
        "icloud-oauth-not-supported"
    );
}
