//! Public-boundary capacity coverage for durable provider OAuth connection metadata.
//!
//! The persisted document deliberately allows at most 32 connections. This regression exercises
//! the exact accepted boundary with distinct, valid records and proves that the 33rd record is
//! rejected before any record can gain lookup authority. It performs only local file I/O; it does
//! not open a browser, contact a provider, access the credential store, or mutate cloud state.

use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::provider_oauth::{connection_for_root, load_connections, requested_scope, OAuthConnection};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use unicode_normalization::UnicodeNormalization;

const GOOGLE_CLIENT_ID: &str = "1234567890-abcxyz.apps.googleusercontent.com";
const MAX_CONNECTIONS: usize = 32;

fn root(index: usize) -> CloudRoot {
    #[cfg(windows)]
    let path = format!(r"C:\Cloud\Drive-{index:02}");
    #[cfg(not(windows))]
    let path = format!("/Cloud/Drive-{index:02}");

    CloudRoot {
        id: format!("google-drive:account-{index:02}"),
        provider: CloudProvider::GoogleDrive,
        account_scope: CloudAccountScope::Unknown,
        label: format!("Google Drive {index:02}"),
        path,
        readable: true,
        access_issue: None,
    }
}

fn connection_id(root: &CloudRoot) -> String {
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

fn connection(root: &CloudRoot, connected_at_ms: u64) -> OAuthConnection {
    OAuthConnection {
        connection_id: connection_id(root),
        provider: root.provider,
        cloud_root_id: root.id.clone(),
        cloud_root_path: root.path.clone(),
        client_id: GOOGLE_CLIENT_ID.into(),
        scope: requested_scope(root.provider)
            .expect("Google Drive has a fixed read-only OAuth scope")
            .into(),
        connected_at_ms,
    }
}

fn write_private(path: &std::path::Path, bytes: &[u8]) {
    std::fs::write(path, bytes).expect("write connection document fixture");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .expect("make OAuth metadata fixture owner-private");
    }
}

#[test]
fn exact_connection_capacity_remains_usable_and_the_next_record_fails_closed() {
    let temp = tempfile::tempdir().expect("create isolated app-data directory");
    let path = temp.path().join("cloud-oauth-connections.json");

    let roots: Vec<_> = (0..MAX_CONNECTIONS).map(root).collect();
    let connections: Vec<_> = roots
        .iter()
        .enumerate()
        .map(|(index, root)| connection(root, 1_000 + index as u64))
        .collect();
    write_private(
        &path,
        &serde_json::to_vec(&serde_json::json!({
            "version": 1,
            "connections": connections,
        }))
        .expect("serialize exact-capacity fixture"),
    );

    let loaded = load_connections(&path).expect("the documented 32-record capacity must be usable");
    assert_eq!(loaded.len(), MAX_CONNECTIONS);
    for (index, root) in roots.iter().enumerate() {
        let selected = connection_for_root(&loaded, root)
            .expect("every admitted record must remain addressable by its exact root");
        assert_eq!(selected.connection_id, connection_id(root));
        assert_eq!(selected.connected_at_ms, 1_000 + index as u64);
    }

    let overflow_root = root(MAX_CONNECTIONS);
    let mut overflow = loaded;
    overflow.push(connection(&overflow_root, 2_000));
    write_private(
        &path,
        &serde_json::to_vec(&serde_json::json!({
            "version": 1,
            "connections": overflow,
        }))
        .expect("serialize over-capacity fixture"),
    );

    assert_eq!(
        load_connections(&path).expect_err("the 33rd record must never gain lookup authority"),
        "oauth-connection-document-version-or-count-invalid"
    );
}
