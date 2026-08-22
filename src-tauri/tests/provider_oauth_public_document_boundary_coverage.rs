//! Public-boundary coverage for OAuth connection-document admission.
//!
//! These cases exercise only local filesystem metadata and JSON parsing. They never touch the
//! browser, loopback callback, provider network, keyring, or cloud mutation authority.

use disksage_lib::provider_oauth::{connections_path, load_connections};
use std::path::Path;

#[cfg(unix)]
fn make_private(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
}

#[cfg(not(unix))]
fn make_private(_path: &Path) {}

fn write_private(path: &Path, bytes: &[u8]) {
    std::fs::write(path, bytes).unwrap();
    make_private(path);
}

#[test]
fn connection_path_and_missing_document_are_deterministic_without_side_effects() {
    let temp = tempfile::tempdir().unwrap();
    let path = connections_path(temp.path());

    assert_eq!(path, temp.path().join("cloud-oauth-connections.json"));
    assert_eq!(load_connections(&path).unwrap(), Vec::new());
    assert!(!path.exists(), "read-only lookup must not create the document");
}

#[test]
fn non_regular_and_oversized_documents_fail_before_json_interpretation() {
    let temp = tempfile::tempdir().unwrap();
    let directory_leaf = temp.path().join("connections-directory");
    std::fs::create_dir(&directory_leaf).unwrap();
    assert_eq!(
        load_connections(&directory_leaf).unwrap_err(),
        "oauth-connection-document-not-regular-file"
    );

    let oversized = temp.path().join("oversized.json");
    write_private(&oversized, &vec![b' '; 256 * 1024 + 1]);
    assert_eq!(
        load_connections(&oversized).unwrap_err(),
        "oauth-connection-document-too-large"
    );
}

#[test]
fn structured_document_version_and_schema_are_fail_closed() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("connections.json");

    write_private(&path, br#"{"version":2,"connections":[]}"#);
    assert_eq!(
        load_connections(&path).unwrap_err(),
        "oauth-connection-document-version-or-count-invalid"
    );

    write_private(
        &path,
        br#"{"version":1,"connections":[],"unexpected":true}"#,
    );
    assert_eq!(
        load_connections(&path).unwrap_err(),
        "oauth-connection-document-invalid"
    );

    write_private(&path, br#"{"version":1,"connections":[]}"#);
    assert_eq!(load_connections(&path).unwrap(), Vec::new());
}
