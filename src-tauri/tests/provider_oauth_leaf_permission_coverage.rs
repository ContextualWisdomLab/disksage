//! Unix privacy-boundary coverage for durable OAuth connection metadata.
//!
//! A connection document contains provider, client, scope, and cloud-root identity metadata.
//! DiskSage creates that record as exact mode 0600 on Unix. Loading must therefore reject not only
//! group/other exposure but any owner-mode or special-bit drift from the admitted durable mode.

#[cfg(unix)]
#[test]
fn connection_document_requires_exact_private_leaf_permissions() {
    use disksage_lib::provider_oauth::load_connections;
    use std::os::unix::fs::PermissionsExt;

    for mode in [
        0o400, // owner-write bit removed
        0o700, // unexpected owner execute bit
        0o640, 0o604, 0o620, 0o602, // group/other exposure or mutation
        0o4600, 0o2600, 0o1600, // setuid, setgid, sticky drift
    ] {
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
            "mode {mode:o} must not be admitted as exact private OAuth metadata"
        );
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }
}
