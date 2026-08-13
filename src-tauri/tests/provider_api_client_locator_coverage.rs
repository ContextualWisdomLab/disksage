//! Deterministic public-contract coverage for provider metadata locators.
//!
//! These tests exercise only local path/id validation and fixed-host URL construction; they make
//! no network requests and never require provider credentials.

use disksage_lib::cloud::CloudProvider;
use disksage_lib::provider_api_client::{
    google_drive_path_locator, onedrive_path_locator, provider_metadata_url, ProviderRemoteLocator,
};

#[test]
fn opaque_remote_ids_are_bounded_and_percent_encoded_without_host_control() {
    for object_id in ["", " leading", "trailing ", "line\nbreak"] {
        let locator = ProviderRemoteLocator::OneDriveItemId(object_id.to_string());
        assert_eq!(
            provider_metadata_url(&locator).unwrap_err(),
            "provider-object-id-invalid"
        );
    }

    let oversized = ProviderRemoteLocator::GoogleDriveFileId("x".repeat(1_025));
    assert_eq!(
        provider_metadata_url(&oversized).unwrap_err(),
        "provider-object-id-invalid"
    );

    let onedrive = ProviderRemoteLocator::OneDriveItemId("A/B ?#%".into());
    let url = provider_metadata_url(&onedrive).unwrap();
    assert!(url.starts_with("https://graph.microsoft.com/v1.0/me/drive/items/"));
    assert!(url.contains("A%2FB%20%3F%23%25"));
    assert_eq!(onedrive.provider(), CloudProvider::Onedrive);
    assert_eq!(onedrive.object_id(), Some("A/B ?#%"));
    assert!(!onedrive.location_bound());

    let google = ProviderRemoteLocator::GoogleDriveFileId("id/with space".into());
    let url = provider_metadata_url(&google).unwrap();
    assert!(url.starts_with("https://www.googleapis.com/drive/v3/files/"));
    assert!(url.contains("id%2Fwith%20space"));
    assert_eq!(google.provider(), CloudProvider::GoogleDrive);
    assert_eq!(google.object_id(), Some("id/with space"));
    assert!(!google.location_bound());
}

#[test]
fn onedrive_path_locator_binds_only_descendants_and_normalizes_segments() {
    let root = tempfile::tempdir().unwrap();
    let destination = root.path().join("Folder").join("a b#.txt");
    let locator = onedrive_path_locator(root.path(), &destination).unwrap();

    assert_eq!(locator.provider(), CloudProvider::Onedrive);
    assert_eq!(locator.object_id(), None);
    assert!(locator.location_bound());
    let url = provider_metadata_url(&locator).unwrap();
    assert!(url.starts_with("https://graph.microsoft.com/v1.0/me/drive/root:/"));
    assert!(url.contains("Folder/a%20b%23.txt"));

    assert_eq!(
        onedrive_path_locator(root.path(), root.path()).unwrap_err(),
        "provider-path-invalid"
    );
    let outside = tempfile::tempdir().unwrap();
    assert_eq!(
        onedrive_path_locator(root.path(), &outside.path().join("file.txt")).unwrap_err(),
        "destination-outside-cloud-root"
    );
}

#[test]
fn google_drive_path_locator_rejects_invalid_ids_and_excessive_depth() {
    let root = tempfile::tempdir().unwrap();
    let destination = root.path().join("folder").join("file.txt");

    assert_eq!(
        google_drive_path_locator(root.path(), &destination, " bad-id").unwrap_err(),
        "provider-object-id-invalid"
    );
    assert!(google_drive_path_locator(root.path(), &destination, "file-id").is_ok());

    let mut deep = root.path().to_path_buf();
    for index in 0..102 {
        deep.push(format!("s{index}"));
    }
    assert_eq!(
        google_drive_path_locator(root.path(), &deep, "file-id").unwrap_err(),
        "google-drive-path-too-deep"
    );
}
