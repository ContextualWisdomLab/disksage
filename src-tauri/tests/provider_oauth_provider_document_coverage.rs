use disksage_lib::cloud::CloudProvider;
use disksage_lib::provider_oauth::{
    connections_path, load_connections, prepare_authorization,
    prepare_authorization_with_write_access,
};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::fs;

fn connection_id(provider: CloudProvider, root_id: &str, root_path: &str) -> String {
    let mut hasher = Sha256::new();
    for value in [provider.as_str(), root_id, root_path] {
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

fn write_connection(
    path: &std::path::Path,
    provider: &str,
    connection_id: String,
    root_id: &str,
    root_path: &str,
    client_id: &str,
    scope: &str,
) {
    fs::write(
        path,
        serde_json::to_vec(&serde_json::json!({
            "version": 1,
            "connections": [{
                "connection_id": connection_id,
                "provider": provider,
                "cloud_root_id": root_id,
                "cloud_root_path": root_path,
                "client_id": client_id,
                "scope": scope,
                "connected_at_ms": 1
            }]
        }))
        .unwrap(),
    )
    .unwrap();
}

fn query_parameter<'a>(url: &'a str, key: &str) -> &'a str {
    url.split_once('?')
        .and_then(|(_, query)| {
            query.split('&').find_map(|field| {
                let (candidate, value) = field.split_once('=')?;
                (candidate == key).then_some(value)
            })
        })
        .unwrap_or_else(|| panic!("missing {key} in authorization URL"))
}

#[test]
fn onedrive_read_authorization_binds_ephemeral_loopback_and_pkce() {
    let pending = prepare_authorization(
        CloudProvider::Onedrive,
        "01234567-89ab-cdef-0123-456789abcdef",
    )
    .unwrap();
    let url = pending.authorization_url();

    assert!(url.starts_with(
        "https://login.microsoftonline.com/common/oauth2/v2.0/authorize?"
    ));
    assert!(url.contains("redirect_uri=http%3A%2F%2Flocalhost%3A"));
    assert!(url.contains("scope=Files.Read%20offline_access"));
    assert!(url.contains("response_mode=query"));
    assert!(url.contains("prompt=select_account"));
    assert_eq!(query_parameter(url, "code_challenge_method"), "S256");
    assert_eq!(query_parameter(url, "state").len(), 43);
    assert_eq!(query_parameter(url, "code_challenge").len(), 43);
}

#[test]
fn google_write_authorization_uses_write_scope_and_offline_consent() {
    let pending = prepare_authorization_with_write_access(
        CloudProvider::GoogleDrive,
        "client-123.apps.googleusercontent.com",
        true,
    )
    .unwrap();
    let url = pending.authorization_url();

    assert!(url.starts_with("https://accounts.google.com/o/oauth2/v2/auth?"));
    assert!(url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A"));
    assert!(url.contains(
        "scope=https%3A%2F%2Fwww.googleapis.com%2Fauth%2Fdrive"
    ));
    assert!(url.contains("access_type=offline"));
    assert!(url.contains("prompt=consent"));
    assert!(url.contains("include_granted_scopes=true"));
    assert_eq!(query_parameter(url, "code_challenge_method"), "S256");
    assert_eq!(query_parameter(url, "state").len(), 43);
    assert_eq!(query_parameter(url, "code_challenge").len(), 43);
}

#[test]
fn authorization_preparation_fails_before_listener_authority_for_unsupported_or_invalid_clients() {
    assert_eq!(
        prepare_authorization(
            CloudProvider::Icloud,
            "01234567-89ab-cdef-0123-456789abcdef"
        )
        .err()
        .unwrap(),
        "icloud-oauth-not-supported"
    );
    assert_eq!(
        prepare_authorization(CloudProvider::GoogleDrive, "not-a-google-client")
            .err()
            .unwrap(),
        "oauth-client-id-provider-format-invalid"
    );
}

#[test]
fn google_drive_write_connection_document_is_admitted() {
    let directory = tempfile::tempdir().unwrap();
    let path = connections_path(directory.path());
    let root_id = "google-root";
    let root_path = "/tmp/google-drive-root";

    write_connection(
        &path,
        "google-drive",
        connection_id(CloudProvider::GoogleDrive, root_id, root_path),
        root_id,
        root_path,
        "client-123.apps.googleusercontent.com",
        "https://www.googleapis.com/auth/drive",
    );

    let connections = load_connections(&path).unwrap();
    assert_eq!(connections.len(), 1);
    assert_eq!(connections[0].provider, CloudProvider::GoogleDrive);
    assert_eq!(connections[0].scope, "https://www.googleapis.com/auth/drive");
}

#[test]
fn legacy_decomposed_unicode_connection_identity_remains_readable() {
    let directory = tempfile::tempdir().unwrap();
    let path = connections_path(directory.path());
    let root_id = "Cafe\u{301}";
    let root_path = "/tmp/Cafe\u{301}";

    write_connection(
        &path,
        "google-drive",
        connection_id(CloudProvider::GoogleDrive, root_id, root_path),
        root_id,
        root_path,
        "client.apps.googleusercontent.com",
        "https://www.googleapis.com/auth/drive.metadata.readonly",
    );

    let connections = load_connections(&path).unwrap();
    assert_eq!(connections.len(), 1);
    assert_eq!(connections[0].cloud_root_id, root_id);
    assert_eq!(connections[0].cloud_root_path, root_path);
}

#[test]
fn icloud_connection_document_fails_closed_at_oauth_provider_boundary() {
    let directory = tempfile::tempdir().unwrap();
    let path = connections_path(directory.path());
    let root_id = "icloud-root";
    let root_path = "/tmp/icloud-root";

    write_connection(
        &path,
        "icloud",
        connection_id(CloudProvider::Icloud, root_id, root_path),
        root_id,
        root_path,
        "01234567-89ab-cdef-0123-456789abcdef",
        "Files.Read offline_access",
    );

    assert_eq!(load_connections(&path).unwrap_err(), "icloud-oauth-not-supported");
}
