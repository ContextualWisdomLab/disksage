//! Public-contract coverage for provider OAuth validation and connection-document admission.
//!
//! These regressions exercise deterministic, credential-free production boundaries only. They do
//! not contact providers, open browser flows, or read/write the operating-system credential store.

use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::provider_oauth::{
    connection_for_root, connections_path, load_connections, requested_scope, validate_client_id,
    OAuthConnection,
};
use sha2::{Digest, Sha256};

const MICROSOFT_CLIENT_ID: &str = "12345678-1234-4abc-8def-1234567890ab";
const GOOGLE_CLIENT_ID: &str = "1234567890-abcxyz.apps.googleusercontent.com";

fn provider_client_id(provider: CloudProvider) -> &'static str {
    match provider {
        CloudProvider::Onedrive => MICROSOFT_CLIENT_ID,
        CloudProvider::GoogleDrive => GOOGLE_CLIENT_ID,
        CloudProvider::Icloud => "unsupported",
    }
}

fn root(provider: CloudProvider, id: &str) -> CloudRoot {
    #[cfg(windows)]
    let path = format!(r"C:\Cloud\{id}");
    #[cfg(not(windows))]
    let path = format!("/Cloud/{id}");

    CloudRoot {
        id: id.into(),
        provider,
        account_scope: CloudAccountScope::Unknown,
        label: "Cloud".into(),
        path,
        readable: true,
        access_issue: None,
    }
}

fn connection_id(provider: CloudProvider, root_id: &str, root_path: &str) -> String {
    let mut hasher = Sha256::new();
    for value in [provider.as_str(), root_id, root_path] {
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
        connection_id: connection_id(root.provider, &root.id, &root.path),
        provider: root.provider,
        cloud_root_id: root.id.clone(),
        cloud_root_path: root.path.clone(),
        client_id: provider_client_id(root.provider).into(),
        scope: requested_scope(root.provider).unwrap_or_default().into(),
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
}

#[test]
fn provider_scopes_and_client_identifiers_fail_closed() {
    assert_eq!(
        requested_scope(CloudProvider::Onedrive).unwrap(),
        "Files.Read offline_access"
    );
    assert_eq!(
        requested_scope(CloudProvider::GoogleDrive).unwrap(),
        "https://www.googleapis.com/auth/drive.metadata.readonly"
    );
    assert_eq!(
        requested_scope(CloudProvider::Icloud).unwrap_err(),
        "icloud-oauth-not-supported"
    );

    assert!(validate_client_id(CloudProvider::Onedrive, MICROSOFT_CLIENT_ID).is_ok());
    assert!(validate_client_id(CloudProvider::GoogleDrive, GOOGLE_CLIENT_ID).is_ok());

    for invalid in [
        "",
        " 12345678-1234-4abc-8def-1234567890ab",
        "12345678-1234-4abc-8def-1234567890ab\n",
        "한글.apps.googleusercontent.com",
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
    for malformed in [
        "not-a-guid",
        "1234567-1234-4abc-8def-1234567890ab",
        "12345678-123-4abc-8def-1234567890ab",
        "12345678-1234-4abc-8def-1234567890a",
        "12345678-1234-4abc-8def-1234567890az",
        "12345678-1234-4abc-8def-1234567890ab-extra",
    ] {
        assert_eq!(
            validate_client_id(CloudProvider::Onedrive, malformed).unwrap_err(),
            "oauth-client-id-provider-format-invalid"
        );
    }
    for malformed in [
        "missing-provider-suffix",
        ".apps.googleusercontent.com",
        "abc_.apps.googleusercontent.com",
        "abc!.apps.googleusercontent.com",
    ] {
        assert_eq!(
            validate_client_id(CloudProvider::GoogleDrive, malformed).unwrap_err(),
            "oauth-client-id-provider-format-invalid"
        );
    }
    assert_eq!(
        validate_client_id(CloudProvider::Icloud, MICROSOFT_CLIENT_ID).unwrap_err(),
        "icloud-oauth-not-supported"
    );
}

#[test]
fn connection_document_admission_rejects_unsafe_files_and_malformed_documents() {
    let temp = tempfile::tempdir().unwrap();
    let path = connections_path(temp.path());
    assert_eq!(path, temp.path().join("cloud-oauth-connections.json"));
    assert!(load_connections(&path).unwrap().is_empty());

    std::fs::create_dir(&path).unwrap();
    assert_eq!(
        load_connections(&path).unwrap_err(),
        "oauth-connection-document-not-regular-file"
    );
    std::fs::remove_dir(&path).unwrap();

    std::fs::write(&path, b"not-json").unwrap();
    assert_eq!(
        load_connections(&path).unwrap_err(),
        "oauth-connection-document-invalid"
    );

    std::fs::write(&path, br#"{"version":2,"connections":[]}"#).unwrap();
    assert_eq!(
        load_connections(&path).unwrap_err(),
        "oauth-connection-document-version-or-count-invalid"
    );

    std::fs::write(&path, br#"{"version":1,"connections":[]}"#).unwrap();
    assert!(load_connections(&path).unwrap().is_empty());

    std::fs::write(&path, vec![b'x'; 256 * 1024 + 1]).unwrap();
    assert_eq!(
        load_connections(&path).unwrap_err(),
        "oauth-connection-document-too-large"
    );
}

#[test]
fn valid_connection_documents_bind_provider_root_and_identity() {
    let temp = tempfile::tempdir().unwrap();
    let path = connections_path(temp.path());
    let google_root = root(CloudProvider::GoogleDrive, "google-account");
    let onedrive_root = root(CloudProvider::Onedrive, "microsoft-account");
    let google = connection(&google_root);
    let onedrive = connection(&onedrive_root);

    write_document(&path, &[google.clone(), onedrive.clone()]);
    let loaded = load_connections(&path).unwrap();
    assert_eq!(loaded, vec![google.clone(), onedrive.clone()]);
    assert_eq!(connection_for_root(&loaded, &google_root).unwrap(), google);
    assert_eq!(connection_for_root(&loaded, &onedrive_root).unwrap(), onedrive);

    let missing = root(CloudProvider::GoogleDrive, "other-google-account");
    assert_eq!(
        connection_for_root(&loaded, &missing).unwrap_err(),
        "provider-oauth-connection-missing"
    );

    let duplicate = connection(&google_root);
    assert_eq!(
        connection_for_root(&[duplicate.clone(), duplicate], &google_root).unwrap_err(),
        "provider-oauth-connection-ambiguous"
    );
}

#[test]
fn connection_document_validation_rejects_each_identity_boundary() {
    let temp = tempfile::tempdir().unwrap();
    let path = connections_path(temp.path());
    let google_root = root(CloudProvider::GoogleDrive, "google-account");
    let valid = connection(&google_root);

    let mut cases = Vec::new();

    let mut wrong_length = valid.clone();
    wrong_length.connection_id = "0".repeat(63);
    cases.push((wrong_length, "oauth-connection-invalid"));

    let mut non_hex = valid.clone();
    non_hex.connection_id = "z".repeat(64);
    cases.push((non_hex, "oauth-connection-invalid"));

    let mut blank_root = valid.clone();
    blank_root.cloud_root_id = "   ".into();
    cases.push((blank_root, "oauth-connection-invalid"));

    let mut blank_path = valid.clone();
    blank_path.cloud_root_path = "   ".into();
    cases.push((blank_path, "oauth-connection-invalid"));

    let mut relative_path = valid.clone();
    relative_path.cloud_root_path = "relative/cloud".into();
    cases.push((relative_path, "oauth-connection-invalid"));

    let mut wrong_scope = valid.clone();
    wrong_scope.scope = "https://www.googleapis.com/auth/drive.file".into();
    cases.push((wrong_scope, "oauth-connection-invalid"));

    let mut wrong_client = valid.clone();
    wrong_client.client_id = "not-a-google-client".into();
    cases.push((wrong_client, "oauth-client-id-provider-format-invalid"));

    let mut mismatched_id = valid.clone();
    mismatched_id.connection_id = "0".repeat(64);
    cases.push((mismatched_id, "oauth-connection-id-mismatch"));

    for (candidate, expected_error) in cases {
        write_document(&path, &[candidate]);
        assert_eq!(load_connections(&path).unwrap_err(), expected_error);
    }

    write_document(&path, &vec![valid; 33]);
    assert_eq!(
        load_connections(&path).unwrap_err(),
        "oauth-connection-document-version-or-count-invalid"
    );
}
