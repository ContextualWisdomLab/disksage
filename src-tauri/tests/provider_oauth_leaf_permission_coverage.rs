//! Unix privacy-boundary coverage for durable OAuth connection metadata.
//!
//! A connection document contains provider, client, scope, and cloud-root identity metadata.
//! DiskSage writes new documents as mode 0600 on Unix, so loading a pre-existing document must
//! fail closed if any group or other permission bit exposes or weakens that local metadata.

#[cfg(unix)]
#[test]
fn connection_document_requires_private_leaf_permissions() {
    use disksage_lib::provider_oauth::load_connections;
    use std::os::unix::fs::PermissionsExt;

    for mode in [0o640, 0o604, 0o620, 0o602] {
        let temp = tempfile::tempdir().unwrap();
        let parent = temp.path().join(format!("oauth-private-parent-{mode:o}"));
        std::fs::create_dir(&parent).unwrap();
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700)).unwrap();
        let path = parent.join("connections.json");
        let original = b"{\"version\":1,\"connections\":[]}";
        std::fs::write(&path, original).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();

        assert_eq!(
            load_connections(&path).unwrap_err(),
            "oauth-connection-document-permissions-unsafe",
            "mode {mode:o} must not be admitted as private OAuth metadata"
        );
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }
}
