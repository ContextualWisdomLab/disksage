//! Regression coverage for duplicate durable OAuth connection identities.
//!
//! A connection id is also the credential-store lookup key. Persisting the exact same id twice
//! makes one credential identity correspond to multiple connection records and defers the failure
//! until root lookup. The connection-document parser must reject that ambiguity at the durable
//! evidence boundary while continuing to allow distinct canonical/legacy ids for migration.

use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::provider_oauth::{load_connections, requested_scope, OAuthConnection};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use unicode_normalization::UnicodeNormalization;

const GOOGLE_CLIENT_ID: &str = "1234567890-abcxyz.apps.googleusercontent.com";

fn canonical_connection_id(root: &CloudRoot) -> String {
    let root_id = root.id.nfc().collect::<String>();
    let root_path = root.path.nfc().collect::<String>();
    let mut hasher = Sha256::new();
    for value in [root.provider.as_str(), root_id.as_str(), root_path.as_str()] {
        hasher.update(value.as_bytes());
        hasher.update([0]);
    }
    let digest = hasher.finalize();
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut encoded, "{byte:02x}").expect("writing to String cannot fail");
    }
    encoded
}

fn private_write(path: &std::path::Path, bytes: &[u8]) {
    std::fs::write(path, bytes).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
}

#[test]
fn duplicate_durable_connection_id_is_rejected_at_document_load() {
    let temp = tempfile::tempdir().unwrap();
    #[cfg(windows)]
    let path = r"C:\Cloud\Drive".to_string();
    #[cfg(not(windows))]
    let path = "/Cloud/Drive".to_string();
    let root = CloudRoot {
        id: "google-drive:account".into(),
        provider: CloudProvider::GoogleDrive,
        account_scope: CloudAccountScope::Unknown,
        label: "Google Drive".into(),
        path,
        readable: true,
        access_issue: None,
    };
    let connection = OAuthConnection {
        connection_id: canonical_connection_id(&root),
        provider: root.provider,
        cloud_root_id: root.id,
        cloud_root_path: root.path,
        client_id: GOOGLE_CLIENT_ID.into(),
        scope: requested_scope(CloudProvider::GoogleDrive).unwrap().into(),
        connected_at_ms: 123,
    };
    let document_path = temp.path().join("connections.json");
    private_write(
        &document_path,
        &serde_json::to_vec(&serde_json::json!({
            "version": 1,
            "connections": [connection.clone(), connection]
        }))
        .unwrap(),
    );

    assert_eq!(
        load_connections(&document_path).unwrap_err(),
        "oauth-connection-document-duplicate-id"
    );
}
