//! Credential-free process-preparation coverage for provider OAuth authorization.
//!
//! These regressions bind only ephemeral loopback listeners and inspect the generated system-
//! browser URL. They never open a browser, contact a provider, exchange a code, read credentials,
//! or authorize cloud mutation.

use disksage_lib::cloud::CloudProvider;
use disksage_lib::provider_oauth::prepare_authorization;
use std::collections::BTreeMap;

const MICROSOFT_CLIENT_ID: &str = "12345678-1234-4abc-8def-1234567890ab";
const GOOGLE_CLIENT_ID: &str = "1234567890-abcxyz.apps.googleusercontent.com";

fn query_pairs(url: &str) -> BTreeMap<&str, &str> {
    let (_, query) = url
        .split_once('?')
        .expect("authorization URL must contain a query");
    query
        .split('&')
        .map(|pair| {
            pair.split_once('=')
                .expect("authorization query pair must contain '='")
        })
        .collect()
}

fn assert_urlsafe_random_parameter(value: &str) {
    assert_eq!(value.len(), 43);
    assert!(value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')));
}

fn authorization_error(provider: CloudProvider, client_id: &str) -> String {
    match prepare_authorization(provider, client_id) {
        Ok(_) => panic!("invalid provider authorization request unexpectedly succeeded"),
        Err(error) => error,
    }
}

#[test]
fn onedrive_authorization_preparation_is_pkce_bound_and_read_only() {
    let pending = prepare_authorization(CloudProvider::Onedrive, MICROSOFT_CLIENT_ID).unwrap();
    let url = pending.authorization_url();
    assert!(url.starts_with(
        "https://login.microsoftonline.com/common/oauth2/v2.0/authorize?"
    ));

    let pairs = query_pairs(url);
    assert_eq!(pairs.get("client_id"), Some(&MICROSOFT_CLIENT_ID));
    assert_eq!(pairs.get("response_type"), Some(&"code"));
    assert_eq!(pairs.get("scope"), Some(&"Files.Read%20offline_access"));
    assert_eq!(pairs.get("response_mode"), Some(&"query"));
    assert_eq!(pairs.get("prompt"), Some(&"select_account"));
    assert_eq!(pairs.get("code_challenge_method"), Some(&"S256"));
    assert!(pairs
        .get("redirect_uri")
        .is_some_and(|value| value.starts_with("http%3A%2F%2Flocalhost%3A")));
    assert_urlsafe_random_parameter(pairs["state"]);
    assert_urlsafe_random_parameter(pairs["code_challenge"]);
    assert!(!url.contains("code_verifier"));
    assert!(!url.contains("access_token"));
    assert!(!url.contains("refresh_token"));
}

#[test]
fn google_authorization_preparation_is_pkce_bound_and_metadata_only() {
    let pending = prepare_authorization(CloudProvider::GoogleDrive, GOOGLE_CLIENT_ID).unwrap();
    let url = pending.authorization_url();
    assert!(url.starts_with("https://accounts.google.com/o/oauth2/v2/auth?"));

    let pairs = query_pairs(url);
    assert_eq!(pairs.get("client_id"), Some(&GOOGLE_CLIENT_ID));
    assert_eq!(pairs.get("response_type"), Some(&"code"));
    assert_eq!(
        pairs.get("scope"),
        Some(&"https%3A%2F%2Fwww.googleapis.com%2Fauth%2Fdrive.metadata.readonly")
    );
    assert_eq!(pairs.get("access_type"), Some(&"offline"));
    assert_eq!(pairs.get("prompt"), Some(&"consent"));
    assert_eq!(pairs.get("include_granted_scopes"), Some(&"true"));
    assert_eq!(pairs.get("code_challenge_method"), Some(&"S256"));
    assert!(pairs
        .get("redirect_uri")
        .is_some_and(|value| value.starts_with("http%3A%2F%2F127.0.0.1%3A")));
    assert_urlsafe_random_parameter(pairs["state"]);
    assert_urlsafe_random_parameter(pairs["code_challenge"]);
    assert!(!url.contains("code_verifier"));
    assert!(!url.contains("access_token"));
    assert!(!url.contains("refresh_token"));
}

#[test]
fn authorization_preparation_rejects_invalid_identity_before_provider_activity() {
    assert_eq!(
        authorization_error(CloudProvider::Onedrive, "not-a-microsoft-client"),
        "oauth-client-id-provider-format-invalid"
    );
    assert_eq!(
        authorization_error(CloudProvider::GoogleDrive, "not-a-google-client"),
        "oauth-client-id-provider-format-invalid"
    );
    assert_eq!(
        authorization_error(CloudProvider::Icloud, MICROSOFT_CLIENT_ID),
        "icloud-oauth-not-supported"
    );
}
