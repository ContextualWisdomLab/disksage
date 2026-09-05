//! Unix authority-chain regression for durable provider OAuth connection metadata.
//!
//! A private immediate app-data directory is not sufficient authority when a non-sticky
//! group/other-writable ancestor can replace that directory entry. This test exercises the real
//! public `load_connections` filesystem boundary without browser, network, keyring, or provider
//! mutation.

#![cfg(unix)]

use disksage_lib::provider_oauth::load_connections;
use std::os::unix::fs::PermissionsExt;

#[test]
fn non_sticky_shared_writable_ancestor_never_authorizes_connection_metadata() {
    let temp = tempfile::tempdir().unwrap();
    let shared_ancestor = temp.path().join("shared-ancestor");
    let private_parent = shared_ancestor.join("app-data");
    std::fs::create_dir_all(&private_parent).unwrap();

    std::fs::set_permissions(
        &shared_ancestor,
        std::fs::Permissions::from_mode(0o770),
    )
    .unwrap();
    std::fs::set_permissions(&private_parent, std::fs::Permissions::from_mode(0o700)).unwrap();

    let document = private_parent.join("cloud-oauth-connections.json");
    std::fs::write(&document, b"{\"version\":1,\"connections\":[]}").unwrap();
    std::fs::set_permissions(&document, std::fs::Permissions::from_mode(0o600)).unwrap();

    assert_eq!(
        load_connections(&document).unwrap_err(),
        "oauth-connection-directory-writable-by-others",
        "a replaceable private child must not inherit authority through a non-sticky shared-writable ancestor"
    );
}
