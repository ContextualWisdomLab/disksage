//! Real-filesystem edge coverage for OAuth connection-document admission.
//!
//! These cases prove DiskSage fails before interpreting an external/non-directory authority chain
//! and keeps unreadable local metadata distinct from malformed JSON. No credential or network
//! authority is exercised.

use disksage_lib::provider_oauth::load_connections;

#[test]
fn non_directory_ancestor_is_rejected_before_leaf_observation() {
    let temp = tempfile::tempdir().unwrap();
    let file_ancestor = temp.path().join("app-data");
    std::fs::write(&file_ancestor, b"not-a-directory").unwrap();
    let path = file_ancestor.join("nested").join("connections.json");

    assert_eq!(
        load_connections(&path).unwrap_err(),
        "oauth-connection-directory-unsafe"
    );
}

#[cfg(unix)]
#[test]
fn unreadable_regular_document_has_a_stable_io_error_before_json_parsing() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("connections.json");
    std::fs::write(&path, b"{\"version\":1,\"connections\":[]}").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();

    let result = load_connections(&path);

    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(result.unwrap_err(), "oauth-connection-document-unreadable");
}
