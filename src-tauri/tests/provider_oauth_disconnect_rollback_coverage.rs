#![allow(dead_code, unused_imports)]

//! Credential-store failure must not silently discard durable OAuth connection state.
//!
//! Compile the production module into this integration-test crate so the real private transactional
//! boundary can be exercised without widening the shipped API or contacting a provider/keyring.

include!("../src/provider_oauth.rs");

mod cloud {
    pub use disksage_lib::cloud::*;
}

fn google_root() -> CloudRoot {
    #[cfg(windows)]
    let path = r"C:\Cloud\Rollback";
    #[cfg(not(windows))]
    let path = "/Cloud/Rollback";

    CloudRoot {
        id: "google-drive:rollback-account".into(),
        provider: CloudProvider::GoogleDrive,
        account_scope: crate::cloud::CloudAccountScope::Unknown,
        label: "Google Drive".into(),
        path: path.into(),
        readable: true,
        access_issue: None,
    }
}

fn connection(root: &CloudRoot) -> OAuthConnection {
    OAuthConnection {
        connection_id: connection_id(root),
        provider: root.provider,
        cloud_root_id: root.id.clone(),
        cloud_root_path: root.path.clone(),
        client_id: "1234567890-abcxyz.apps.googleusercontent.com".into(),
        scope: requested_scope(root.provider).unwrap().into(),
        connected_at_ms: 123,
    }
}

#[test]
fn credential_delete_failure_restores_the_original_connection_document() {
    let temp = tempfile::tempdir().unwrap();
    let document = temp.path().join("connections.json");
    let root = google_root();
    let original = connection(&root);
    save_connections(&document, std::slice::from_ref(&original)).unwrap();

    let mut delete_calls = 0usize;
    let error = disconnect_with_delete(&document, &root, |connection_id| {
        delete_calls += 1;
        assert_eq!(connection_id, original.connection_id);
        Err("provider-oauth-keyring-delete-failed".to_string())
    })
    .unwrap_err();

    assert_eq!(error, "provider-oauth-keyring-delete-failed");
    assert_eq!(delete_calls, 1, "the primary credential delete is attempted once");
    assert_eq!(
        load_connections(&document).unwrap(),
        vec![original],
        "failed credential deletion must roll the durable connection document back to its original state"
    );
}

#[test]
fn successful_credential_delete_commits_an_empty_durable_connection_document() {
    let temp = tempfile::tempdir().unwrap();
    let document = temp.path().join("connections.json");
    let root = google_root();
    let original = connection(&root);
    save_connections(&document, std::slice::from_ref(&original)).unwrap();

    let mut deleted = Vec::new();
    disconnect_with_delete(&document, &root, |connection_id| {
        deleted.push(connection_id.to_string());
        Ok(())
    })
    .unwrap();

    assert_eq!(
        deleted,
        vec![original.connection_id],
        "a successful disconnect deletes exactly the credential bound to the selected canonical connection"
    );
    assert!(
        document.is_file(),
        "disconnect keeps a valid durable versioned document rather than deleting the evidence path"
    );
    assert!(
        load_connections(&document).unwrap().is_empty(),
        "successful credential deletion must commit the matching connection removal"
    );
}
