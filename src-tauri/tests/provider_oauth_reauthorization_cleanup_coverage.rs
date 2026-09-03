#![allow(dead_code, unused_imports)]

//! A successful canonical refresh-token write must not make failed legacy cleanup invisible.
//!
//! Reauthorization can migrate an NFC/NFD legacy connection to the canonical identifier. If
//! deleting a legacy keyring credential fails after the canonical token has been stored, the
//! durable document must retain a retry-visible legacy identity while preferring the canonical
//! connection for normal use. Already-deleted legacy entries may also remain as retry handles:
//! keyring `NoEntry` is idempotent success on the next cleanup attempt.

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

fn unrelated_google_root(index: usize) -> CloudRoot {
    #[cfg(windows)]
    let path = format!(r"C:\Cloud\unrelated-{index}");
    #[cfg(not(windows))]
    let path = format!("/Cloud/unrelated-{index}");
    CloudRoot {
        id: path.clone(),
        provider: CloudProvider::GoogleDrive,
        account_scope: crate::cloud::CloudAccountScope::Unknown,
        label: format!("Google Drive {index}"),
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
fn failed_legacy_cleanup_restores_a_retry_visible_identity_beside_the_canonical_connection() {
    let temp = tempfile::tempdir().unwrap();
    let document = temp.path().join("connections.json");
    let saved_root = unicode_google_root(true);
    let requested_root = unicode_google_root(false);
    let legacy = google_connection(&saved_root, legacy_connection_id(&saved_root), 100);
    let canonical = google_connection(&requested_root, connection_id(&requested_root), 200);
    assert_ne!(legacy.connection_id, canonical.connection_id);

    let original = vec![legacy.clone()];
    save_connections(&document, std::slice::from_ref(&canonical)).unwrap();

    let mut deleted = Vec::new();
    let error = cleanup_stale_authorization_credentials(
        &document,
        &requested_root,
        &original,
        &canonical,
        |connection_id| {
            deleted.push(connection_id.to_string());
            Err("provider-oauth-keyring-delete-failed".to_string())
        },
    )
    .unwrap_err();

    assert_eq!(error, "provider-oauth-keyring-delete-failed");
    assert_eq!(deleted, vec![legacy.connection_id.clone()]);
    let retry_visible = load_connections(&document).unwrap();
    assert!(retry_visible.contains(&legacy));
    assert!(retry_visible.contains(&canonical));
    assert_eq!(
        connection_for_root(&retry_visible, &requested_root).unwrap(),
        canonical,
        "normal use must continue to prefer the newly stored canonical credential while the stale identity remains available for cleanup retry"
    );
}

#[test]
fn successful_legacy_cleanup_keeps_the_published_document_canonical_only() {
    let temp = tempfile::tempdir().unwrap();
    let document = temp.path().join("connections.json");
    let saved_root = unicode_google_root(true);
    let requested_root = unicode_google_root(false);
    let legacy = google_connection(&saved_root, legacy_connection_id(&saved_root), 100);
    let canonical = google_connection(&requested_root, connection_id(&requested_root), 200);
    let original = vec![legacy.clone()];
    save_connections(&document, std::slice::from_ref(&canonical)).unwrap();

    let mut deleted = Vec::new();
    cleanup_stale_authorization_credentials(
        &document,
        &requested_root,
        &original,
        &canonical,
        |connection_id| {
            deleted.push(connection_id.to_string());
            Ok(())
        },
    )
    .unwrap();

    assert_eq!(deleted, vec![legacy.connection_id]);
    assert_eq!(load_connections(&document).unwrap(), vec![canonical]);
}

#[test]
fn no_stale_identity_never_calls_the_credential_delete_boundary() {
    let temp = tempfile::tempdir().unwrap();
    let document = temp.path().join("connections.json");
    let requested_root = unicode_google_root(false);
    let canonical = google_connection(&requested_root, connection_id(&requested_root), 200);
    let original = vec![canonical.clone()];
    save_connections(&document, std::slice::from_ref(&canonical)).unwrap();

    cleanup_stale_authorization_credentials(
        &document,
        &requested_root,
        &original,
        &canonical,
        |_| -> Result<(), String> { panic!("no stale credential may reach the delete boundary") },
    )
    .unwrap();

    assert_eq!(load_connections(&document).unwrap(), vec![canonical]);
}

#[test]
fn failed_retry_visibility_publication_is_reported_separately() {
    let temp = tempfile::tempdir().unwrap();
    let document = temp.path().join("connections.json");
    let saved_root = unicode_google_root(true);
    let requested_root = unicode_google_root(false);
    let legacy = google_connection(&saved_root, legacy_connection_id(&saved_root), 100);
    let canonical = google_connection(&requested_root, connection_id(&requested_root), 200);
    let original = vec![legacy];
    save_connections(&document, std::slice::from_ref(&canonical)).unwrap();

    std::fs::remove_file(&document).unwrap();
    std::fs::create_dir(&document).unwrap();
    let error = cleanup_stale_authorization_credentials(
        &document,
        &requested_root,
        &original,
        &canonical,
        |_| Err("provider-oauth-keyring-delete-failed".to_string()),
    )
    .unwrap_err();

    assert_eq!(
        error,
        "provider-oauth-keyring-delete-and-config-recovery-failed"
    );
    assert!(
        document.is_dir(),
        "recovery publication must fail closed rather than mutate a non-regular authority path"
    );
}

#[test]
fn legacy_only_migration_reserves_capacity_for_retry_visible_recovery() {
    let saved_root = unicode_google_root(true);
    let requested_root = unicode_google_root(false);
    let legacy = google_connection(&saved_root, legacy_connection_id(&saved_root), 100);
    let canonical = google_connection(&requested_root, connection_id(&requested_root), 200);
    let mut full_without_canonical = vec![legacy];
    for index in 0..(MAX_CONNECTIONS - 1) {
        let unrelated = unrelated_google_root(index);
        full_without_canonical.push(google_connection(
            &unrelated,
            connection_id(&unrelated),
            1_000 + index as u64,
        ));
    }
    assert_eq!(full_without_canonical.len(), MAX_CONNECTIONS);

    assert_eq!(
        ensure_reauthorization_cleanup_capacity(
            &full_without_canonical,
            &requested_root,
            &canonical,
        )
        .unwrap_err(),
        "provider-oauth-reauthorization-recovery-capacity-exhausted"
    );

    let mut with_room = full_without_canonical.clone();
    with_room.pop();
    assert!(
        ensure_reauthorization_cleanup_capacity(&with_room, &requested_root, &canonical).is_ok()
    );

    let mut full_with_canonical = with_room;
    full_with_canonical.push(canonical.clone());
    assert_eq!(full_with_canonical.len(), MAX_CONNECTIONS);
    assert!(ensure_reauthorization_cleanup_capacity(
        &full_with_canonical,
        &requested_root,
        &canonical,
    )
    .is_ok());
}
