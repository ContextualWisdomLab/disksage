//! Credential-free filesystem-authority coverage for persisted OAuth connection metadata.
//!
//! These regressions exercise the public read boundary against real Unix permissions. They never
//! launch a browser, contact a provider, use the credential store, or create cloud-side state.

use disksage_lib::provider_oauth::load_connections;

#[cfg(unix)]
fn set_mode(path: &std::path::Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

#[cfg(unix)]
#[test]
fn connection_document_rejects_group_or_other_access_at_the_leaf_before_parsing() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("connections.json");
    std::fs::write(&path, br#"{"version":1,"connections":[]}"#).unwrap();

    for mode in [0o640, 0o602] {
        set_mode(&path, mode);
        assert_eq!(
            load_connections(&path).unwrap_err(),
            "oauth-connection-document-permissions-unsafe",
            "mode {mode:o} must not gain connection-document read authority"
        );
    }
}

#[cfg(unix)]
#[test]
fn connection_document_rejects_group_or_other_writable_parent_before_observing_a_leaf() {
    let temp = tempfile::tempdir().unwrap();

    for (name, mode) in [("group-writable", 0o720), ("other-writable", 0o702)] {
        let parent = temp.path().join(name);
        std::fs::create_dir(&parent).unwrap();
        set_mode(&parent, mode);

        assert_eq!(
            load_connections(&parent.join("connections.json")).unwrap_err(),
            "oauth-connection-directory-writable-by-others",
            "mode {mode:o} must fail before a missing leaf can be treated as an empty document"
        );
    }
}
