//! Unix filesystem-authority coverage for OAuth connection metadata persistence.
//!
//! The connection registry selects the local OAuth metadata associated with refresh-token
//! credentials. Its immediate parent therefore must not give group or other principals directory
//! entry replacement authority, even when the document itself is a private regular file.

#[cfg(unix)]
#[test]
fn connection_document_rejects_group_and_other_writable_parent() {
    use disksage_lib::provider_oauth::load_connections;
    use std::os::unix::fs::PermissionsExt;

    for writable_bit in [0o020, 0o002] {
        let temp = tempfile::tempdir().unwrap();
        let parent = temp.path().join(format!("oauth-parent-{writable_bit:o}"));
        std::fs::create_dir(&parent).unwrap();
        let path = parent.join("connections.json");
        let original = b"{\"version\":1,\"connections\":[]}";
        std::fs::write(&path, original).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
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
            "Unix connection metadata must fail closed when parent mode includes {writable_bit:o}"
        );
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }
}
