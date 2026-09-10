use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::provider_oauth::{
    connection_for_root, connections_path, load_connections, requested_scope, requested_write_scope,
    scope_allows_write, validate_client_id, OAuthConnection,
};
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

fn connection(provider: CloudProvider, scope: &str) -> OAuthConnection {
    OAuthConnection {
        connection_id: "0".repeat(64),
        provider,
        cloud_root_id: "root-1".into(),
        cloud_root_path: "/tmp/disksage-provider-root".into(),
        client_id: "01234567-89ab-cdef-0123-456789abcdef".into(),
        scope: scope.into(),
        connected_at_ms: 1,
    }
}

#[test]
fn provider_scopes_and_client_ids_fail_closed_by_provider() {
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

    let microsoft_client = "01234567-89ab-cdef-0123-456789abcdef";
    let google_client = "client-123.apps.googleusercontent.com";
    assert_eq!(
        validate_client_id(CloudProvider::Onedrive, microsoft_client),
        Ok(())
    );
    assert_eq!(
        validate_client_id(CloudProvider::GoogleDrive, google_client),
        Ok(())
    );

    for invalid in ["", " client", "client ", "not-a-guid", "01234567-89ab-cdef-0123-456789abcdeg"] {
        assert!(validate_client_id(CloudProvider::Onedrive, invalid).is_err());
    }
    assert_eq!(
        validate_client_id(
            CloudProvider::GoogleDrive,
            "client_123.apps.googleusercontent.com"
        )
        .unwrap_err(),
        "oauth-client-id-provider-format-invalid"
    );
    assert_eq!(
        validate_client_id(CloudProvider::Icloud, microsoft_client).unwrap_err(),
        "icloud-oauth-not-supported"
    );

    assert!(scope_allows_write(&connection(
        CloudProvider::Onedrive,
        "Files.ReadWrite offline_access"
    )));
    assert!(!scope_allows_write(&connection(
        CloudProvider::Onedrive,
        "Files.Read offline_access"
    )));
    assert!(!scope_allows_write(&connection(
        CloudProvider::Icloud,
        "Files.ReadWrite offline_access"
    )));
}

#[test]
fn connection_document_admission_rejects_invalid_filesystem_and_json_shapes() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "disksage-provider-oauth-boundary-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&root).unwrap();
    let path = connections_path(&root);
    assert!(path.ends_with("cloud-oauth-connections.json"));

    assert_eq!(load_connections(&path).unwrap(), Vec::<OAuthConnection>::new());

    fs::create_dir(&path).unwrap();
    assert_eq!(
        load_connections(&path).unwrap_err(),
        "oauth-connection-document-not-regular-file"
    );
    fs::remove_dir(&path).unwrap();

    fs::write(&path, b"not-json").unwrap();
    assert_eq!(
        load_connections(&path).unwrap_err(),
        "oauth-connection-document-invalid"
    );

    fs::write(&path, br#"{"version":2,"connections":[]}"#).unwrap();
    assert_eq!(
        load_connections(&path).unwrap_err(),
        "oauth-connection-document-version-or-count-invalid"
    );

    fs::write(&path, vec![b'x'; 256 * 1024 + 1]).unwrap();
    assert_eq!(
        load_connections(&path).unwrap_err(),
        "oauth-connection-document-too-large"
    );

    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn root_lookup_without_an_admitted_connection_is_explicitly_missing() {
    let root = CloudRoot {
        id: "root-1".into(),
        provider: CloudProvider::Onedrive,
        account_scope: CloudAccountScope::Unknown,
        label: "OneDrive".into(),
        path: Path::new("/tmp/disksage-provider-root")
            .to_string_lossy()
            .into_owned(),
        readable: true,
        access_issue: None,
    };

    assert_eq!(
        connection_for_root(&[], &root).unwrap_err(),
        "provider-oauth-connection-missing"
    );
}
