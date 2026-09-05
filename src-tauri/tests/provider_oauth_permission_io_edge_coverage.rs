//! Real-filesystem permission/I/O edge coverage for OAuth connection-document admission.
//!
//! These cases preserve the #156 evidence that is not already covered by the dedicated leaf and
//! shared-writable-parent matrices: an owner-unreadable private leaf must remain an I/O failure,
//! and an untraversable private authority directory must not be mistaken for a missing document.

#[cfg(unix)]
use disksage_lib::provider_oauth::load_connections;

#[cfg(unix)]
fn write_private_document(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;

    std::fs::write(path, b"{\"version\":1,\"connections\":[]}").unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
}

#[cfg(unix)]
#[test]
fn owner_unreadable_document_fails_as_io_without_parsing_partial_state() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("connections-owner-unreadable.json");
    write_private_document(&path);
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();

    let result = load_connections(&path);

    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(
        result.unwrap_err(),
        "oauth-connection-document-unreadable",
        "an unreadable private leaf must fail closed rather than becoming an empty or parsed document"
    );
}

#[cfg(unix)]
#[test]
fn untraversable_private_parent_is_unavailable_not_missing() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    let parent = temp.path().join("oauth-untraversable-parent");
    std::fs::create_dir(&parent).unwrap();
    let path = parent.join("connections.json");
    write_private_document(&path);
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o600)).unwrap();

    let result = load_connections(&path);

    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(
        result.unwrap_err(),
        "oauth-connection-document-unavailable",
        "an untraversable authority directory must not be treated as a non-authorizing missing document"
    );
}
