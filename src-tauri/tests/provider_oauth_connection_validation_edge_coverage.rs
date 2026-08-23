//! Read-only edge coverage for durable provider OAuth connection admission.
//!
//! These regressions exercise the public connection-document parser and deterministic root lookup
//! against malformed authority metadata. They use only private temporary files and never launch a
//! browser, open a loopback listener, contact a provider, or access the OS credential store.

use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::provider_oauth::{
    connection_for_root, load_connections, requested_scope, OAuthConnection,
};
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

const GOOGLE_CLIENT_ID: &str = "1234567890-abcxyz.apps.googleusercontent.com";
const MICROSOFT_CLIENT_ID: &str = "12345678-1234-4abc-8def-1234567890ab";

fn root(provider: CloudProvider) -> CloudRoot {
    #[cfg(windows)]
    let path = r"C:\Cloud\Drive";
    #[cfg(not(windows))]
    let path = "/Cloud/Drive";

    CloudRoot {
        id: format!("{}:coverage-account", provider.as_str()),
        provider,
        account_scope: CloudAccountScope::Unknown,
        label: "Coverage cloud root".into(),
        path: path.into(),
        readable: true,
        access_issue: None,
    }
}

fn canonical_connection_id(root: &CloudRoot) -> String {
    let root_id = root.id.nfc().collect::<String>();
    let root_path = root.path.nfc().collect::<String>();
    let mut hasher = Sha256::new();
    for value in [root.provider.as_str(), root_id.as_str(), root_path.as_str()] {
        hasher.update(value.as_bytes());
        hasher.update([0]);
    }
    use std::fmt::Write as _;
    let digest = hasher.finalize();
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut encoded, "{byte:02x}").expect("writing to String cannot fail");
    }
    encoded
}

fn connection(root: &CloudRoot) -> OAuthConnection {
    OAuthConnection {
        connection_id: canonical_connection_id(root),
        provider: root.provider,
        cloud_root_id: root.id.clone(),
        cloud_root_path: root.path.clone(),
        client_id: match root.provider {
            CloudProvider::GoogleDrive => GOOGLE_CLIENT_ID,
            CloudProvider::Onedrive => MICROSOFT_CLIENT_ID,
            CloudProvider::Icloud => MICROSOFT_CLIENT_ID,
        }
        .into(),
        scope: requested_scope(root.provider).unwrap_or_default().into(),
        connected_at_ms: 42,
    }
}

fn write_private(path: &std::path::Path, value: &OAuthConnection) {
    let bytes = serde_json::to_vec(&serde_json::json!({
        "version": 1,
        "connections": [value]
    }))
    .unwrap();
    std::fs::write(path, bytes).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
}

#[test]
fn non_directory_parent_is_rejected_before_document_observation() {
    let temp = tempfile::tempdir().unwrap();
    let parent_file = temp.path().join("not-a-directory");
    std::fs::write(&parent_file, b"outside authority").unwrap();

    assert_eq!(
        load_connections(&parent_file.join("connections.json")).unwrap_err(),
        "oauth-connection-directory-unsafe"
    );
}

#[test]
fn malformed_connection_identity_fields_fail_closed_at_the_public_document_boundary() {
    let temp = tempfile::tempdir().unwrap();
    let google_root = root(CloudProvider::GoogleDrive);
    let valid = connection(&google_root);

    let mut non_hex_id = valid.clone();
    non_hex_id.connection_id = "g".repeat(64);

    let mut mismatched_id = valid.clone();
    mismatched_id.connection_id = "0".repeat(64);

    let mut whitespace_path = valid;
    whitespace_path.cloud_root_path = "   ".into();

    for (index, (candidate, expected)) in [
        (non_hex_id, "oauth-connection-invalid"),
        (mismatched_id, "oauth-connection-id-mismatch"),
        (whitespace_path, "oauth-connection-invalid"),
    ]
    .into_iter()
    .enumerate()
    {
        let path = temp.path().join(format!("invalid-{index}.json"));
        write_private(&path, &candidate);
        assert_eq!(load_connections(&path).unwrap_err(), expected);
    }
}

#[test]
fn malformed_connection_fields_are_rejected_before_identity_lookup() {
    let temp = tempfile::tempdir().unwrap();
    let google_root = root(CloudProvider::GoogleDrive);
    let valid = connection(&google_root);

    let mut whitespace_root_id = valid.clone();
    whitespace_root_id.cloud_root_id = "   ".into();

    let mut relative_root_path = valid.clone();
    relative_root_path.cloud_root_path = "relative/cloud/root".into();

    let mut wrong_scope = valid.clone();
    wrong_scope.scope = "Files.Read".into();

    let mut malformed_client = valid;
    malformed_client.client_id = "not-a-google-client.apps.googleusercontent.invalid".into();

    for (index, (candidate, expected)) in [
        (whitespace_root_id, "oauth-connection-invalid"),
        (relative_root_path, "oauth-connection-invalid"),
        (wrong_scope, "oauth-connection-invalid"),
        (malformed_client, "oauth-client-id-provider-format-invalid"),
    ]
    .into_iter()
    .enumerate()
    {
        let path = temp.path().join(format!("field-invalid-{index}.json"));
        write_private(&path, &candidate);
        assert_eq!(load_connections(&path).unwrap_err(), expected);
    }
}

#[test]
fn unsupported_provider_connection_and_cross_provider_lookup_do_not_authorize() {
    let temp = tempfile::tempdir().unwrap();
    let icloud_root = root(CloudProvider::Icloud);
    let icloud_connection = connection(&icloud_root);
    let path = temp.path().join("icloud.json");
    write_private(&path, &icloud_connection);
    assert_eq!(
        load_connections(&path).unwrap_err(),
        "icloud-oauth-not-supported"
    );

    let google_root = root(CloudProvider::GoogleDrive);
    let google_connection = connection(&google_root);
    let onedrive_root = root(CloudProvider::Onedrive);
    assert_eq!(
        connection_for_root(std::slice::from_ref(&google_connection), &onedrive_root).unwrap_err(),
        "provider-oauth-connection-missing"
    );
}
