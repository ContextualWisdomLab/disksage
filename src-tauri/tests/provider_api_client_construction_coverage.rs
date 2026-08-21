//! Credential-free construction and fail-closed coverage for the fixed-host provider metadata client.
//!
//! These tests intentionally stop before any network request. They exercise the production client
//! configuration and its locator-validation boundary with an invalid opaque ID, proving that
//! malformed remote identifiers are rejected before bearer credentials can reach transport I/O.

#![cfg(not(coverage))]

use disksage_lib::provider_api_client::{
    FixedHostProviderMetadataClient, ProviderMetadataTransport, ProviderRemoteLocator,
};

#[test]
fn fixed_host_client_rejects_invalid_locator_before_network_io() {
    let client = FixedHostProviderMetadataClient::default();
    let locator = ProviderRemoteLocator::OneDriveItemId(String::new());

    let error = client
        .fetch_json(&locator, "credential-must-not-be-used")
        .expect_err("an empty provider object ID must fail before transport I/O");

    assert_eq!(error, "provider-object-id-invalid");
    assert_eq!(locator.provider(), disksage_lib::cloud::CloudProvider::Onedrive);
    assert_eq!(locator.object_id(), Some(""));
    assert!(!locator.location_bound());
}

#[test]
fn fixed_host_client_rejects_control_character_locator_without_network_io() {
    let client = FixedHostProviderMetadataClient::default();
    let locator = ProviderRemoteLocator::GoogleDriveFileId("opaque\nremote-id".into());

    let error = client
        .fetch_json(&locator, "credential-must-not-be-used")
        .expect_err("control characters must fail before transport I/O");

    assert_eq!(error, "provider-object-id-invalid");
    assert_eq!(locator.provider(), disksage_lib::cloud::CloudProvider::GoogleDrive);
    assert_eq!(locator.object_id(), Some("opaque\nremote-id"));
    assert!(!locator.location_bound());
}
