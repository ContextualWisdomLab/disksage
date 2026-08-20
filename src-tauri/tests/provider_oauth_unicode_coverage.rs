//! Coverage for Unicode-stable provider OAuth connection identity selection.
//!
//! macOS File Provider can expose equivalent root identifiers and paths in NFC or NFD. These
//! deterministic regressions prove that a legacy raw-spelling connection remains usable, that a
//! canonical connection is preferred when both identities exist, and that duplicate or invalid
//! records fail closed instead of selecting an arbitrary credential.

use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::provider_oauth::{
    connection_for_root, connections_path, load_connections, requested_scope, OAuthConnection,
};
use sha2::{Digest, Sha256};

const GOOGLE_CLIENT_ID: &str = "1234567890-abcxyz.apps.googleusercontent.com";

fn root(id: &str, path: &str) -> CloudRoot {
    CloudRoot {
        id: id.into(),
        provider: CloudProvider::GoogleDrive,
        account_scope: CloudAccountScope::Unknown,
        label: "Cloud".into(),
        path: path.into(),
        readable: true,
        access_issue: None,
    }
}

fn raw_connection_id(root: &CloudRoot) -> String {
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
        connection_id: raw_connection_id(root),
        provider: root.provider,
        cloud_root_id: root.id.clone(),
        cloud_root_path: root.path.clone(),
        client_id: GOOGLE_CLIENT_ID.into(),
        scope: requested_scope(root.provider).unwrap().into(),
        connected_at_ms: 123,
    }
}

fn write_document(path: &std::path::Path, connections: &[OAuthConnection]) {
    std::fs::write(
        path,
        serde_json::to_vec(&serde_json::json!({
            "version": 1,
            "connections": connections,
        }))
        .unwrap(),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
}

#[cfg(windows)]
const COMPOSED_PATH: &str = "C:\\Cloud\\Caf\u{e9}";
#[cfg(windows)]
const DECOMPOSED_PATH: &str = "C:\\Cloud\\Cafe\u{301}";
#[cfg(windows)]
const OTHER_PATH: &str = "C:\\Cloud\\Other";
#[cfg(not(windows))]
const COMPOSED_PATH: &str = "/Cloud/Caf\u{e9}";
#[cfg(not(windows))]
const DECOMPOSED_PATH: &str = "/Cloud/Cafe\u{301}";
#[cfg(not(windows))]
const OTHER_PATH: &str = "/Cloud/Other";

#[test]
fn legacy_decomposed_identity_matches_the_canonical_root() {
    let temp = tempfile::tempdir().unwrap();
    let path = connections_path(temp.path());
    let decomposed = root("Cafe\u{301}", DECOMPOSED_PATH);
    let canonical = root("Caf\u{e9}", COMPOSED_PATH);
    let legacy = connection(&decomposed);

    write_document(&path, std::slice::from_ref(&legacy));
    let loaded = load_connections(&path).unwrap();

    assert_eq!(connection_for_root(&loaded, &canonical).unwrap(), legacy);
}

#[test]
fn canonical_identity_is_preferred_over_an_equivalent_legacy_record() {
    let temp = tempfile::tempdir().unwrap();
    let path = connections_path(temp.path());
    let decomposed = root("Cafe\u{301}", DECOMPOSED_PATH);
    let canonical_root = root("Caf\u{e9}", COMPOSED_PATH);
    let legacy = connection(&decomposed);
    let canonical = connection(&canonical_root);

    write_document(&path, &[legacy, canonical.clone()]);
    let loaded = load_connections(&path).unwrap();

    assert_eq!(
        connection_for_root(&loaded, &canonical_root).unwrap(),
        canonical
    );
}

#[test]
fn duplicate_canonical_identities_fail_closed_as_ambiguous() {
    let temp = tempfile::tempdir().unwrap();
    let path = connections_path(temp.path());
    let canonical_root = root("Caf\u{e9}", COMPOSED_PATH);
    let canonical = connection(&canonical_root);

    write_document(&path, &[canonical.clone(), canonical]);
    let loaded = load_connections(&path).unwrap();

    assert_eq!(
        connection_for_root(&loaded, &canonical_root).unwrap_err(),
        "provider-oauth-connection-ambiguous"
    );
}

#[test]
fn duplicate_legacy_identities_fail_closed_as_ambiguous() {
    let temp = tempfile::tempdir().unwrap();
    let path = connections_path(temp.path());
    let decomposed = root("Cafe\u{301}", DECOMPOSED_PATH);
    let canonical = root("Caf\u{e9}", COMPOSED_PATH);
    let legacy = connection(&decomposed);

    write_document(&path, &[legacy.clone(), legacy]);
    let loaded = load_connections(&path).unwrap();

    assert_eq!(
        connection_for_root(&loaded, &canonical).unwrap_err(),
        "provider-oauth-connection-ambiguous"
    );
}

#[test]
fn unrelated_connection_is_reported_as_missing() {
    let requested = root("Caf\u{e9}", COMPOSED_PATH);
    let other = root("other", OTHER_PATH);

    assert_eq!(
        connection_for_root(&[connection(&other)], &requested).unwrap_err(),
        "provider-oauth-connection-missing"
    );
}

#[test]
fn invalid_candidate_is_ignored_instead_of_authorizing_the_root() {
    let requested = root("Caf\u{e9}", COMPOSED_PATH);
    let mut invalid = connection(&requested);
    invalid.scope = "https://example.invalid/overbroad".into();

    assert_eq!(
        connection_for_root(&[invalid], &requested).unwrap_err(),
        "provider-oauth-connection-missing"
    );
}
