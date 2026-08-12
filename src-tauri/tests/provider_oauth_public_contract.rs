use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::provider_oauth::{
    connection_for_root, connections_path, load_connections, requested_scope, validate_client_id,
    OAuthConnection,
};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;

const MICROSOFT_CLIENT_ID: &str = "ABCDEF12-3456-7890-ABCD-EF1234567890";
const GOOGLE_CLIENT_ID: &str = "12345-abcXYZ.apps.googleusercontent.com";

fn root(provider: CloudProvider) -> CloudRoot {
    #[cfg(windows)]
    let path = r"C:\Cloud";
    #[cfg(not(windows))]
    let path = "/Cloud";

    CloudRoot {
        id: format!("{}:public-contract", provider.as_str()),
        provider,
        account_scope: CloudAccountScope::Unknown,
        label: "Cloud".into(),
        path: path.into(),
        readable: true,
        access_issue: None,
    }
}

fn connection_id_for_root(root: &CloudRoot) -> String {
    let mut hasher = Sha256::new();
    for value in [root.provider.as_str(), root.id.as_str(), root.path.as_str()] {
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

fn connection(provider: CloudProvider) -> OAuthConnection {
    let root = root(provider);
    OAuthConnection {
        connection_id: connection_id_for_root(&root),
        provider,
        cloud_root_id: root.id,
        cloud_root_path: root.path,
        client_id: match provider {
            CloudProvider::Onedrive => MICROSOFT_CLIENT_ID,
            CloudProvider::GoogleDrive => GOOGLE_CLIENT_ID,
            CloudProvider::Icloud => "unsupported",
        }
        .into(),
        scope: requested_scope(provider).unwrap_or_default().into(),
        connected_at_ms: 123,
    }
}

#[test]
fn public_provider_scope_and_client_id_admission_fail_closed() {
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
        "ABCDEF12-3456-7890-ABCD-EF123456789",
        "ABCDEF12-3456-7890-ABCD-EF123456789Z",
        "abcdef1234567890abcdef1234567890",
    ] {
        assert_eq!(
            validate_client_id(CloudProvider::Onedrive, invalid).unwrap_err(),
            "oauth-client-id-provider-format-invalid"
        );
    }

    for invalid in [
        ".apps.googleusercontent.com",
        "client_with_underscore.apps.googleusercontent.com",
        "client.example.apps.googleusercontent.com",
        "client.apps.googleusercontent.com.example",
    ] {
        assert_eq!(
            validate_client_id(CloudProvider::GoogleDrive, invalid).unwrap_err(),
            "oauth-client-id-provider-format-invalid"
        );
    }

    for invalid in [
        String::new(),
        " leading-space".into(),
        "trailing-space ".into(),
        "line\nbreak".into(),
        "클라이언트.apps.googleusercontent.com".into(),
        "a".repeat(513),
    ] {
        assert_eq!(
            validate_client_id(CloudProvider::GoogleDrive, &invalid).unwrap_err(),
            "oauth-client-id-invalid"
        );
    }

    assert_eq!(
        validate_client_id(CloudProvider::Icloud, MICROSOFT_CLIENT_ID).unwrap_err(),
        "icloud-oauth-not-supported"
    );
}

#[test]
fn public_connection_document_reader_rejects_unsafe_or_invalid_shapes() {
    let temp = tempfile::tempdir().unwrap();
    let path = connections_path(temp.path());
    assert_eq!(path, temp.path().join("cloud-oauth-connections.json"));
    assert!(load_connections(&path).unwrap().is_empty());

    std::fs::write(&path, b"{\"version\":1,\"connections\":[]}").unwrap();
    assert!(load_connections(&path).unwrap().is_empty());

    std::fs::write(&path, b"{\"version\":2,\"connections\":[]}").unwrap();
    assert_eq!(
        load_connections(&path).unwrap_err(),
        "oauth-connection-document-version-or-count-invalid"
    );

    std::fs::write(&path, b"not-json").unwrap();
    assert_eq!(
        load_connections(&path).unwrap_err(),
        "oauth-connection-document-invalid"
    );

    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    assert_eq!(
        load_connections(&path).unwrap_err(),
        "oauth-connection-document-not-regular-file"
    );
}

#[test]
fn public_connection_document_reader_rejects_invalid_records_and_excess_count() {
    let temp = tempfile::tempdir().unwrap();
    let path = connections_path(temp.path());

    let mut invalid = connection(CloudProvider::Onedrive);
    invalid.connection_id = "0".repeat(63);
    std::fs::write(
        &path,
        serde_json::to_vec(&serde_json::json!({
            "version": 1,
            "connections": [invalid],
        }))
        .unwrap(),
    )
    .unwrap();
    assert_eq!(load_connections(&path).unwrap_err(), "oauth-connection-invalid");

    let mut mismatched = connection(CloudProvider::Onedrive);
    mismatched.connection_id = "0".repeat(64);
    std::fs::write(
        &path,
        serde_json::to_vec(&serde_json::json!({
            "version": 1,
            "connections": [mismatched],
        }))
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        load_connections(&path).unwrap_err(),
        "oauth-connection-id-mismatch"
    );

    let repeated = vec![connection(CloudProvider::GoogleDrive); 33];
    std::fs::write(
        &path,
        serde_json::to_vec(&serde_json::json!({
            "version": 1,
            "connections": repeated,
        }))
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        load_connections(&path).unwrap_err(),
        "oauth-connection-document-version-or-count-invalid"
    );
}

#[test]
fn public_connection_document_reader_bounds_size_before_reading() {
    let temp = tempfile::tempdir().unwrap();
    let path = connections_path(temp.path());
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(256 * 1024 + 1).unwrap();

    assert_eq!(
        load_connections(&path).unwrap_err(),
        "oauth-connection-document-too-large"
    );
}

#[test]
fn public_connection_lookup_reports_missing_without_credentials() {
    for provider in [CloudProvider::Onedrive, CloudProvider::GoogleDrive] {
        assert_eq!(
            connection_for_root(&[], &root(provider)).unwrap_err(),
            "provider-oauth-connection-missing"
        );
    }
}

#[test]
fn public_connection_lookup_accepts_exact_record_and_rejects_ambiguity() {
    for provider in [CloudProvider::Onedrive, CloudProvider::GoogleDrive] {
        let root = root(provider);
        let exact = connection(provider);
        assert_eq!(
            connection_for_root(std::slice::from_ref(&exact), &root).unwrap(),
            exact
        );
        assert_eq!(
            connection_for_root(&[exact.clone(), exact], &root).unwrap_err(),
            "provider-oauth-connection-ambiguous"
        );
    }
}

#[cfg(unix)]
#[test]
fn public_connection_document_reader_rejects_symlink_identity() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("target.json");
    std::fs::write(&target, b"{\"version\":1,\"connections\":[]}").unwrap();
    let link = temp.path().join("cloud-oauth-connections.json");
    symlink(&target, &link).unwrap();

    assert_eq!(
        load_connections(&link).unwrap_err(),
        "oauth-connection-document-not-regular-file"
    );
}
