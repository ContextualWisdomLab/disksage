//! Public-contract coverage for fixed-host provider metadata locator construction.
//!
//! These tests are credential-free and network-free. They exercise the production path/ID
//! admission boundary so malformed or ambiguous remote locators fail closed before any provider
//! transport is invoked.

use disksage_lib::cloud::CloudProvider;
use disksage_lib::provider_api_client::{
    google_drive_path_locator, onedrive_path_locator, provider_metadata_url,
    ProviderRemoteLocator,
};

#[test]
fn locator_identity_and_fixed_host_urls_preserve_provider_authority() {
    let onedrive = ProviderRemoteLocator::OneDriveItemId("item ! 1".into());
    assert_eq!(onedrive.provider(), CloudProvider::Onedrive);
    assert_eq!(onedrive.object_id(), Some("item ! 1"));
    assert!(!onedrive.location_bound());
    assert_eq!(
        provider_metadata_url(&onedrive).unwrap(),
        "https://graph.microsoft.com/v1.0/me/drive/items/item%20%21%201?%24select=id%2Csize%2CeTag%2Cfile%2Cdeleted"
    );

    let google = ProviderRemoteLocator::GoogleDriveFileId("g/id?#".into());
    assert_eq!(google.provider(), CloudProvider::GoogleDrive);
    assert_eq!(google.object_id(), Some("g/id?#"));
    assert!(!google.location_bound());
    assert_eq!(
        provider_metadata_url(&google).unwrap(),
        "https://www.googleapis.com/drive/v3/files/g%2Fid%3F%23?fields=id%2Cname%2Cparents%2CmimeType%2CdriveId%2Cversion%2Csize%2Csha256Checksum%2Ctrashed&supportsAllDrives=true"
    );

    let root = tempfile::tempdir().unwrap();
    let nested = root.path().join("Archive").join("보고서 #1.pdf");
    let path_locator = onedrive_path_locator(root.path(), &nested).unwrap();
    assert_eq!(path_locator.provider(), CloudProvider::Onedrive);
    assert_eq!(path_locator.object_id(), None);
    assert!(path_locator.location_bound());
    assert_eq!(
        provider_metadata_url(&path_locator).unwrap(),
        "https://graph.microsoft.com/v1.0/me/drive/root:/Archive/%EB%B3%B4%EA%B3%A0%EC%84%9C%20%231.pdf?%24select=id%2Csize%2CeTag%2Cfile%2Cdeleted"
    );
}

#[test]
fn path_locator_builders_reject_outside_root_root_itself_and_parent_components() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();

    assert_eq!(
        onedrive_path_locator(root.path(), &outside.path().join("file.bin")).unwrap_err(),
        "destination-outside-cloud-root"
    );
    assert_eq!(
        onedrive_path_locator(root.path(), root.path()).unwrap_err(),
        "provider-path-invalid"
    );

    let parent_component = root.path().join("folder").join("..").join("file.bin");
    assert_eq!(
        onedrive_path_locator(root.path(), &parent_component).unwrap_err(),
        "provider-path-invalid"
    );

    assert_eq!(
        google_drive_path_locator(root.path(), &outside.path().join("file.bin"), "file-id")
            .unwrap_err(),
        "destination-outside-cloud-root"
    );
    assert_eq!(
        google_drive_path_locator(root.path(), root.path(), "file-id").unwrap_err(),
        "provider-path-invalid"
    );
    assert_eq!(
        google_drive_path_locator(root.path(), &root.path().join("file.bin"), "bad\nid")
            .unwrap_err(),
        "provider-object-id-invalid"
    );
}

#[test]
fn google_drive_locator_enforces_parent_chain_depth_before_remote_collection() {
    let root = tempfile::tempdir().unwrap();
    let mut destination = root.path().to_path_buf();
    for index in 0..102 {
        destination.push(format!("segment-{index}"));
    }

    assert_eq!(
        google_drive_path_locator(root.path(), &destination, "file-id").unwrap_err(),
        "google-drive-path-too-deep"
    );
}

#[test]
fn provider_object_ids_and_paths_enforce_bounded_wire_inputs() {
    for invalid in ["", " leading", "trailing ", "line\nbreak"] {
        assert_eq!(
            provider_metadata_url(&ProviderRemoteLocator::OneDriveItemId(invalid.into()))
                .unwrap_err(),
            "provider-object-id-invalid"
        );
        assert_eq!(
            provider_metadata_url(&ProviderRemoteLocator::GoogleDriveFileId(invalid.into()))
                .unwrap_err(),
            "provider-object-id-invalid"
        );
    }

    assert_eq!(
        provider_metadata_url(&ProviderRemoteLocator::GoogleDriveFileId("x".repeat(1_025)))
            .unwrap_err(),
        "provider-object-id-invalid"
    );
}
