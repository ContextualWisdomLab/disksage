#![allow(dead_code, unused_imports)]

//! Consumer contract for provider-OAuth connection-document updates.
//!
//! The OAuth bounded context must not treat a pathname-validated atomic rename as sufficient
//! authority to replace an existing credential-adjacent record. Until the reusable filesystem
//! owner can bind final publication to the exact reviewed source object, an update must fail
//! closed, preserve the accepted document byte-for-byte, and create no staging pathname.

#[path = "../src/object_bound_publication.rs"]
mod object_bound_publication;
include!("../src/provider_oauth.rs");

mod cloud {
    pub use disksage_lib::cloud::*;
}

fn google_root() -> CloudRoot {
    #[cfg(windows)]
    let path = r"C:\Cloud\replacement-unavailable".to_string();
    #[cfg(not(windows))]
    let path = "/Cloud/replacement-unavailable".to_string();

    CloudRoot {
        id: "google-drive:replacement-unavailable".into(),
        provider: CloudProvider::GoogleDrive,
        account_scope: crate::cloud::CloudAccountScope::Unknown,
        label: "Google Drive".into(),
        path,
        readable: true,
        access_issue: None,
    }
}

fn connection(connected_at_ms: u64) -> OAuthConnection {
    let root = google_root();
    OAuthConnection {
        connection_id: connection_id(&root),
        provider: root.provider,
        cloud_root_id: root.id,
        cloud_root_path: root.path,
        client_id: "1234567890-abcxyz.apps.googleusercontent.com".into(),
        scope: requested_scope(CloudProvider::GoogleDrive).unwrap().into(),
        connected_at_ms,
    }
}

#[cfg(unix)]
#[test]
fn existing_document_update_fails_closed_without_mutating_prior_bytes_or_staging_names() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().expect("tempdir");
    let parent = temp.path().join("oauth");
    std::fs::create_dir(&parent).expect("create parent");
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700))
        .expect("private parent");
    let path = parent.join("connections.json");

    let first = connection(100);
    save_connections(&path, std::slice::from_ref(&first)).expect("first create remains available");
    let before = std::fs::read(&path).expect("read initial document");
    let before_names = std::fs::read_dir(&parent)
        .expect("read parent")
        .map(|entry| entry.expect("entry").file_name())
        .collect::<Vec<_>>();

    let replacement = connection(200);
    let error = save_connections(&path, std::slice::from_ref(&replacement))
        .expect_err("existing document replacement must remain unavailable");

    assert_eq!(
        error,
        "oauth-connection-document-object-bound-replacement-unavailable"
    );
    assert_eq!(
        std::fs::read(&path).expect("read preserved document"),
        before,
        "failed update must preserve the prior accepted OAuth document byte-for-byte"
    );
    let after_names = std::fs::read_dir(&parent)
        .expect("read parent after refusal")
        .map(|entry| entry.expect("entry").file_name())
        .collect::<Vec<_>>();
    assert_eq!(
        after_names, before_names,
        "failed update must not create staging, delete-and-create, or pathname fallback artifacts"
    );
    assert_eq!(
        load_connections(&path).expect("load preserved document"),
        vec![first],
        "replacement refusal must leave the accepted connection state unchanged"
    );
}
