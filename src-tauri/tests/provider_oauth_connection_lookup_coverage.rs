use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::provider_oauth::{connection_for_root, OAuthConnection};

fn onedrive_root(id: &str, path: &str) -> CloudRoot {
    CloudRoot {
        id: id.into(),
        provider: CloudProvider::Onedrive,
        account_scope: CloudAccountScope::Unknown,
        label: "OneDrive".into(),
        path: path.into(),
        readable: true,
        access_issue: None,
    }
}

fn canonical_connection() -> OAuthConnection {
    OAuthConnection {
        connection_id: "20ce9ca07d014bcf578cd0e494f9278fa2ad69e7e62e5dbd9afc5fe30bf7e7eb".into(),
        provider: CloudProvider::Onedrive,
        cloud_root_id: "root".into(),
        cloud_root_path: "/tmp/root".into(),
        client_id: "12345678-1234-1234-1234-123456789abc".into(),
        scope: "Files.Read offline_access".into(),
        connected_at_ms: 1,
    }
}

#[test]
fn connection_lookup_prefers_one_exact_identity_and_fails_closed_on_ambiguity_or_absence() {
    let root = onedrive_root("root", "/tmp/root");
    let connection = canonical_connection();

    assert_eq!(
        connection_for_root(std::slice::from_ref(&connection), &root).unwrap(),
        connection
    );
    assert_eq!(
        connection_for_root(&[], &root).unwrap_err(),
        "provider-oauth-connection-missing"
    );

    let duplicate = canonical_connection();
    assert_eq!(
        connection_for_root(&[canonical_connection(), duplicate], &root).unwrap_err(),
        "provider-oauth-connection-ambiguous"
    );

    let other_root = onedrive_root("other-root", "/tmp/other-root");
    assert_eq!(
        connection_for_root(&[canonical_connection()], &other_root).unwrap_err(),
        "provider-oauth-connection-missing"
    );
}
