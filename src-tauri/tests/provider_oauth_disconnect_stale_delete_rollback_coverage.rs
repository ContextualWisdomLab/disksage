#![allow(dead_code, unused_imports)]

//! A disconnect must not report success when a legacy credential delete fails after the primary
//! credential has already been removed. The durable connection document is the retry handle for
//! finishing cleanup, so the real transactional helper must restore it on any matching-token
//! deletion failure rather than silently leaving an unreachable stale refresh token behind.

include!("../src/provider_oauth.rs");

mod cloud {
    pub use disksage_lib::cloud::*;
}

fn unicode_google_root(decomposed: bool) -> CloudRoot {
    #[cfg(windows)]
    let composed = r"C:\Cloud\내 드라이브";
    #[cfg(not(windows))]
    let composed = "/Cloud/내 드라이브";
    let path = if decomposed {
        composed.nfd().collect::<String>()
    } else {
        composed.to_string()
    };
    CloudRoot {
        id: path.clone(),
        provider: CloudProvider::GoogleDrive,
        account_scope: crate::cloud::CloudAccountScope::Unknown,
        label: "Google Drive".into(),
        path,
        readable: true,
        access_issue: None,
    }
}

fn google_connection(
    root: &CloudRoot,
    connection_id: String,
    connected_at_ms: u64,
) -> OAuthConnection {
    OAuthConnection {
        connection_id,
        provider: root.provider,
        cloud_root_id: root.id.clone(),
        cloud_root_path: root.path.clone(),
        client_id: "1234567890-abcxyz.apps.googleusercontent.com".into(),
        scope: requested_scope(root.provider).unwrap().into(),
        connected_at_ms,
    }
}

#[test]
fn stale_credential_delete_failure_restores_connection_document_for_retry() {
    let temp = tempfile::tempdir().unwrap();
    let document = temp.path().join("connections.json");
    let saved_root = unicode_google_root(true);
    let requested_root = unicode_google_root(false);

    let legacy = google_connection(&saved_root, legacy_connection_id(&saved_root), 100);
    let current = google_connection(&saved_root, connection_id(&saved_root), 200);
    assert_ne!(legacy.connection_id, current.connection_id);
    let original = vec![legacy.clone(), current.clone()];
    save_connections(&document, &original).unwrap();

    let mut deleted = Vec::new();
    let error = disconnect_with_delete(&document, &requested_root, |connection_id| {
        deleted.push(connection_id.to_string());
        if connection_id == legacy.connection_id {
            Err("provider-oauth-keyring-delete-failed".to_string())
        } else {
            Ok(())
        }
    })
    .unwrap_err();

    assert_eq!(error, "provider-oauth-keyring-delete-failed");
    assert_eq!(deleted, vec![current.connection_id, legacy.connection_id]);
    assert_eq!(
        load_connections(&document).unwrap(),
        original,
        "a partial credential cleanup must restore durable connection state so a retry can finish deleting every matching credential"
    );
}
