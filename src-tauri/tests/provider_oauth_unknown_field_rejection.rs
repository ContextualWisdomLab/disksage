//! Fail-closed schema regressions for persisted OAuth connection metadata.
//!
//! Connection documents are versioned local authority metadata. Unknown fields must never be
//! silently discarded because that could reinterpret a newer or forged authority shape as the
//! currently supported schema.

use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::provider_oauth::{load_connections, requested_scope, OAuthConnection};
use sha2::{Digest, Sha256};

const GOOGLE_CLIENT_ID: &str = "1234567890-abcxyz.apps.googleusercontent.com";

fn make_private(path: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
}

fn write_private(path: &std::path::Path, value: serde_json::Value) {
    std::fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
    make_private(path);
}

fn root() -> CloudRoot {
    #[cfg(windows)]
    let path = r"C:\Cloud\GoogleDrive".to_string();
    #[cfg(not(windows))]
    let path = "/Cloud/GoogleDrive".to_string();

    CloudRoot {
        id: "google-drive:unknown-field-regression".into(),
        provider: CloudProvider::GoogleDrive,
        account_scope: CloudAccountScope::Unknown,
        label: "Google Drive".into(),
        path,
        readable: true,
        access_issue: None,
    }
}

fn connection_id(root: &CloudRoot) -> String {
    let mut hasher = Sha256::new();
    for value in [root.provider.as_str(), root.id.as_str(), root.path.as_str()] {
        hasher.update(value.as_bytes());
        hasher.update([0]);
    }
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn connection(root: &CloudRoot) -> OAuthConnection {
    OAuthConnection {
        connection_id: connection_id(root),
        provider: root.provider,
        cloud_root_id: root.id.clone(),
        cloud_root_path: root.path.clone(),
        client_id: GOOGLE_CLIENT_ID.into(),
        scope: requested_scope(root.provider).unwrap().into(),
        connected_at_ms: 1,
    }
}

#[test]
fn connection_document_rejects_unknown_top_level_authority_fields() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("connections.json");
    write_private(
        &path,
        serde_json::json!({
            "version": 1,
            "connections": [],
            "unexpected_authority": "must-not-be-ignored"
        }),
    );

    assert_eq!(
        load_connections(&path).unwrap_err(),
        "oauth-connection-document-invalid"
    );
}

#[test]
fn connection_document_rejects_unknown_nested_connection_authority_fields() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("connections.json");
    let root = root();
    let mut encoded = serde_json::to_value(connection(&root)).unwrap();
    encoded
        .as_object_mut()
        .unwrap()
        .insert("write_authority".into(), serde_json::Value::Bool(true));
    write_private(
        &path,
        serde_json::json!({
            "version": 1,
            "connections": [encoded]
        }),
    );

    assert_eq!(
        load_connections(&path).unwrap_err(),
        "oauth-connection-document-invalid"
    );
}
