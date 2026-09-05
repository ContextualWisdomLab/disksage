//! Unix ancestor-authority regression for durable OAuth connection metadata.
//!
//! Rejecting a symlink only at the leaf is insufficient: a symlinked application-data ancestor
//! can redirect a syntactically ordinary connection path to attacker-controlled storage.

#[cfg(unix)]
#[test]
fn connection_document_rejects_symlinked_parent_ancestor() {
    use disksage_lib::provider_oauth::load_connections;
    use std::os::unix::fs::{symlink, PermissionsExt};

    let temp = tempfile::tempdir().unwrap();
    let outside = temp.path().join("outside");
    let nested = outside.join("nested");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::set_permissions(&outside, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::set_permissions(&nested, std::fs::Permissions::from_mode(0o700)).unwrap();
    let document = nested.join("connections.json");
    let original = b"{\"version\":1,\"connections\":[]}";
    std::fs::write(&document, original).unwrap();
    std::fs::set_permissions(&document, std::fs::Permissions::from_mode(0o600)).unwrap();

    let alias = temp.path().join("app-data-alias");
    symlink(&outside, &alias).unwrap();
    let path = alias.join("nested").join("connections.json");

    assert_eq!(
        load_connections(&path).unwrap_err(),
        "oauth-connection-directory-unsafe"
    );
    assert_eq!(std::fs::read(&document).unwrap(), original);
}
