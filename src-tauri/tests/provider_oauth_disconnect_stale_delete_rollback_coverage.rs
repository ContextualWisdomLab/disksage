#![allow(dead_code, unused_imports)]

//! A disconnect must not delete any canonical or stale refresh token unless the durable connection
//! document can first publish the corresponding state transition. With object-bound replacement
//! unavailable, refusal at the document boundary is the recovery mechanism: credentials and the
//! accepted document both remain unchanged.

#[path = "../src/object_bound_publication.rs"]
mod object_bound_publication;
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
fn replacement_refusal_precedes_stale_and_canonical_credential_deletion() {
    let temp = tempfile::tempdir().unwrap();
    let document = temp.path().join("connections.json");
    let saved_root = unicode_google_root(true);
    let requested_root = unicode_google_root(false);

    let legacy = google_connection(&saved_root, legacy_connection_id(&saved_root), 100);
    let current = google_connection(&saved_root, connection_id(&saved_root), 200);
    assert_ne!(legacy.connection_id, current.connection_id);
    let original = vec![legacy, current];
    save_connections(&document, &original).unwrap();
    let before = std::fs::read(&document).unwrap();

    let mut deleted = Vec::new();
    let error = disconnect_with_delete(&document, &requested_root, |connection_id| {
        deleted.push(connection_id.to_string());
        Err("provider-oauth-keyring-delete-failed".to_string())
    })
    .unwrap_err();

    assert_eq!(
        error,
        "oauth-connection-document-object-bound-replacement-unavailable"
    );
    assert!(
        deleted.is_empty(),
        "no keyring credential may be mutated before durable document replacement is authorized"
    );
    assert_eq!(std::fs::read(&document).unwrap(), before);
    assert_eq!(load_connections(&document).unwrap(), original);
}
