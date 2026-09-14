use disksage_lib::cloud::CloudProvider;
use disksage_lib::provider_oauth::{connections_path, load_connections};
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
