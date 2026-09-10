//! Public-boundary coverage for OAuth connection root matching and persisted client-ID admission.
//!
//! These regressions cover authority branches not exercised by the broader connection-document
//! matrix: whitespace-tainted provider credentials propagated through persistence validation,
//! same-provider roots whose filesystem path no longer matches, and bare relative document names
//! whose authority parent is the current directory. They never launch a browser, contact a
//! provider, access the credential store, or mutate a cloud provider.

use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::provider_oauth::{
    connection_for_root, load_connections, requested_scope, OAuthConnection,
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
fn persisted_whitespace_tainted_client_id_fails_closed() {
    let temp = tempfile::tempdir().unwrap();
    let root = google_root();
    let mut connection = valid_connection(&root);
    connection.client_id = format!(" {GOOGLE_CLIENT_ID}");
    let path = temp.path().join("connections.json");
    write_private(&path, &connection);

    assert_eq!(
        load_connections(&path).unwrap_err(),
        "oauth-client-id-invalid",
        "durable metadata must propagate common client-ID admission before lookup"
    );
}

#[test]
fn same_provider_root_lookup_requires_the_persisted_filesystem_path() {
    let root = google_root();
    let connection = valid_connection(&root);
    assert_eq!(
        connection_for_root(std::slice::from_ref(&connection), &root).unwrap(),
        connection
    );

    let mut moved_root = root;
    moved_root.path.push_str("-moved");
    assert_eq!(
        connection_for_root(std::slice::from_ref(&connection), &moved_root).unwrap_err(),
        "provider-oauth-connection-missing",
        "same-provider identity alone must not authorize a different filesystem root"
    );
}

#[test]
fn missing_bare_document_name_uses_current_directory_authority_without_creation() {
    let path = std::path::PathBuf::from(format!(
        ".disksage-oauth-missing-{}.json",
        std::process::id()
    ));
    assert!(
        !path.exists(),
        "coverage fixture name must not collide with a repository file"
    );
    assert!(
        load_connections(&path).unwrap().is_empty(),
        "a missing bare filename must remain a non-authorizing empty document"
    );
    assert!(
        !path.exists(),
        "read-only connection lookup must not create the missing document"
    );
}
