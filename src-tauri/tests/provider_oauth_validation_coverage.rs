//! Credential-free validation coverage for the public provider OAuth boundary.
//!
//! These regressions exercise malformed client identifiers and persisted connection documents
//! without opening a browser, contacting an OAuth provider, touching the credential store, or
//! authorizing any cloud mutation.

use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::provider_oauth::{
    connection_for_root, connections_path, load_connections, requested_scope, validate_client_id,
    OAuthConnection,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::fs;
use unicode_normalization::UnicodeNormalization;

const MICROSOFT_CLIENT_ID: &str = "12345678-1234-4abc-8def-1234567890ab";
const GOOGLE_CLIENT_ID: &str = "1234567890-abcxyz.apps.googleusercontent.com";

fn root(provider: CloudProvider) -> CloudRoot {
    #[cfg(windows)]
    let path = r"C:\Cloud";
    #[cfg(not(windows))]
    let path = "/Cloud";

    CloudRoot {
        id: format!("{}:coverage", provider.as_str()),
        provider,
        account_scope: CloudAccountScope::Unknown,
        label: "Coverage root".into(),
        path: path.into(),
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
        write!(&mut encoded, "{byte:02x}").unwrap();
    }
    encoded
}

fn current_connection_id(root: &CloudRoot) -> String {
    let root_id = root.id.nfc().collect::<String>();
    let root_path = root.path.nfc().collect::<String>();
    connection_id_for_values(root.provider.as_str(), &root_id, &root_path)
}

fn bound_connection(provider: CloudProvider) -> OAuthConnection {
    let root = root(provider);
    let client_id = match provider {
        CloudProvider::Onedrive => MICROSOFT_CLIENT_ID,
        CloudProvider::GoogleDrive => GOOGLE_CLIENT_ID,
        CloudProvider::Icloud => panic!("iCloud OAuth is intentionally unsupported"),
    };
    OAuthConnection {
        connection_id: current_connection_id(&root),
        provider,
        cloud_root_id: root.id,
        cloud_root_path: root.path,
        client_id: client_id.into(),
        scope: requested_scope(provider).unwrap().into(),
        connected_at_ms: 1,
    }
}

fn structurally_valid_but_unbound_connection() -> OAuthConnection {
    let root = root(CloudProvider::Onedrive);
    OAuthConnection {
        connection_id: "0".repeat(64),
        provider: CloudProvider::Onedrive,
        cloud_root_id: root.id,
        cloud_root_path: root.path,
        client_id: MICROSOFT_CLIENT_ID.into(),
        scope: requested_scope(CloudProvider::Onedrive).unwrap().into(),
        connected_at_ms: 1,
    }
}

#[test]
fn client_identifier_admission_rejects_generic_and_provider_specific_malformed_values() {
    assert!(validate_client_id(CloudProvider::Onedrive, MICROSOFT_CLIENT_ID).is_ok());
    assert!(validate_client_id(CloudProvider::GoogleDrive, GOOGLE_CLIENT_ID).is_ok());

    for invalid in [
        "",
        " leading-space",
        "trailing-space ",
        "contains\ncontrol",
        "é",
    ] {
        assert_eq!(
            validate_client_id(CloudProvider::Onedrive, invalid).unwrap_err(),
            "oauth-client-id-invalid"
        );
    }
    assert_eq!(
        validate_client_id(CloudProvider::Onedrive, &"a".repeat(513)).unwrap_err(),
        "oauth-client-id-invalid"
    );

    for invalid in [
        "1234567-1234-4abc-8def-1234567890ab",
        "12345678-1234-4abc-8def-1234567890ag",
        "1234567812344abc8def1234567890ab",
    ] {
        assert_eq!(
            validate_client_id(CloudProvider::Onedrive, invalid).unwrap_err(),
            "oauth-client-id-provider-format-invalid"
        );
    }

    for invalid in [
        ".apps.googleusercontent.com",
        "abc_xyz.apps.googleusercontent.com",
        "abc.example.com",
    ] {
        assert_eq!(
            validate_client_id(CloudProvider::GoogleDrive, invalid).unwrap_err(),
            "oauth-client-id-provider-format-invalid"
        );
    }

    assert_eq!(
        validate_client_id(CloudProvider::Icloud, MICROSOFT_CLIENT_ID).unwrap_err(),
        "icloud-oauth-not-supported"
    );
    assert_eq!(
        requested_scope(CloudProvider::Icloud).unwrap_err(),
        "icloud-oauth-not-supported"
    );
}

#[test]
fn connection_document_path_and_missing_document_are_deterministic() {
    let temp = tempfile::tempdir().unwrap();
    let path = connections_path(temp.path());

    assert_eq!(path, temp.path().join("cloud-oauth-connections.json"));
    assert_eq!(load_connections(&path).unwrap(), Vec::<OAuthConnection>::new());
}

#[test]
fn valid_connection_document_round_trips_and_duplicate_current_ids_fail_closed() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("connections.json");
    let target = root(CloudProvider::Onedrive);
    let connection = bound_connection(CloudProvider::Onedrive);

    fs::write(
        &path,
        serde_json::to_vec(&json!({
            "version": 1,
            "connections": [connection.clone()]
        }))
        .unwrap(),
    )
    .unwrap();
    let loaded = load_connections(&path).unwrap();
    assert_eq!(loaded, vec![connection.clone()]);
    assert_eq!(connection_for_root(&loaded, &target).unwrap(), connection);

    let duplicate = vec![connection.clone(), connection];
    assert_eq!(
        connection_for_root(&duplicate, &target).unwrap_err(),
        "provider-oauth-connection-ambiguous"
    );
}

#[test]
fn legacy_unicode_connection_id_matches_canonical_equivalent_root() {
    #[cfg(windows)]
    let composed_path = r"C:\Cloud\Café";
    #[cfg(not(windows))]
    let composed_path = "/Cloud/Café";
    let decomposed_path = composed_path.nfd().collect::<String>();
    let composed_id = "google-drive:Café";
    let decomposed_id = composed_id.nfd().collect::<String>();

    let saved_root = CloudRoot {
        id: decomposed_id.clone(),
        provider: CloudProvider::GoogleDrive,
        account_scope: CloudAccountScope::Unknown,
        label: "Coverage root".into(),
        path: decomposed_path.clone(),
        readable: true,
        access_issue: None,
    };
    let requested_root = CloudRoot {
        id: composed_id.into(),
        provider: CloudProvider::GoogleDrive,
        account_scope: CloudAccountScope::Unknown,
        label: "Coverage root".into(),
        path: composed_path.into(),
        readable: true,
        access_issue: None,
    };
    let connection = OAuthConnection {
        connection_id: connection_id_for_values(
            saved_root.provider.as_str(),
            &saved_root.id,
            &saved_root.path,
        ),
        provider: saved_root.provider,
        cloud_root_id: saved_root.id.clone(),
        cloud_root_path: saved_root.path.clone(),
        client_id: GOOGLE_CLIENT_ID.into(),
        scope: requested_scope(CloudProvider::GoogleDrive).unwrap().into(),
        connected_at_ms: 2,
    };

    assert_ne!(connection.connection_id, current_connection_id(&saved_root));
    assert_eq!(
        connection_for_root(std::slice::from_ref(&connection), &requested_root).unwrap(),
        connection
    );
}

#[test]
fn connection_document_rejects_non_regular_oversized_invalid_and_unsupported_documents() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("connections.json");

    assert_eq!(
        load_connections(temp.path()).unwrap_err(),
        "oauth-connection-document-not-regular-file"
    );

    let oversized = fs::File::create(&path).unwrap();
    oversized.set_len(256 * 1024 + 1).unwrap();
    drop(oversized);
    assert_eq!(
        load_connections(&path).unwrap_err(),
        "oauth-connection-document-too-large"
    );

    fs::write(&path, b"not-json").unwrap();
    assert_eq!(
        load_connections(&path).unwrap_err(),
        "oauth-connection-document-invalid"
    );

    fs::write(
        &path,
        serde_json::to_vec(&json!({"version": 2, "connections": []})).unwrap(),
    )
    .unwrap();
    assert_eq!(
        load_connections(&path).unwrap_err(),
        "oauth-connection-document-version-or-count-invalid"
    );

    let too_many = vec![structurally_valid_but_unbound_connection(); 33];
    fs::write(
        &path,
        serde_json::to_vec(&json!({"version": 1, "connections": too_many})).unwrap(),
    )
    .unwrap();
    assert_eq!(
        load_connections(&path).unwrap_err(),
        "oauth-connection-document-version-or-count-invalid"
    );

    fs::write(
        &path,
        serde_json::to_vec(&json!({
            "version": 1,
            "connections": [structurally_valid_but_unbound_connection()]
        }))
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        load_connections(&path).unwrap_err(),
        "oauth-connection-id-mismatch"
    );
}

#[cfg(unix)]
#[test]
fn connection_document_rejects_symbolic_links_without_following_them() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("target.json");
    let link = temp.path().join("connections.json");
    fs::write(&target, br#"{"version":1,"connections":[]}"#).unwrap();
    symlink(&target, &link).unwrap();

    assert_eq!(
        load_connections(&link).unwrap_err(),
        "oauth-connection-document-not-regular-file"
    );
}

#[test]
fn root_lookup_ignores_invalid_persisted_connections_and_fails_closed_when_missing() {
    let target = root(CloudProvider::Onedrive);
    assert_eq!(
        connection_for_root(&[], &target).unwrap_err(),
        "provider-oauth-connection-missing"
    );
    assert_eq!(
        connection_for_root(&[structurally_valid_but_unbound_connection()], &target).unwrap_err(),
        "provider-oauth-connection-missing"
    );
}
