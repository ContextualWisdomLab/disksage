use disksage_lib::cloud::CloudProvider;
use disksage_lib::provider_oauth::{
    connections_path, load_connections, requested_scope, requested_write_scope,
    scope_allows_write, validate_client_id, OAuthConnection,
};

fn connection(provider: CloudProvider, scope: &str) -> OAuthConnection {
    OAuthConnection {
        connection_id: "0".repeat(64),
        provider,
        cloud_root_id: "root".into(),
        cloud_root_path: "/tmp/root".into(),
        client_id: "client".into(),
        scope: scope.into(),
        connected_at_ms: 1,
    }
}

#[test]
fn scope_contract_covers_read_write_and_unsupported_provider() {
    assert_eq!(
        requested_scope(CloudProvider::Onedrive).unwrap(),
        "Files.Read offline_access"
    );
    assert_eq!(
        requested_write_scope(CloudProvider::Onedrive).unwrap(),
        "Files.ReadWrite offline_access"
    );
    assert_eq!(
        requested_scope(CloudProvider::GoogleDrive).unwrap(),
        "https://www.googleapis.com/auth/drive.metadata.readonly"
    );
    assert_eq!(
        requested_write_scope(CloudProvider::GoogleDrive).unwrap(),
        "https://www.googleapis.com/auth/drive"
    );
    assert_eq!(
        requested_scope(CloudProvider::Icloud).unwrap_err(),
        "icloud-oauth-not-supported"
    );
    assert_eq!(
        requested_write_scope(CloudProvider::Icloud).unwrap_err(),
        "icloud-oauth-not-supported"
    );

    assert!(scope_allows_write(&connection(
        CloudProvider::GoogleDrive,
        "https://www.googleapis.com/auth/drive"
    )));
    assert!(!scope_allows_write(&connection(
        CloudProvider::GoogleDrive,
        "https://www.googleapis.com/auth/drive.metadata.readonly"
    )));
    assert!(!scope_allows_write(&connection(
        CloudProvider::Icloud,
        "icloud"
    )));
}

#[test]
fn client_id_contract_covers_provider_formats_and_fail_closed_inputs() {
    assert!(validate_client_id(
        CloudProvider::Onedrive,
        "12345678-1234-1234-1234-123456789abc"
    )
    .is_ok());
    assert!(validate_client_id(
        CloudProvider::GoogleDrive,
        "google-client.apps.googleusercontent.com"
    )
    .is_ok());

    assert_eq!(
        validate_client_id(CloudProvider::Icloud, "client").unwrap_err(),
        "icloud-oauth-not-supported"
    );
    assert_eq!(
        validate_client_id(
            CloudProvider::GoogleDrive,
            " google-client.apps.googleusercontent.com"
        )
        .unwrap_err(),
        "oauth-client-id-invalid"
    );
    assert_eq!(
        validate_client_id(CloudProvider::Onedrive, "not-a-guid").unwrap_err(),
        "oauth-client-id-provider-format-invalid"
    );
}

#[test]
fn absent_local_connection_document_is_an_empty_real_filesystem_state() {
    let directory = tempfile::tempdir().unwrap();
    let path = connections_path(directory.path());
    assert_eq!(path, directory.path().join("cloud-oauth-connections.json"));
    assert_eq!(load_connections(&path).unwrap(), Vec::<OAuthConnection>::new());
}
