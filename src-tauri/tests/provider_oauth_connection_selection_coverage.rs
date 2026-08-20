//! Credential-free coverage for positive, ambiguous, and malformed OAuth connection selection.
//!
//! The fixtures reproduce DiskSage's stable connection identifier from public root fields without
//! opening a browser, reading the credential store, or contacting a provider.

use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::provider_oauth::{
    connection_for_root, connections_path, load_connections, requested_scope, OAuthConnection,
};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;

const MICROSOFT_CLIENT_ID: &str = "12345678-1234-4abc-8def-1234567890ab";
const GOOGLE_CLIENT_ID: &str = "1234567890-abcxyz.apps.googleusercontent.com";

fn root(provider: CloudProvider) -> CloudRoot {
    #[cfg(windows)]
    let path = match provider {
        CloudProvider::Onedrive => r"C:\Cloud\OneDrive",
        CloudProvider::GoogleDrive => r"C:\Cloud\GoogleDrive",
        CloudProvider::Icloud => r"C:\Cloud\iCloud",
    };
    #[cfg(not(windows))]
    let path = match provider {
        CloudProvider::Onedrive => "/Cloud/OneDrive",
        CloudProvider::GoogleDrive => "/Cloud/GoogleDrive",
        CloudProvider::Icloud => "/Cloud/iCloud",
    };

    CloudRoot {
        id: format!("{}:selection-coverage", provider.as_str()),
        provider,
        account_scope: CloudAccountScope::Unknown,
        label: "Selection coverage".into(),
        path: path.into(),
        readable: true,
        access_issue: None,
    }
}

fn stable_connection_id(root: &CloudRoot) -> String {
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

fn connection(root: &CloudRoot) -> OAuthConnection {
    let client_id = match root.provider {
        CloudProvider::Onedrive => MICROSOFT_CLIENT_ID,
        CloudProvider::GoogleDrive => GOOGLE_CLIENT_ID,
        CloudProvider::Icloud => unreachable!("iCloud has no OAuth connection"),
    };
    OAuthConnection {
        connection_id: stable_connection_id(root),
        provider: root.provider,
        cloud_root_id: root.id.clone(),
        cloud_root_path: root.path.clone(),
        client_id: client_id.into(),
        scope: requested_scope(root.provider).unwrap().into(),
        connected_at_ms: 7,
    }
}

fn make_private(path: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
}

fn write_private(path: &std::path::Path, bytes: impl AsRef<[u8]>) {
    std::fs::write(path, bytes).unwrap();
    make_private(path);
}

#[test]
fn exact_connection_is_selected_and_duplicate_exact_connections_fail_ambiguous() {
    let target = root(CloudProvider::Onedrive);
    let bound = connection(&target);

    assert_eq!(
        connection_for_root(std::slice::from_ref(&bound), &target).unwrap(),
        bound
    );
    assert_eq!(
        connection_for_root(&[bound.clone(), bound], &target).unwrap_err(),
        "provider-oauth-connection-ambiguous"
    );
}

#[test]
fn valid_connection_document_round_trips_and_nonmatching_provider_is_ignored() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("connections.json");
    let onedrive = root(CloudProvider::Onedrive);
    let google = root(CloudProvider::GoogleDrive);
    let document = serde_json::json!({
        "version": 1,
        "connections": [connection(&onedrive), connection(&google)]
    });
    write_private(&path, serde_json::to_vec(&document).unwrap());

    let loaded = load_connections(&path).unwrap();
    assert_eq!(loaded.len(), 2);
    assert_eq!(connection_for_root(&loaded, &onedrive).unwrap().provider, CloudProvider::Onedrive);

    let unrelated = root(CloudProvider::GoogleDrive);
    let only_onedrive = vec![connection(&onedrive)];
    assert_eq!(
        connection_for_root(&only_onedrive, &unrelated).unwrap_err(),
        "provider-oauth-connection-missing"
    );
}

#[test]
fn persisted_connection_shape_validation_fails_closed_by_field() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("connections.json");
    let target = root(CloudProvider::Onedrive);
    let baseline = connection(&target);

    let mut cases = Vec::new();

    let mut short_connection_id = baseline.clone();
    short_connection_id.connection_id = "0".repeat(63);
    cases.push(short_connection_id);

    let mut non_hex_connection_id = baseline.clone();
    non_hex_connection_id.connection_id = "g".repeat(64);
    cases.push(non_hex_connection_id);

    let mut empty_root_id = baseline.clone();
    empty_root_id.cloud_root_id.clear();
    cases.push(empty_root_id);

    let mut whitespace_root_id = baseline.clone();
    whitespace_root_id.cloud_root_id = "   ".into();
    cases.push(whitespace_root_id);

    let mut empty_root_path = baseline.clone();
    empty_root_path.cloud_root_path.clear();
    cases.push(empty_root_path);

    let mut relative_root = baseline.clone();
    relative_root.cloud_root_path = "relative/root".into();
    cases.push(relative_root);

    let mut wrong_scope = baseline.clone();
    wrong_scope.scope = "Files.ReadWrite".into();
    cases.push(wrong_scope);

    let mut malformed_client = baseline;
    malformed_client.client_id = "not-a-microsoft-client-id".into();
    cases.push(malformed_client);

    for invalid in cases {
        let document = serde_json::json!({"version": 1, "connections": [invalid]});
        write_private(&path, serde_json::to_vec(&document).unwrap());
        assert!(load_connections(&path).is_err());
    }
}

#[test]
fn well_formed_but_wrong_connection_id_reaches_identity_binding_guard() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("connections.json");
    let target = root(CloudProvider::Onedrive);
    let mut wrong_id = connection(&target);
    wrong_id.connection_id = "0".repeat(64);
    assert_ne!(wrong_id.connection_id, stable_connection_id(&target));
    let document = serde_json::json!({"version": 1, "connections": [wrong_id]});
    write_private(&path, serde_json::to_vec(&document).unwrap());

    assert_eq!(load_connections(&path).unwrap_err(), "oauth-connection-id-mismatch");
}

#[test]
fn persisted_icloud_oauth_identity_fails_at_the_unsupported_provider_boundary() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("connections.json");
    let icloud = root(CloudProvider::Icloud);
    let invalid = OAuthConnection {
        connection_id: "0".repeat(64),
        provider: CloudProvider::Icloud,
        cloud_root_id: icloud.id,
        cloud_root_path: icloud.path,
        client_id: MICROSOFT_CLIENT_ID.into(),
        scope: "Files.Read offline_access".into(),
        connected_at_ms: 7,
    };
    let document = serde_json::json!({"version": 1, "connections": [invalid]});
    write_private(&path, serde_json::to_vec(&document).unwrap());

    assert_eq!(
        load_connections(&path).unwrap_err(),
        "icloud-oauth-not-supported"
    );
}

#[test]
fn connection_document_admission_bounds_fail_closed_before_connection_use() {
    let temp = tempfile::tempdir().unwrap();
    let missing = temp.path().join("missing.json");
    assert!(load_connections(&missing).unwrap().is_empty());

    let directory = temp.path().join("directory");
    std::fs::create_dir(&directory).unwrap();
    assert_eq!(
        load_connections(&directory).unwrap_err(),
        "oauth-connection-document-not-regular-file"
    );

    let malformed = temp.path().join("malformed.json");
    write_private(&malformed, b"{not-json");
    assert_eq!(
        load_connections(&malformed).unwrap_err(),
        "oauth-connection-document-invalid"
    );

    let wrong_version = temp.path().join("wrong-version.json");
    write_private(
        &wrong_version,
        serde_json::to_vec(&serde_json::json!({"version": 2, "connections": []})).unwrap(),
    );
    assert_eq!(
        load_connections(&wrong_version).unwrap_err(),
        "oauth-connection-document-version-or-count-invalid"
    );

    let too_many = temp.path().join("too-many.json");
    let repeated = vec![connection(&root(CloudProvider::Onedrive)); 33];
    write_private(
        &too_many,
        serde_json::to_vec(&serde_json::json!({"version": 1, "connections": repeated})).unwrap(),
    );
    assert_eq!(
        load_connections(&too_many).unwrap_err(),
        "oauth-connection-document-version-or-count-invalid"
    );

    let oversized = temp.path().join("oversized.json");
    write_private(&oversized, vec![b' '; 256 * 1024 + 1]);
    assert_eq!(
        load_connections(&oversized).unwrap_err(),
        "oauth-connection-document-too-large"
    );

    #[cfg(unix)]
    {
        let regular = temp.path().join("regular.json");
        let symlink = temp.path().join("connections-link.json");
        std::fs::write(&regular, br#"{"version":1,"connections":[]}"#).unwrap();
        std::os::unix::fs::symlink(&regular, &symlink).unwrap();
        assert_eq!(
            load_connections(&symlink).unwrap_err(),
            "oauth-connection-document-not-regular-file"
        );
    }
}

#[test]
fn connections_path_is_stably_scoped_under_application_data() {
    let temp = tempfile::tempdir().unwrap();
    assert_eq!(
        connections_path(temp.path()),
        temp.path().join("cloud-oauth-connections.json")
    );
}

#[cfg(unix)]
#[test]
fn unreadable_connection_document_fails_closed_after_metadata_admission() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("connections.json");
    std::fs::write(&path, br#"{"version":1,"connections":[]}"#).unwrap();
    let original = std::fs::metadata(&path).unwrap().permissions();
    let mut denied = original.clone();
    denied.set_mode(0o000);
    std::fs::set_permissions(&path, denied).unwrap();

    let result = load_connections(&path);

    std::fs::set_permissions(&path, original).unwrap();
    assert_eq!(result.unwrap_err(), "oauth-connection-document-unreadable");
}
