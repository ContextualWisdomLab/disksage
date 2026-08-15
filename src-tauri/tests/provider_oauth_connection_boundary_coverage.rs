//! Public-boundary coverage for cloud OAuth connection persistence and lookup.
//!
//! These tests exercise malformed host input and persistence states through the exported
//! DiskSage API. They intentionally avoid network access and credential-store mutation.

use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::provider_oauth::{
    connection_for_root, connections_path, load_connections, requested_scope, validate_client_id,
    OAuthConnection,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::fs::File;
use std::path::{Path, PathBuf};

fn root_path() -> String {
    #[cfg(windows)]
    {
        r"C:\Cloud".to_string()
    }
    #[cfg(not(windows))]
    {
        "/Cloud".to_string()
    }
}

fn connection_id_for_values(provider: CloudProvider, root_id: &str, root_path: &str) -> String {
    let mut hasher = Sha256::new();
    for value in [provider.as_str(), root_id, root_path] {
        hasher.update(value.as_bytes());
        hasher.update([0]);
    }
    let digest = hasher.finalize();
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut encoded, "{byte:02x}").expect("writing a digest to String cannot fail");
    }
    encoded
}

fn root(provider: CloudProvider) -> CloudRoot {
    let path = root_path();
    CloudRoot {
        id: format!("{}:account", provider.as_str()),
        provider,
        account_scope: CloudAccountScope::Unknown,
        label: "Cloud".into(),
        path,
        readable: true,
        access_issue: None,
    }
}

fn connection(provider: CloudProvider) -> OAuthConnection {
    let root = root(provider);
    let client_id = match provider {
        CloudProvider::Onedrive => "12345678-1234-4abc-8def-1234567890ab",
        CloudProvider::GoogleDrive => "1234567890-abcxyz.apps.googleusercontent.com",
        CloudProvider::Icloud => "unsupported",
    };
    OAuthConnection {
        connection_id: connection_id_for_values(provider, &root.id, &root.path),
        provider,
        cloud_root_id: root.id,
        cloud_root_path: root.path,
        client_id: client_id.into(),
        scope: requested_scope(provider).unwrap_or_default().into(),
        connected_at_ms: 123,
    }
}

fn write_document(path: &Path, version: u32, connections: &[OAuthConnection]) {
    let encoded = serde_json::to_vec(&json!({
        "version": version,
        "connections": connections,
    }))
    .expect("test connection document must serialize");
    std::fs::write(path, encoded).expect("test connection document must be writable");
}

#[test]
fn client_id_validation_rejects_length_encoding_whitespace_and_provider_shape_edges() {
    assert!(validate_client_id(
        CloudProvider::Onedrive,
        "12345678-1234-4abc-8def-1234567890ab"
    )
    .is_ok());
    assert!(validate_client_id(
        CloudProvider::GoogleDrive,
        "1234567890-abcxyz.apps.googleusercontent.com"
    )
    .is_ok());

    for invalid in [
        " 12345678-1234-4abc-8def-1234567890ab",
        "12345678-1234-4abc-8def-1234567890ab ",
        "12345678-1234-4abc-8def-1234567890a\n",
        "12345678-1234-4abc-8def-1234567890äb",
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
    ] {
        assert_eq!(
            validate_client_id(CloudProvider::Onedrive, invalid).unwrap_err(),
            "oauth-client-id-provider-format-invalid"
        );
    }
    for invalid in [
        ".apps.googleusercontent.com",
        "abc_def.apps.googleusercontent.com",
        "abc.example.invalid",
    ] {
        assert_eq!(
            validate_client_id(CloudProvider::GoogleDrive, invalid).unwrap_err(),
            "oauth-client-id-provider-format-invalid"
        );
    }
    assert_eq!(
        requested_scope(CloudProvider::Icloud).unwrap_err(),
        "icloud-oauth-not-supported"
    );
    assert_eq!(
        validate_client_id(CloudProvider::Icloud, "client").unwrap_err(),
        "icloud-oauth-not-supported"
    );
}

#[test]
fn connection_document_rejects_non_regular_oversized_invalid_version_and_excess_count() {
    let temp = tempfile::tempdir().expect("temporary directory must be available");
    let missing = temp.path().join("missing.json");
    assert!(load_connections(&missing).unwrap().is_empty());

    let directory = temp.path().join("directory.json");
    std::fs::create_dir(&directory).unwrap();
    assert_eq!(
        load_connections(&directory).unwrap_err(),
        "oauth-connection-document-not-regular-file"
    );

    let oversized = temp.path().join("oversized.json");
    let file = File::create(&oversized).unwrap();
    file.set_len(256 * 1024 + 1).unwrap();
    assert_eq!(
        load_connections(&oversized).unwrap_err(),
        "oauth-connection-document-too-large"
    );

    let invalid_json = temp.path().join("invalid.json");
    std::fs::write(&invalid_json, b"{").unwrap();
    assert_eq!(
        load_connections(&invalid_json).unwrap_err(),
        "oauth-connection-document-invalid"
    );

    let version = temp.path().join("version.json");
    write_document(&version, 2, &[]);
    assert_eq!(
        load_connections(&version).unwrap_err(),
        "oauth-connection-document-version-or-count-invalid"
    );

    let count = temp.path().join("count.json");
    write_document(&count, 1, &vec![connection(CloudProvider::Onedrive); 33]);
    assert_eq!(
        load_connections(&count).unwrap_err(),
        "oauth-connection-document-version-or-count-invalid"
    );
}

#[test]
fn persisted_connection_validation_rejects_identity_scope_path_and_client_tampering() {
    let temp = tempfile::tempdir().expect("temporary directory must be available");
    let path = temp.path().join("connections.json");
    let baseline = connection(CloudProvider::Onedrive);

    let mut cases: Vec<(OAuthConnection, &str)> = Vec::new();

    let mut short_id = baseline.clone();
    short_id.connection_id = "0".repeat(63);
    cases.push((short_id, "oauth-connection-invalid"));

    let mut non_hex_id = baseline.clone();
    non_hex_id.connection_id = "g".repeat(64);
    cases.push((non_hex_id, "oauth-connection-invalid"));

    let mut empty_root_id = baseline.clone();
    empty_root_id.cloud_root_id = " ".into();
    cases.push((empty_root_id, "oauth-connection-invalid"));

    let mut relative_root = baseline.clone();
    relative_root.cloud_root_path = "relative/cloud".into();
    cases.push((relative_root, "oauth-connection-invalid"));

    let mut wrong_scope = baseline.clone();
    wrong_scope.scope = "Files.ReadWrite".into();
    cases.push((wrong_scope, "oauth-connection-invalid"));

    let mut bad_client = baseline.clone();
    bad_client.client_id = "not-a-guid".into();
    cases.push((bad_client, "oauth-client-id-provider-format-invalid"));

    let mut mismatched_id = baseline.clone();
    mismatched_id.connection_id = "0".repeat(64);
    cases.push((mismatched_id, "oauth-connection-id-mismatch"));

    for (candidate, expected_error) in cases {
        write_document(&path, 1, &[candidate]);
        assert_eq!(load_connections(&path).unwrap_err(), expected_error);
    }
}

#[test]
fn connection_lookup_is_exact_missing_and_ambiguous_without_trusting_invalid_records() {
    let requested = root(CloudProvider::GoogleDrive);
    let exact = connection(CloudProvider::GoogleDrive);
    assert_eq!(
        connection_for_root(std::slice::from_ref(&exact), &requested).unwrap(),
        exact
    );
    assert_eq!(
        connection_for_root(&[], &requested).unwrap_err(),
        "provider-oauth-connection-missing"
    );
    assert_eq!(
        connection_for_root(&[exact.clone(), exact.clone()], &requested).unwrap_err(),
        "provider-oauth-connection-ambiguous"
    );

    let mut invalid = exact.clone();
    invalid.connection_id = "0".repeat(64);
    assert_eq!(
        connection_for_root(&[invalid], &requested).unwrap_err(),
        "provider-oauth-connection-missing"
    );
}

#[test]
fn connections_path_is_confined_to_the_supplied_application_data_directory() {
    let base = PathBuf::from("application-data");
    assert_eq!(
        connections_path(&base),
        base.join("cloud-oauth-connections.json")
    );
}
