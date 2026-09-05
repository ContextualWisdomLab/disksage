#![allow(dead_code, unused_imports)]

//! Disconnecting one canonical File Provider root must remove every durable canonical/legacy
//! record that identifies that same root. Leaving a legacy record behind would preserve a local
//! connection and credential lookup path after the user was told the provider was disconnected.

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
fn disconnect_removes_every_canonical_and_legacy_record_for_the_same_root() {
    let temp = tempfile::tempdir().unwrap();
    let document = temp.path().join("connections.json");
    let saved_root = unicode_google_root(true);
    let requested_root = unicode_google_root(false);

    let legacy = google_connection(&saved_root, legacy_connection_id(&saved_root), 100);
    let current = google_connection(&saved_root, connection_id(&saved_root), 200);
    assert_ne!(legacy.connection_id, current.connection_id);
    save_connections(&document, &[legacy.clone(), current.clone()]).unwrap();

    let mut deleted = Vec::new();
    disconnect_with_delete(&document, &requested_root, |connection_id| {
        deleted.push(connection_id.to_string());
        Ok(())
    })
    .unwrap();

    assert!(load_connections(&document).unwrap().is_empty());
    assert_eq!(deleted, vec![legacy.connection_id, current.connection_id]);
}
