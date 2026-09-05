#![allow(dead_code, unused_imports)]

//! Credential-free coverage for durable OAuth connection publication.
//!
//! The writer is intentionally private: callers reach it through the bounded OAuth lifecycle.
//! Including the production module here lets these regressions exercise the real local persistence
//! boundary without widening the shipped API, opening a browser, contacting a provider, or touching
//! the credential store.

include!("../src/provider_oauth.rs");

mod cloud {
    pub use disksage_lib::cloud::*;
}

fn google_root(id: &str) -> CloudRoot {
    #[cfg(windows)]
    let path = format!(r"C:\Cloud\{id}");
    #[cfg(not(windows))]
    let path = format!("/Cloud/{id}");

    CloudRoot {
        id: format!("google-drive:{id}"),
        provider: CloudProvider::GoogleDrive,
        account_scope: crate::cloud::CloudAccountScope::Unknown,
        label: "Google Drive".into(),
        path,
        readable: true,
        access_issue: None,
    }
}

fn connection(id: &str, connected_at_ms: u64) -> OAuthConnection {
    let root = google_root(id);
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

#[test]
fn valid_publication_is_private_loadable_and_replaceable() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("first-use-app-data").join("connections.json");
    let first = connection("account-a", 123);

    save_connections(&path, std::slice::from_ref(&first)).unwrap();
    assert_eq!(load_connections(&path).unwrap(), vec![first.clone()]);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600,
            "durable OAuth metadata must be private at creation time"
        );
    }

    let mut replacement = first;
    replacement.connected_at_ms = 456;
    save_connections(&path, std::slice::from_ref(&replacement)).unwrap();
    assert_eq!(
        load_connections(&path).unwrap(),
        vec![replacement],
        "replacement must publish the complete new document rather than preserve stale metadata"
    );
}

#[cfg(unix)]
#[test]
fn shared_writable_parent_never_gains_publication_authority() {
    use std::os::unix::fs::PermissionsExt;

    for writable_bit in [0o020, 0o002] {
        let temp = tempfile::tempdir().unwrap();
        let parent = temp.path().join(format!("oauth-write-parent-{writable_bit:o}"));
        std::fs::create_dir(&parent).unwrap();
        std::fs::set_permissions(
            &parent,
            std::fs::Permissions::from_mode(0o700 | writable_bit),
        )
        .unwrap();
        let path = parent.join("connections.json");

        let result = save_connections(&path, &[connection("account", 1)]);

        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(
            result.unwrap_err(),
            "oauth-connection-directory-writable-by-others",
            "writable bit {writable_bit:o} must fail before durable OAuth metadata is published"
        );
        assert!(
            !path.exists(),
            "a rejected authority directory must not receive a connection document"
        );
        assert_eq!(
            std::fs::read_dir(&parent).unwrap().count(),
            0,
            "authority rejection must not leave a temporary OAuth document behind"
        );
    }
}

#[test]
fn invalid_sets_fail_before_first_use_authority_is_created() {
    let temp = tempfile::tempdir().unwrap();

    let duplicate_parent = temp.path().join("duplicate-first-use");
    let duplicate_path = duplicate_parent.join("connections.json");
    let duplicate = connection("duplicate", 1);
    assert_eq!(
        save_connections(&duplicate_path, &[duplicate.clone(), duplicate]).unwrap_err(),
        "oauth-connection-document-duplicate-id"
    );
    assert!(
        !duplicate_parent.exists(),
        "duplicate identities must be rejected before creating the durable authority directory"
    );

    let count_parent = temp.path().join("count-first-use");
    let count_path = count_parent.join("connections.json");
    let too_many: Vec<_> = (0..=MAX_CONNECTIONS)
        .map(|index| connection(&format!("account-{index}"), index as u64))
        .collect();
    assert_eq!(
        save_connections(&count_path, &too_many).unwrap_err(),
        "oauth-connection-count-invalid"
    );
    assert!(
        !count_parent.exists(),
        "an over-capacity document must not create its first-use authority directory"
    );
}

#[test]
fn an_existing_non_regular_destination_never_gains_publication_authority() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("connections.json");
    std::fs::create_dir(&path).unwrap();

    assert_eq!(
        save_connections(&path, &[connection("account", 1)]).unwrap_err(),
        "oauth-connection-document-not-regular-file"
    );
    assert!(path.is_dir(), "rejected destination must remain untouched");
}

#[cfg(unix)]
#[test]
fn a_symlink_destination_never_gains_publication_authority_or_mutates_its_target() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("external-sensitive.json");
    let original = b"external-sensitive-bytes";
    std::fs::write(&target, original).unwrap();
    let path = temp.path().join("connections.json");
    symlink(&target, &path).unwrap();

    assert_eq!(
        save_connections(&path, &[connection("account", 1)]).unwrap_err(),
        "oauth-connection-document-not-regular-file"
    );
    assert!(
        std::fs::symlink_metadata(&path)
            .unwrap()
            .file_type()
            .is_symlink(),
        "rejected OAuth destination must remain a symlink rather than being replaced"
    );
    assert_eq!(
        std::fs::read(&target).unwrap(),
        original,
        "a rejected symlink destination must not mutate its target"
    );
    assert_eq!(
        std::fs::read_dir(temp.path()).unwrap().count(),
        2,
        "symlink rejection must not leave a temporary OAuth document behind"
    );
}
