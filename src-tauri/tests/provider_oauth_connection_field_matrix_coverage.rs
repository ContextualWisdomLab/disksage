//! Public-boundary coverage for durable OAuth connection field admission and root matching.
//!
//! These regressions exercise the real connection-document parser and root lookup with malformed
//! authority metadata. They use only private temporary files and never launch a browser, contact a
//! provider, access the credential store, or mutate a cloud provider.

use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::provider_oauth::{
    connection_for_root, connections_path, load_connections, requested_scope, OAuthConnection,
};
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

const GOOGLE_CLIENT_ID: &str = "1234567890-abcxyz.apps.googleusercontent.com";

fn google_root() -> CloudRoot {
    #[cfg(windows)]
    let path = r"C:\Cloud\Google Drive";
    #[cfg(not(windows))]
    let path = "/Cloud/Google Drive";

    CloudRoot {
        id: "google-drive:field-matrix".into(),
        provider: CloudProvider::GoogleDrive,
        account_scope: CloudAccountScope::Unknown,
        label: "Google Drive".into(),
        path: path.into(),
        readable: true,
        access_issue: None,
    }
}

fn canonical_connection_id(root: &CloudRoot) -> String {
    let root_id = root.id.nfc().collect::<String>();
    let root_path = root.path.nfc().collect::<String>();
    let mut hasher = Sha256::new();
    for value in [root.provider.as_str(), root_id.as_str(), root_path.as_str()] {
        hasher.update(value.as_bytes());
        hasher.update([0]);
    }
    use std::fmt::Write as _;
    let digest = hasher.finalize();
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut encoded, "{byte:02x}").expect("writing to String cannot fail");
    }
    encoded
}

fn valid_connection(root: &CloudRoot) -> OAuthConnection {
    OAuthConnection {
        connection_id: canonical_connection_id(root),
        provider: root.provider,
        cloud_root_id: root.id.clone(),
        cloud_root_path: root.path.clone(),
        client_id: GOOGLE_CLIENT_ID.into(),
        scope: requested_scope(root.provider).unwrap().into(),
        connected_at_ms: 1,
    }
}

fn write_private(path: &std::path::Path, connection: &OAuthConnection) {
    std::fs::write(
        path,
        serde_json::to_vec(&serde_json::json!({
            "version": 1,
            "connections": [connection]
        }))
        .unwrap(),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
}

#[test]
fn malformed_connection_fields_fail_closed_at_the_document_boundary() {
    let temp = tempfile::tempdir().unwrap();
    let root = google_root();
    let valid = valid_connection(&root);

    let mut short_id = valid.clone();
    short_id.connection_id = "0".repeat(63);

    let mut blank_root_id = valid.clone();
    blank_root_id.cloud_root_id = "   ".into();

    let mut relative_root_path = valid.clone();
    relative_root_path.cloud_root_path = "relative/Google Drive".into();

    let mut wrong_scope = valid.clone();
    wrong_scope.scope = "https://www.googleapis.com/auth/drive.file".into();

    let mut wrong_provider_client_shape = valid.clone();
    wrong_provider_client_shape.client_id = "bad_prefix.apps.googleusercontent.com".into();

    let mut whitespace_client = valid;
    whitespace_client.client_id = format!(" {GOOGLE_CLIENT_ID}");

    for (index, (candidate, expected)) in [
        (short_id, "oauth-connection-invalid"),
        (blank_root_id, "oauth-connection-invalid"),
        (relative_root_path, "oauth-connection-invalid"),
        (wrong_scope, "oauth-connection-invalid"),
        (
            wrong_provider_client_shape,
            "oauth-client-id-provider-format-invalid",
        ),
        (whitespace_client, "oauth-client-id-invalid"),
    ]
    .into_iter()
    .enumerate()
    {
        let path = temp.path().join(format!("candidate-{index}.json"));
        write_private(&path, &candidate);
        assert_eq!(load_connections(&path).unwrap_err(), expected);
    }
}

#[test]
fn root_lookup_requires_same_provider_identity_and_path() {
    let root = google_root();
    let connection = valid_connection(&root);

    assert_eq!(
        connection_for_root(std::slice::from_ref(&connection), &root).unwrap(),
        connection
    );

    let mut wrong_id = root.clone();
    wrong_id.id.push_str("-other");
    assert_eq!(
        connection_for_root(std::slice::from_ref(&connection), &wrong_id).unwrap_err(),
        "provider-oauth-connection-missing"
    );

    let mut wrong_path = root;
    wrong_path.path.push_str("-other");
    assert_eq!(
        connection_for_root(std::slice::from_ref(&connection), &wrong_path).unwrap_err(),
        "provider-oauth-connection-missing"
    );
}

#[test]
fn public_scope_and_connection_document_path_contracts_are_provider_exact() {
    assert_eq!(
        requested_scope(CloudProvider::Onedrive).unwrap(),
        "Files.Read offline_access"
    );
    assert_eq!(
        requested_scope(CloudProvider::GoogleDrive).unwrap(),
        "https://www.googleapis.com/auth/drive.metadata.readonly"
    );
    assert_eq!(
        requested_scope(CloudProvider::Icloud).unwrap_err(),
        "icloud-oauth-not-supported"
    );

    let base = std::path::Path::new("app-data");
    assert_eq!(
        connections_path(base),
        base.join("cloud-oauth-connections.json")
    );
}
