use disksage_lib::cloud::CloudProvider;
use disksage_lib::provider_oauth::{
    connections_path, load_connections, requested_scope, requested_write_scope, scope_allows_write,
    validate_client_id, OAuthConnection,
};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::fs;

fn connection_id(provider: CloudProvider, root_id: &str, root_path: &str) -> String {
    let mut hasher = Sha256::new();
    for value in [provider.as_str(), root_id, root_path] {
        hasher.update(value.as_bytes());
        hasher.update([0]);
    }
    let digest = hasher.finalize();
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut encoded, "{byte:02x}").unwrap();
    }
    encoded
}

fn write_connection(
    path: &std::path::Path,
    provider: &str,
    connection_id: String,
    root_id: &str,
    root_path: &str,
    client_id: &str,
    scope: &str,
) {
    fs::write(
        path,
        serde_json::to_vec(&serde_json::json!({
            "version": 1,
            "connections": [{
                "connection_id": connection_id,
                "provider": provider,
                "cloud_root_id": root_id,
                "cloud_root_path": root_path,
                "client_id": client_id,
                "scope": scope,
                "connected_at_ms": 1
            }]
        }))
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn provider_scope_and_client_id_boundaries_are_explicit() {
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

    assert!(validate_client_id(
        CloudProvider::Onedrive,
        "01234567-89ab-cdef-0123-456789abcdef"
    )
    .is_ok());
    assert!(validate_client_id(
        CloudProvider::GoogleDrive,
        "client-123.apps.googleusercontent.com"
    )
    .is_ok());

    for invalid in ["", " leading", "trailing ", "contains\ncontrol", "café"] {
        assert_eq!(
            validate_client_id(CloudProvider::GoogleDrive, invalid).unwrap_err(),
            "oauth-client-id-invalid"
        );
    }
    let oversized = "a".repeat(513);
    assert_eq!(
        validate_client_id(CloudProvider::GoogleDrive, &oversized).unwrap_err(),
        "oauth-client-id-invalid"
    );
    assert_eq!(
        validate_client_id(CloudProvider::Onedrive, "not-a-guid").unwrap_err(),
        "oauth-client-id-provider-format-invalid"
    );
    assert_eq!(
        validate_client_id(CloudProvider::GoogleDrive, ".apps.googleusercontent.com").unwrap_err(),
        "oauth-client-id-provider-format-invalid"
    );
    assert_eq!(
        validate_client_id(CloudProvider::Icloud, "01234567-89ab-cdef-0123-456789abcdef")
            .unwrap_err(),
        "icloud-oauth-not-supported"
    );

    let write_connection = OAuthConnection {
        connection_id: "0".repeat(64),
        provider: CloudProvider::GoogleDrive,
        cloud_root_id: "root".into(),
        cloud_root_path: "/tmp/root".into(),
        client_id: "client.apps.googleusercontent.com".into(),
        scope: "https://www.googleapis.com/auth/drive".into(),
        connected_at_ms: 1,
    };
    assert!(scope_allows_write(&write_connection));

    let read_connection = OAuthConnection {
        scope: "https://www.googleapis.com/auth/drive.metadata.readonly".into(),
        ..write_connection.clone()
    };
    assert!(!scope_allows_write(&read_connection));

    let unsupported_connection = OAuthConnection {
        provider: CloudProvider::Icloud,
        ..write_connection
    };
    assert!(!scope_allows_write(&unsupported_connection));
}

#[test]
fn google_drive_write_connection_document_is_admitted() {
    let directory = tempfile::tempdir().unwrap();
    let path = connections_path(directory.path());
    let root_id = "google-root";
    let root_path = "/tmp/google-drive-root";

    write_connection(
        &path,
        "google-drive",
        connection_id(CloudProvider::GoogleDrive, root_id, root_path),
        root_id,
        root_path,
        "client-123.apps.googleusercontent.com",
        "https://www.googleapis.com/auth/drive",
    );

    let connections = load_connections(&path).unwrap();
    assert_eq!(connections.len(), 1);
    assert_eq!(connections[0].provider, CloudProvider::GoogleDrive);
    assert_eq!(connections[0].scope, "https://www.googleapis.com/auth/drive");
}

#[test]
fn onedrive_read_connection_document_is_admitted() {
    let directory = tempfile::tempdir().unwrap();
    let path = connections_path(directory.path());
    let root_id = "onedrive-root";
    let root_path = "/tmp/onedrive-root";

    write_connection(
        &path,
        "onedrive",
        connection_id(CloudProvider::Onedrive, root_id, root_path),
        root_id,
        root_path,
        "01234567-89ab-cdef-0123-456789abcdef",
        "Files.Read offline_access",
    );

    let connections = load_connections(&path).unwrap();
    assert_eq!(connections.len(), 1);
    assert_eq!(connections[0].provider, CloudProvider::Onedrive);
    assert!(!scope_allows_write(&connections[0]));
}

#[test]
fn legacy_decomposed_unicode_connection_identity_remains_readable() {
    let directory = tempfile::tempdir().unwrap();
    let path = connections_path(directory.path());
    let root_id = "Cafe\u{301}";
    let root_path = "/tmp/Cafe\u{301}";

    write_connection(
        &path,
        "google-drive",
        connection_id(CloudProvider::GoogleDrive, root_id, root_path),
        root_id,
        root_path,
        "client.apps.googleusercontent.com",
        "https://www.googleapis.com/auth/drive.metadata.readonly",
    );

    let connections = load_connections(&path).unwrap();
    assert_eq!(connections.len(), 1);
    assert_eq!(connections[0].cloud_root_id, root_id);
    assert_eq!(connections[0].cloud_root_path, root_path);
}

#[test]
fn icloud_connection_document_fails_closed_at_oauth_provider_boundary() {
    let directory = tempfile::tempdir().unwrap();
    let path = connections_path(directory.path());
    let root_id = "icloud-root";
    let root_path = "/tmp/icloud-root";

    write_connection(
        &path,
        "icloud",
        connection_id(CloudProvider::Icloud, root_id, root_path),
        root_id,
        root_path,
        "01234567-89ab-cdef-0123-456789abcdef",
        "Files.Read offline_access",
    );

    assert_eq!(load_connections(&path).unwrap_err(), "icloud-oauth-not-supported");
}

#[test]
fn missing_connection_document_is_an_empty_store() {
    let directory = tempfile::tempdir().unwrap();
    let path = connections_path(directory.path());

    assert!(load_connections(&path).unwrap().is_empty());
}

#[test]
fn directory_connection_document_is_rejected_before_read() {
    let directory = tempfile::tempdir().unwrap();
    let path = connections_path(directory.path());
    fs::create_dir(&path).unwrap();

    assert_eq!(
        load_connections(&path).unwrap_err(),
        "oauth-connection-document-not-regular-file"
    );
}

#[cfg(unix)]
#[test]
fn symlink_connection_document_is_rejected_before_read() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("target.json");
    let path = connections_path(directory.path());
    fs::write(&target, b"{}").unwrap();
    symlink(&target, &path).unwrap();

    assert_eq!(
        load_connections(&path).unwrap_err(),
        "oauth-connection-document-not-regular-file"
    );
}

#[test]
fn oversized_connection_document_is_rejected_before_parse() {
    let directory = tempfile::tempdir().unwrap();
    let path = connections_path(directory.path());
    fs::write(&path, vec![b'x'; 256 * 1024 + 1]).unwrap();

    assert_eq!(
        load_connections(&path).unwrap_err(),
        "oauth-connection-document-too-large"
    );
}

#[test]
fn malformed_connection_document_fails_closed() {
    let directory = tempfile::tempdir().unwrap();
    let path = connections_path(directory.path());
    fs::write(&path, b"{not-json").unwrap();

    assert_eq!(
        load_connections(&path).unwrap_err(),
        "oauth-connection-document-invalid"
    );
}

#[test]
fn unsupported_connection_document_version_fails_closed() {
    let directory = tempfile::tempdir().unwrap();
    let path = connections_path(directory.path());
    fs::write(
        &path,
        serde_json::to_vec(&serde_json::json!({
            "version": 2,
            "connections": []
        }))
        .unwrap(),
    )
    .unwrap();

    assert_eq!(
        load_connections(&path).unwrap_err(),
        "oauth-connection-document-version-or-count-invalid"
    );
}

#[test]
fn over_capacity_connection_document_fails_before_record_validation() {
    let directory = tempfile::tempdir().unwrap();
    let path = connections_path(directory.path());
    let record = serde_json::json!({
        "connection_id": "not-semantic-authority",
        "provider": "google-drive",
        "cloud_root_id": "root",
        "cloud_root_path": "/tmp/root",
        "client_id": "client.apps.googleusercontent.com",
        "scope": "https://www.googleapis.com/auth/drive.metadata.readonly",
        "connected_at_ms": 1
    });
    let connections = vec![record; 33];
    fs::write(
        &path,
        serde_json::to_vec(&serde_json::json!({
            "version": 1,
            "connections": connections
        }))
        .unwrap(),
    )
    .unwrap();

    assert_eq!(
        load_connections(&path).unwrap_err(),
        "oauth-connection-document-version-or-count-invalid"
    );
}

#[test]
fn invalid_connection_semantics_fail_closed_before_authority_is_returned() {
    let directory = tempfile::tempdir().unwrap();
    let path = connections_path(directory.path());
    let root_id = "root";
    let root_path = "/tmp/root";
    let valid_id = connection_id(CloudProvider::GoogleDrive, root_id, root_path);

    let cases = [
        (
            "short-id",
            "short".to_string(),
            root_id,
            root_path,
            "client.apps.googleusercontent.com",
            "https://www.googleapis.com/auth/drive.metadata.readonly",
            "oauth-connection-invalid",
        ),
        (
            "non-hex-id",
            "g".repeat(64),
            root_id,
            root_path,
            "client.apps.googleusercontent.com",
            "https://www.googleapis.com/auth/drive.metadata.readonly",
            "oauth-connection-invalid",
        ),
        (
            "blank-root-id",
            valid_id.clone(),
            " ",
            root_path,
            "client.apps.googleusercontent.com",
            "https://www.googleapis.com/auth/drive.metadata.readonly",
            "oauth-connection-invalid",
        ),
        (
            "relative-root-path",
            valid_id.clone(),
            root_id,
            "relative/root",
            "client.apps.googleusercontent.com",
            "https://www.googleapis.com/auth/drive.metadata.readonly",
            "oauth-connection-invalid",
        ),
        (
            "invalid-scope",
            valid_id.clone(),
            root_id,
            root_path,
            "client.apps.googleusercontent.com",
            "https://example.invalid/scope",
            "oauth-connection-invalid",
        ),
        (
            "invalid-client-id",
            valid_id.clone(),
            root_id,
            root_path,
            "invalid-client-id",
            "https://www.googleapis.com/auth/drive.metadata.readonly",
            "oauth-client-id-provider-format-invalid",
        ),
        (
            "mismatched-identity",
            "0".repeat(64),
            root_id,
            root_path,
            "client.apps.googleusercontent.com",
            "https://www.googleapis.com/auth/drive.metadata.readonly",
            "oauth-connection-id-mismatch",
        ),
    ];

    for (case, id, case_root_id, case_root_path, client_id, scope, expected) in cases {
        write_connection(
            &path,
            "google-drive",
            id,
            case_root_id,
            case_root_path,
            client_id,
            scope,
        );
        assert_eq!(load_connections(&path).unwrap_err(), expected, "{case}");
    }
}
