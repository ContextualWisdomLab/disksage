use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::provider_oauth::{
    connection_for_root, connections_path, load_connections, requested_scope, validate_client_id,
    OAuthConnection,
};
use sha2::{Digest, Sha256};

const MICROSOFT_CLIENT_ID: &str = "12345678-1234-4abc-8def-1234567890ab";
const GOOGLE_CLIENT_ID: &str = "1234567890-abcxyz.apps.googleusercontent.com";

fn root(provider: CloudProvider, suffix: &str) -> CloudRoot {
    #[cfg(windows)]
    let path = format!(r"C:\Cloud\{suffix}");
    #[cfg(not(windows))]
    let path = format!("/Cloud/{suffix}");

    CloudRoot {
        id: format!("{}:{suffix}", provider.as_str()),
        provider,
        account_scope: CloudAccountScope::Unknown,
        label: suffix.to_string(),
        path,
        readable: true,
        access_issue: None,
    }
}

fn connection_id(root: &CloudRoot) -> String {
    let mut hasher = Sha256::new();
    for value in [root.provider.as_str(), root.id.as_str(), root.path.as_str()] {
        hasher.update(value.as_bytes());
        hasher.update([0]);
    }
    format!("{:x}", hasher.finalize())
}

fn connection(root: &CloudRoot) -> OAuthConnection {
    OAuthConnection {
        connection_id: connection_id(root),
        provider: root.provider,
        cloud_root_id: root.id.clone(),
        cloud_root_path: root.path.clone(),
        client_id: match root.provider {
            CloudProvider::Onedrive => MICROSOFT_CLIENT_ID,
            CloudProvider::GoogleDrive => GOOGLE_CLIENT_ID,
            CloudProvider::Icloud => "unsupported",
        }
        .to_string(),
        scope: requested_scope(root.provider).unwrap_or_default().to_string(),
        connected_at_ms: 123,
    }
}

#[test]
fn public_client_id_contract_rejects_malformed_provider_identities() {
    assert_eq!(
        requested_scope(CloudProvider::Icloud).unwrap_err(),
        "icloud-oauth-not-supported"
    );

    assert!(validate_client_id(
        CloudProvider::Onedrive,
        "ABCDEF12-3456-7890-abcd-EF1234567890"
    )
    .is_ok());
    assert!(validate_client_id(
        CloudProvider::GoogleDrive,
        "desktop-client-42.apps.googleusercontent.com"
    )
    .is_ok());

    for invalid in [
        "1234567-1234-4abc-8def-1234567890ab",
        "12345678-1234-4abz-8def-1234567890ab",
        "12345678-1234-4abc-8def-1234567890ab-extra",
    ] {
        assert_eq!(
            validate_client_id(CloudProvider::Onedrive, invalid).unwrap_err(),
            "oauth-client-id-provider-format-invalid"
        );
    }

    for invalid in [
        ".apps.googleusercontent.com",
        "client_name.apps.googleusercontent.com",
        "client.example.com",
    ] {
        assert_eq!(
            validate_client_id(CloudProvider::GoogleDrive, invalid).unwrap_err(),
            "oauth-client-id-provider-format-invalid"
        );
    }

    let oversized = "a".repeat(513);
    for invalid in [" client", "client ", "client\n", "클라이언트", oversized.as_str()] {
        assert_eq!(
            validate_client_id(CloudProvider::GoogleDrive, invalid).unwrap_err(),
            "oauth-client-id-invalid"
        );
    }
    assert_eq!(
        validate_client_id(CloudProvider::Icloud, MICROSOFT_CLIENT_ID).unwrap_err(),
        "icloud-oauth-not-supported"
    );
}

#[test]
fn connection_document_admission_is_fail_closed_before_identity_use() {
    let temp = tempfile::tempdir().unwrap();
    let path = connections_path(temp.path());
    assert_eq!(load_connections(&path).unwrap(), Vec::<OAuthConnection>::new());

    std::fs::create_dir(&path).unwrap();
    assert_eq!(
        load_connections(&path).unwrap_err(),
        "oauth-connection-document-not-regular-file"
    );
    std::fs::remove_dir(&path).unwrap();

    std::fs::write(&path, vec![b'x'; 256 * 1024 + 1]).unwrap();
    assert_eq!(
        load_connections(&path).unwrap_err(),
        "oauth-connection-document-too-large"
    );

    std::fs::write(&path, br#"{"version":2,"connections":[]}"#.replace("\\\"", "\"")).unwrap();
    assert_eq!(
        load_connections(&path).unwrap_err(),
        "oauth-connection-document-version-or-count-invalid"
    );

    let too_many = serde_json::json!({
        "version": 1,
        "connections": (0..33).map(|_| serde_json::json!({})).collect::<Vec<_>>()
    });
    std::fs::write(&path, serde_json::to_vec(&too_many).unwrap()).unwrap();
    assert_eq!(
        load_connections(&path).unwrap_err(),
        "oauth-connection-document-version-or-count-invalid"
    );
}

#[test]
fn connection_lookup_rejects_ambiguous_invalid_and_wrong_root_records() {
    let requested = root(CloudProvider::GoogleDrive, "primary");
    let valid = connection(&requested);
    assert_eq!(
        connection_for_root(std::slice::from_ref(&valid), &requested).unwrap(),
        valid
    );

    let mut duplicate = valid.clone();
    duplicate.connected_at_ms += 1;
    assert_eq!(
        connection_for_root(&[valid.clone(), duplicate], &requested).unwrap_err(),
        "provider-oauth-connection-ambiguous"
    );

    let mut tampered = valid.clone();
    tampered.connection_id = "0".repeat(64);
    assert_eq!(
        connection_for_root(&[tampered], &requested).unwrap_err(),
        "provider-oauth-connection-missing"
    );

    let other = root(CloudProvider::GoogleDrive, "other");
    assert_eq!(
        connection_for_root(&[connection(&other)], &requested).unwrap_err(),
        "provider-oauth-connection-missing"
    );
}
