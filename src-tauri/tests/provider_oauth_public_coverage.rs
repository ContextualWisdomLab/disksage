//! Public-contract coverage for provider OAuth validation and connection-document admission.
//!
//! These regressions exercise deterministic, credential-free production boundaries only. They do
//! not contact providers, open browser flows, or read/write the operating-system credential store.

use disksage_lib::cloud::CloudProvider;
use disksage_lib::provider_oauth::{connections_path, load_connections, requested_scope, validate_client_id};

const MICROSOFT_CLIENT_ID: &str = "12345678-1234-4abc-8def-1234567890ab";
const GOOGLE_CLIENT_ID: &str = "1234567890-abcxyz.apps.googleusercontent.com";

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
    assert_eq!(
        validate_client_id(CloudProvider::Onedrive, "not-a-guid").unwrap_err(),
        "oauth-client-id-provider-format-invalid"
    );
    assert_eq!(
        validate_client_id(CloudProvider::GoogleDrive, "missing-provider-suffix").unwrap_err(),
        "oauth-client-id-provider-format-invalid"
    );
    assert_eq!(
        validate_client_id(CloudProvider::GoogleDrive, ".apps.googleusercontent.com").unwrap_err(),
        "oauth-client-id-provider-format-invalid"
    );
    assert_eq!(
        validate_client_id(CloudProvider::GoogleDrive, "abc_.apps.googleusercontent.com").unwrap_err(),
        "oauth-client-id-provider-format-invalid"
    );
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
