//! Credential-free migration coverage for persisted OAuth connection identity selection.
//!
//! Version-1 connection documents hashed the raw filesystem spelling while current identities
//! normalize File Provider roots to NFC. These tests exercise the public lookup boundary with
//! realistic legacy/current records and no browser, keyring, provider, or network access.

use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::provider_oauth::{connection_for_root, requested_scope, OAuthConnection};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use unicode_normalization::UnicodeNormalization;

const GOOGLE_CLIENT_ID: &str = "1234567890-abcxyz.apps.googleusercontent.com";

fn root(path: String) -> CloudRoot {
    CloudRoot {
        id: path.clone(),
        provider: CloudProvider::GoogleDrive,
        account_scope: CloudAccountScope::Organization,
        label: "Google Drive".into(),
        path,
        readable: true,
        access_issue: None,
    }
}

fn connection_id_for_values(provider: &str, root_id: &str, root_path: &str) -> String {
    let mut hasher = Sha256::new();
    for value in [provider, root_id, root_path] {
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

fn canonical_connection_id(root: &CloudRoot) -> String {
    let root_id = root.id.nfc().collect::<String>();
    let root_path = root.path.nfc().collect::<String>();
    connection_id_for_values(root.provider.as_str(), &root_id, &root_path)
}

fn legacy_connection_id(root: &CloudRoot) -> String {
    connection_id_for_values(root.provider.as_str(), &root.id, &root.path)
}

fn connection(root: &CloudRoot, connection_id: String, connected_at_ms: u64) -> OAuthConnection {
    OAuthConnection {
        connection_id,
        provider: root.provider,
        cloud_root_id: root.id.clone(),
        cloud_root_path: root.path.clone(),
        client_id: GOOGLE_CLIENT_ID.into(),
        scope: requested_scope(root.provider).unwrap().into(),
        connected_at_ms,
    }
}

fn equivalent_roots() -> (CloudRoot, CloudRoot) {
    #[cfg(windows)]
    let composed = r"C:\Cloud\내 드라이브";
    #[cfg(not(windows))]
    let composed = "/Cloud/내 드라이브";

    let requested = root(composed.to_string());
    let legacy = root(composed.nfd().collect::<String>());
    assert_ne!(requested.path, legacy.path);
    assert_eq!(
        requested.path.nfc().collect::<String>(),
        legacy.path.nfc().collect::<String>()
    );
    (requested, legacy)
}

#[test]
fn canonical_record_wins_when_equivalent_legacy_record_is_still_present() {
    let (requested, legacy_root) = equivalent_roots();
    let legacy = connection(&legacy_root, legacy_connection_id(&legacy_root), 100);
    let current = connection(&legacy_root, canonical_connection_id(&legacy_root), 200);
    assert_ne!(legacy.connection_id, current.connection_id);

    let selected = connection_for_root(&[legacy, current.clone()], &requested).unwrap();

    assert_eq!(selected, current);
    assert_eq!(selected.connected_at_ms, 200);
}

#[test]
fn a_single_legacy_record_remains_usable_during_identity_migration() {
    let (requested, legacy_root) = equivalent_roots();
    let legacy = connection(&legacy_root, legacy_connection_id(&legacy_root), 100);

    assert_eq!(
        connection_for_root(std::slice::from_ref(&legacy), &requested).unwrap(),
        legacy
    );
}

#[test]
fn missing_connection_fails_closed() {
    let (requested, _) = equivalent_roots();

    assert_eq!(
        connection_for_root(&[], &requested).unwrap_err(),
        "provider-oauth-connection-missing"
    );
}

#[test]
fn duplicate_canonical_records_remain_ambiguous_instead_of_gaining_authority() {
    let (requested, legacy_root) = equivalent_roots();
    let current = connection(&legacy_root, canonical_connection_id(&legacy_root), 200);

    assert_eq!(
        connection_for_root(&[current.clone(), current], &requested).unwrap_err(),
        "provider-oauth-connection-ambiguous"
    );
}

#[test]
fn duplicate_legacy_records_remain_ambiguous_instead_of_gaining_authority() {
    let (requested, legacy_root) = equivalent_roots();
    let legacy = connection(&legacy_root, legacy_connection_id(&legacy_root), 100);

    assert_eq!(
        connection_for_root(&[legacy.clone(), legacy], &requested).unwrap_err(),
        "provider-oauth-connection-ambiguous"
    );
}
