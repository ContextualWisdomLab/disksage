//! Public-boundary coverage for provider-specific OAuth client-ID grammars.
//!
//! These tests exercise only deterministic admission logic. They do not bind a listener, launch a
//! browser, contact a provider, touch the keyring, or persist connection metadata.

use disksage_lib::cloud::CloudProvider;
use disksage_lib::provider_oauth::{requested_scope, validate_client_id};

const MICROSOFT_CLIENT_ID: &str = "12345678-1234-4abc-8def-1234567890ab";
const GOOGLE_CLIENT_ID: &str = "1234567890-abcxyz.apps.googleusercontent.com";

#[test]
fn microsoft_client_id_requires_exact_five_part_hex_guid_shape() {
    for valid in [
        MICROSOFT_CLIENT_ID,
        "ABCDEF12-3456-789A-bcde-FEDCBA987654",
    ] {
        assert_eq!(validate_client_id(CloudProvider::Onedrive, valid), Ok(()));
    }

    for invalid in [
        "12345678-1234-4abc-8def",
        "1234567-1234-4abc-8def-1234567890ab",
        "12345678-123-4abc-8def-1234567890ab",
        "12345678-1234-4abc-8def-1234567890a",
        "1234567g-1234-4abc-8def-1234567890ab",
        "12345678-1234-4abc-8def-1234567890ab-extra",
    ] {
        assert_eq!(
            validate_client_id(CloudProvider::Onedrive, invalid).unwrap_err(),
            "oauth-client-id-provider-format-invalid",
            "unexpectedly accepted malformed Microsoft client ID: {invalid}"
        );
    }
}

#[test]
fn google_client_id_requires_nonempty_alphanumeric_or_dash_prefix_and_exact_suffix() {
    for valid in [
        GOOGLE_CLIENT_ID,
        "A1-b2-C3.apps.googleusercontent.com",
        "0.apps.googleusercontent.com",
    ] {
        assert_eq!(validate_client_id(CloudProvider::GoogleDrive, valid), Ok(()));
    }

    for invalid in [
        ".apps.googleusercontent.com",
        "abc_xyz.apps.googleusercontent.com",
        "abc.xyz.apps.googleusercontent.com",
        "abc.apps.googleusercontent.co",
        "abc.apps.googleusercontent.com.extra",
    ] {
        assert_eq!(
            validate_client_id(CloudProvider::GoogleDrive, invalid).unwrap_err(),
            "oauth-client-id-provider-format-invalid",
            "unexpectedly accepted malformed Google client ID: {invalid}"
        );
    }
}

#[test]
fn unsupported_icloud_scope_and_client_id_remain_fail_closed() {
    assert_eq!(
        requested_scope(CloudProvider::Icloud).unwrap_err(),
        "icloud-oauth-not-supported"
    );
    assert_eq!(
        validate_client_id(CloudProvider::Icloud, MICROSOFT_CLIENT_ID).unwrap_err(),
        "icloud-oauth-not-supported"
    );
}
