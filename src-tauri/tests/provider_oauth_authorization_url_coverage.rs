//! Credential-free coverage for OAuth authorization preparation.
//!
//! These tests bind only ephemeral loopback listeners. They do not open a browser, contact an
//! OAuth provider, read the credential store, or authorize cloud mutation.

use disksage_lib::cloud::CloudProvider;
use disksage_lib::provider_oauth::prepare_authorization;

const MICROSOFT_CLIENT_ID: &str = "12345678-1234-4abc-8def-1234567890ab";
const GOOGLE_CLIENT_ID: &str = "1234567890-abcxyz.apps.googleusercontent.com";

fn query_value<'a>(url: &'a str, key: &str) -> &'a str {
    let query = url.split_once('?').expect("authorization URL has query").1;
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find_map(|(candidate, value)| (candidate == key).then_some(value))
        .unwrap_or_else(|| panic!("missing OAuth query key: {key}"))
}

#[test]
fn onedrive_preparation_uses_loopback_pkce_and_read_only_scope() {
    let pending = prepare_authorization(CloudProvider::Onedrive, MICROSOFT_CLIENT_ID).unwrap();
    let url = pending.authorization_url();

    assert!(url.starts_with(
        "https://login.microsoftonline.com/common/oauth2/v2.0/authorize?"
    ));
    assert_eq!(query_value(url, "client_id"), MICROSOFT_CLIENT_ID);
    assert!(query_value(url, "redirect_uri").starts_with("http%3A%2F%2Flocalhost%3A"));
    assert_eq!(query_value(url, "response_type"), "code");
    assert_eq!(query_value(url, "scope"), "Files.Read%20offline_access");
    assert_eq!(query_value(url, "code_challenge_method"), "S256");
    assert_eq!(query_value(url, "response_mode"), "query");
    assert_eq!(query_value(url, "prompt"), "select_account");
    assert_eq!(query_value(url, "code_challenge").len(), 43);
    assert_eq!(query_value(url, "state").len(), 43);
}

#[test]
fn google_preparation_uses_ipv4_loopback_pkce_and_metadata_scope() {
    let pending = prepare_authorization(CloudProvider::GoogleDrive, GOOGLE_CLIENT_ID).unwrap();
    let url = pending.authorization_url();

    assert!(url.starts_with("https://accounts.google.com/o/oauth2/v2/auth?"));
    assert_eq!(query_value(url, "client_id"), GOOGLE_CLIENT_ID);
    assert!(query_value(url, "redirect_uri").starts_with("http%3A%2F%2F127.0.0.1%3A"));
    assert_eq!(
        query_value(url, "scope"),
        "https%3A%2F%2Fwww.googleapis.com%2Fauth%2Fdrive.metadata.readonly"
    );
    assert_eq!(query_value(url, "access_type"), "offline");
    assert_eq!(query_value(url, "prompt"), "consent");
    assert_eq!(query_value(url, "include_granted_scopes"), "true");
    assert_eq!(query_value(url, "code_challenge_method"), "S256");
    assert_eq!(query_value(url, "code_challenge").len(), 43);
    assert_eq!(query_value(url, "state").len(), 43);
}

#[test]
fn unsupported_or_malformed_clients_fail_before_authorization() {
    assert_eq!(
        prepare_authorization(CloudProvider::Icloud, MICROSOFT_CLIENT_ID)
            .err()
            .expect("iCloud OAuth must fail closed"),
        "icloud-oauth-not-supported"
    );
    assert_eq!(
        prepare_authorization(CloudProvider::GoogleDrive, "not-a-google-client")
            .err()
            .expect("malformed Google client ID must fail closed"),
        "oauth-client-id-provider-format-invalid"
    );
}
