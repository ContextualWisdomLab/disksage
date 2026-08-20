//! Credential-free public-boundary coverage for OAuth connection metadata persistence.
//!
//! These tests exercise only the local, read-only connection-document parser and deterministic
//! root lookup. They never launch a browser, open a loopback listener, access the OS credential
//! store, exchange tokens, or make a network request.

use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::provider_oauth::{
    connection_for_root, connections_path, load_connections, requested_scope, OAuthConnection,
};
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

const GOOGLE_CLIENT_ID: &str = "1234567890-abcxyz.apps.googleusercontent.com";

fn google_root(path: String) -> CloudRoot {
    CloudRoot {
        id: "google-drive:account".into(),
        provider: CloudProvider::GoogleDrive,
        account_scope: CloudAccountScope::Unknown,
        label: "Google Drive".into(),
        path,
        readable: true,
        access_issue: None,
    }
}

fn canonical_connection_id(root: &CloudRoot) -> String {
    let root_id = root.id.nfc().collect::<String>();
    let root_path = root.path.nfc().collect::<String>();
    let mut hasher = Sha256::new();
    for value in [root.provider.as_str(), root_id.as_str(), root_path.as_str()] {
        hasher.update(value.as_bytes());
        hasher.update([0]);
    }
    format!("{:x}", hasher.finalize())
}

fn connection(root: &CloudRoot) -> OAuthConnection {
    OAuthConnection {
        connection_id: canonical_connection_id(root),
        provider: root.provider,
        cloud_root_id: root.id.clone(),
        cloud_root_path: root.path.clone(),
        client_id: GOOGLE_CLIENT_ID.into(),
        scope: requested_scope(root.provider).unwrap().into(),
        connected_at_ms: 123,
    }
}

#[test]
fn connection_document_path_and_missing_document_are_non_authorizing() {
    let temp = tempfile::tempdir().unwrap();
    let path = connections_path(temp.path());
    assert_eq!(path, temp.path().join("cloud-oauth-connections.json"));
    assert!(load_connections(&path).unwrap().is_empty());
}

#[test]
fn malformed_directory_version_count_and_size_fail_closed_before_lookup() {
    let temp = tempfile::tempdir().unwrap();

    let directory = temp.path().join("directory.json");
    std::fs::create_dir(&directory).unwrap();
    assert_eq!(
        load_connections(&directory).unwrap_err(),
        "oauth-connection-document-not-regular-file"
    );

    let invalid_json = temp.path().join("invalid.json");
    std::fs::write(&invalid_json, b"not-json").unwrap();
    assert_eq!(
        load_connections(&invalid_json).unwrap_err(),
        "oauth-connection-document-invalid"
    );

    let wrong_version = temp.path().join("wrong-version.json");
    std::fs::write(
        &wrong_version,
        serde_json::to_vec(&serde_json::json!({"version": 2, "connections": []})).unwrap(),
    )
    .unwrap();
    assert_eq!(
        load_connections(&wrong_version).unwrap_err(),
        "oauth-connection-document-version-or-count-invalid"
    );

    let too_many = temp.path().join("too-many.json");
    let entries: Vec<_> = (0..33).map(|_| serde_json::json!({})).collect();
    std::fs::write(
        &too_many,
        serde_json::to_vec(&serde_json::json!({"version": 1, "connections": entries})).unwrap(),
    )
    .unwrap();
    assert_eq!(
        load_connections(&too_many).unwrap_err(),
        "oauth-connection-document-version-or-count-invalid"
    );

    let oversized = temp.path().join("oversized.json");
    let file = std::fs::File::create(&oversized).unwrap();
    file.set_len(256 * 1024 + 1).unwrap();
    assert_eq!(
        load_connections(&oversized).unwrap_err(),
        "oauth-connection-document-too-large"
    );
}

#[test]
fn valid_document_binds_exact_root_and_duplicate_matches_are_ambiguous() {
    let temp = tempfile::tempdir().unwrap();
    #[cfg(windows)]
    let root = google_root(r"C:\Cloud\Drive".into());
    #[cfg(not(windows))]
    let root = google_root("/Cloud/Drive".into());
    let expected = connection(&root);
    let path = temp.path().join("connections.json");
    std::fs::write(
        &path,
        serde_json::to_vec(&serde_json::json!({
            "version": 1,
            "connections": [expected.clone()]
        }))
        .unwrap(),
    )
    .unwrap();

    let loaded = load_connections(&path).unwrap();
    assert_eq!(loaded, vec![expected.clone()]);
    assert_eq!(connection_for_root(&loaded, &root).unwrap(), expected);

    let ambiguous = vec![loaded[0].clone(), loaded[0].clone()];
    assert_eq!(
        connection_for_root(&ambiguous, &root).unwrap_err(),
        "provider-oauth-connection-ambiguous"
    );

    let mut other_root = root.clone();
    other_root.id.push_str("-other");
    assert_eq!(
        connection_for_root(&loaded, &other_root).unwrap_err(),
        "provider-oauth-connection-missing"
    );
}

#[cfg(unix)]
#[test]
fn connection_document_file_symlink_is_rejected_without_following_it() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("target.json");
    std::fs::write(&target, b"{\"version\":1,\"connections\":[]}").unwrap();
    let link = temp.path().join("connections.json");
    symlink(&target, &link).unwrap();

    assert_eq!(
        load_connections(&link).unwrap_err(),
        "oauth-connection-document-not-regular-file"
    );
}

#[cfg(unix)]
#[test]
fn connection_document_symlinked_parent_is_rejected_before_reading_outside_data() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let outside = temp.path().join("outside");
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(
        outside.join("connections.json"),
        b"{\"version\":1,\"connections\":[]}",
    )
    .unwrap();
    let alias = temp.path().join("app-data");
    symlink(&outside, &alias).unwrap();

    assert!(load_connections(&alias.join("connections.json")).is_err());
}