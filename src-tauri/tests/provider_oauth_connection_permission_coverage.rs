//! Real-filesystem coverage for OAuth connection-document permission authority.
//!
//! The tests exercise the public read boundary only. They never open a browser, access a
//! credential store, or perform provider network requests.

#[cfg(unix)]
use disksage_lib::provider_oauth::load_connections;

#[cfg(unix)]
fn write_empty_document(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;

    std::fs::write(path, b"{\"version\":1,\"connections\":[]}").unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
}

#[cfg(unix)]
#[test]
fn connection_document_rejects_group_or_other_readable_leaf_permissions() {
    use std::os::unix::fs::PermissionsExt;

    for unsafe_mode in [0o640, 0o604] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(format!("connections-{unsafe_mode:o}.json"));
        write_empty_document(&path);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(unsafe_mode)).unwrap();

        let result = load_connections(&path);

        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(
            result.unwrap_err(),
            "oauth-connection-document-permissions-unsafe",
            "mode {unsafe_mode:o} must not expose durable OAuth connection metadata"
        );
    }
}

#[cfg(unix)]
#[test]
fn connection_document_rejects_group_or_other_writable_parent_before_leaf_read() {
    use std::os::unix::fs::PermissionsExt;

    for writable_bit in [0o020, 0o002] {
        let temp = tempfile::tempdir().unwrap();
        let parent = temp.path().join(format!("oauth-parent-{writable_bit:o}"));
        std::fs::create_dir(&parent).unwrap();
        let path = parent.join("connections.json");
        write_empty_document(&path);
        std::fs::set_permissions(
            &parent,
            std::fs::Permissions::from_mode(0o700 | writable_bit),
        )
        .unwrap();

        let result = load_connections(&path);

        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(
            result.unwrap_err(),
            "oauth-connection-directory-writable-by-others",
            "writable bit {writable_bit:o} must fail before the document is read"
        );
    }
}
