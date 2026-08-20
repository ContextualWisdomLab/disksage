//! Credential-free migration coverage for persisted OAuth connection identity.
//!
//! Version 1 documents may contain a legacy hash of a decomposed macOS File Provider path while
//! current DiskSage normalizes the same logical root before hashing. These tests keep lookup
//! deterministic without touching a browser, credential store, provider API, or network socket.

use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::provider_oauth::{connection_for_root, load_connections, requested_scope, OAuthConnection};
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

const GOOGLE_CLIENT_ID: &str = "1234567890-abcxyz.apps.googleusercontent.com";

fn google_root(path: String) -> CloudRoot {
    CloudRoot {
        id: path.clone(),
        provider: CloudProvider::GoogleDrive,
        account_scope: CloudAccountScope::Unknown,
        label: "Google Drive".into(),
        path,
        readable: true,
        access_issue: None,
    }
}

fn connection_id(provider: CloudProvider, root_id: &str, root_path: &str, normalize: bool) -> String {
    let root_id = if normalize {
        root_id.nfc().collect::<String>()
    } else {
        root_id.to_owned()
    };
    let root_path = if normalize {
        root_path.nfc().collect::<String>()
    } else {
        root_path.to_owned()
    };
    let mut hasher = Sha256::new();
    for value in [provider.as_str(), root_id.as_str(), root_path.as_str()] {
        hasher.update(value.as_bytes());
        hasher.update([0]);
    }
    format!("{:x}", hasher.finalize())
}

fn connection(root: &CloudRoot, normalize_id: bool) -> OAuthConnection {
    OAuthConnection {
        connection_id: connection_id(root.provider, &root.id, &root.path, normalize_id),
        provider: root.provider,
        cloud_root_id: root.id.clone(),
        cloud_root_path: root.path.clone(),
        client_id: GOOGLE_CLIENT_ID.into(),
        scope: requested_scope(root.provider).unwrap().into(),
        connected_at_ms: 123,
    }
}

fn write_private(path: &std::path::Path, bytes: impl AsRef<[u8]>) {
    std::fs::write(path, bytes).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
}

#[test]
fn valid_looking_but_unbound_connection_id_is_rejected_at_document_load() {
    let temp = tempfile::tempdir().unwrap();
    #[cfg(windows)]
    let root = google_root(r"C:\Cloud\Drive".into());
    #[cfg(not(windows))]
    let root = google_root("/Cloud/Drive".into());
    let mut forged = connection(&root, true);
    forged.connection_id = "0".repeat(64);
    let path = temp.path().join("connections.json");
    write_private(
        &path,
        serde_json::to_vec(&serde_json::json!({
            "version": 1,
            "connections": [forged]
        }))
        .unwrap(),
    );

    assert_eq!(
        load_connections(&path).unwrap_err(),
        "oauth-connection-id-mismatch"
    );
}

#[test]
fn canonical_identity_wins_when_legacy_unicode_record_is_also_present() {
    #[cfg(windows)]
    let composed_path = r"C:\Cloud\내 드라이브";
    #[cfg(not(windows))]
    let composed_path = "/Cloud/내 드라이브";
    let decomposed_path = composed_path.nfd().collect::<String>();
    assert_ne!(composed_path, decomposed_path);

    let requested = google_root(composed_path.to_owned());
    let legacy_root = google_root(decomposed_path);
    let legacy = connection(&legacy_root, false);
    let canonical = connection(&requested, true);
    assert_ne!(legacy.connection_id, canonical.connection_id);

    assert_eq!(
        connection_for_root(&[legacy.clone(), canonical.clone()], &requested).unwrap(),
        canonical,
        "one exact canonical record must be preferred over an otherwise equivalent legacy record"
    );
    assert_eq!(
        connection_for_root(&[legacy.clone(), legacy], &requested).unwrap_err(),
        "provider-oauth-connection-ambiguous",
        "multiple legacy-equivalent records must fail closed when no canonical record disambiguates them"
    );
}
