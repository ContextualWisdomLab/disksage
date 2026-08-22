#![cfg(feature = "cloud-cli")]

//! Black-box coverage for the shipped provider OAuth CLI connection-document boundary.
//!
//! These tests launch the real binary against local, permission-bounded connection documents. They
//! exercise non-empty machine-readable list output and fail-closed duplicate identity handling
//! without opening a browser, contacting a provider, touching the credential store, or mutating a
//! cloud root.

use disksage_lib::cloud::CloudProvider;
use disksage_lib::provider_oauth::{requested_scope, OAuthConnection};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::process::{Command, Output};

const GOOGLE_CLIENT_ID: &str = "1234567890-abcxyz.apps.googleusercontent.com";

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_disksage-provider-oauth"))
}

fn connection_id(provider: CloudProvider, root_id: &str, root_path: &str) -> String {
    let mut hasher = Sha256::new();
    for value in [provider.as_str(), root_id, root_path] {
        hasher.update(value.as_bytes());
        hasher.update([0]);
    }
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn google_connection(root_path: &Path) -> OAuthConnection {
    let root_path = root_path.to_string_lossy().into_owned();
    OAuthConnection {
        connection_id: connection_id(CloudProvider::GoogleDrive, "google-account", &root_path),
        provider: CloudProvider::GoogleDrive,
        cloud_root_id: "google-account".into(),
        cloud_root_path: root_path,
        client_id: GOOGLE_CLIENT_ID.into(),
        scope: requested_scope(CloudProvider::GoogleDrive)
            .expect("Google Drive must expose its read-only scope")
            .into(),
        connected_at_ms: 123,
    }
}

fn write_private_document(path: &Path, connections: &[OAuthConnection]) {
    let document = serde_json::json!({
        "version": 1,
        "connections": connections,
    });
    std::fs::write(path, serde_json::to_vec(&document).expect("document must serialize"))
        .expect("connection document should be written");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .expect("connection document should remain private");
    }
}

fn run_list(home: &Path, connections: &Path) -> Output {
    command()
        .arg("--home")
        .arg(home)
        .arg("--connections")
        .arg(connections)
        .arg("--list")
        .output()
        .expect("provider OAuth CLI should start")
}

#[test]
fn read_only_list_serializes_valid_nonempty_connection_document_without_secret_or_mutation() {
    let temp = tempfile::tempdir().expect("temporary app-data root should be created");
    let cloud_root = temp.path().join("cloud-root");
    let connection = google_connection(&cloud_root);
    let document = temp.path().join("connections.json");
    write_private_document(&document, std::slice::from_ref(&connection));

    let output = run_list(temp.path(), &document);

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("list stdout should remain JSON");
    assert_eq!(value["action"], "list");
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["connection_count"], 1);
    assert_eq!(value["connections"][0]["connection_id"], connection.connection_id);
    assert_eq!(value["connections"][0]["cloud_root_id"], "google-account");
    assert_eq!(value["secrets_included"], false);
    assert_eq!(value["connection_document_effect"], "none");
    assert_eq!(value["credential_store_effect"], "none");
    assert_eq!(value["cloud_write_executed"], false);
    assert_eq!(value["source_eviction_executed"], false);
}

#[test]
fn read_only_list_rejects_duplicate_connection_identity_without_partial_output() {
    let temp = tempfile::tempdir().expect("temporary app-data root should be created");
    let connection = google_connection(&temp.path().join("cloud-root"));
    let document = temp.path().join("connections.json");
    write_private_document(&document, &[connection.clone(), connection]);

    let output = run_list(temp.path(), &document);

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(output.stderr, b"oauth-connection-document-duplicate-id\n");
}
