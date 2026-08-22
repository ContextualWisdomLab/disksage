//! Credential-free public-boundary coverage for provider OAuth authorization preparation.
//!
//! These tests exercise the real pre-browser OAuth boundary: local loopback listener admission,
//! provider-specific redirect authority, PKCE generation, read-only scopes, and client-ID
//! validation. They do not launch a browser, contact a provider, touch the credential store, or
//! persist connection metadata.

use disksage_lib::cloud::CloudProvider;
use disksage_lib::provider_oauth::prepare_authorization;

const MICROSOFT_CLIENT_ID: &str = "12345678-1234-4abc-8def-1234567890ab";
const GOOGLE_CLIENT_ID: &str = "1234567890-abcxyz.apps.googleusercontent.com";

fn query_parameter<'a>(url: &'a str, name: &str) -> &'a str {
    let query = url
        .split_once('?')
        .map(|(_, query)| query)
        .expect("authorization URL must contain a query");
    query
        .split('&')
        .find_map(|pair| {
            let (key, value) = pair.split_once('=')?;
            (key == name).then_some(value)
        })
        .unwrap_or_else(|| panic!("authorization URL must contain {name}"))
}

#[test]
fn google_authorization_preparation_is_loopback_pkce_and_read_only() {
    let first = prepare_authorization(CloudProvider::GoogleDrive, GOOGLE_CLIENT_ID)
        .expect("Google authorization preparation should stay local and deterministic");
    let second = prepare_authorization(CloudProvider::GoogleDrive, GOOGLE_CLIENT_ID)
        .expect("a second preparation should bind an independent ephemeral listener");

    let first_url = first.authorization_url();
    let second_url = second.authorization_url();
    assert!(first_url.starts_with("https://accounts.google.com/o/oauth2/v2/auth?"));
    assert!(first_url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A"));
    assert!(first_url.contains(
        "scope=https%3A%2F%2Fwww.googleapis.com%2Fauth%2Fdrive.metadata.readonly"
    ));
    assert!(first_url.contains("access_type=offline"));
    assert!(first_url.contains("prompt=consent"));
    assert!(first_url.contains("include_granted_scopes=true"));
    assert!(first_url.contains("code_challenge_method=S256"));
    assert!(!first_url.contains("drive.file"));
    assert_eq!(query_parameter(first_url, "state").len(), 43);
    assert_eq!(query_parameter(first_url, "code_challenge").len(), 43);
    assert_ne!(
        query_parameter(first_url, "state"),
        query_parameter(second_url, "state"),
        "independent authorization preparations must not reuse CSRF state"
    );
    assert_ne!(
        query_parameter(first_url, "code_challenge"),
        query_parameter(second_url, "code_challenge"),
        "independent authorization preparations must not reuse PKCE challenges"
    );
}

#[test]
fn onedrive_authorization_preparation_uses_registered_localhost_and_read_only_scope() {
    let pending = prepare_authorization(CloudProvider::Onedrive, MICROSOFT_CLIENT_ID)
        .expect("OneDrive authorization preparation should stay local and deterministic");
    let url = pending.authorization_url();

    assert!(url.starts_with(
        "https://login.microsoftonline.com/common/oauth2/v2.0/authorize?"
    ));
    assert!(url.contains("redirect_uri=http%3A%2F%2Flocalhost%3A"));
    assert!(url.contains("scope=Files.Read%20offline_access"));
    assert!(url.contains("response_mode=query"));
    assert!(url.contains("prompt=select_account"));
    assert!(url.contains("code_challenge_method=S256"));
    assert!(!url.contains("ReadWrite"));
    assert_eq!(query_parameter(url, "state").len(), 43);
    assert_eq!(query_parameter(url, "code_challenge").len(), 43);
}

#[test]
fn unsupported_provider_and_malformed_clients_fail_before_browser_or_network_work() {
    assert_eq!(
        prepare_authorization(CloudProvider::Icloud, MICROSOFT_CLIENT_ID)
            .err()
            .expect("iCloud OAuth must remain unsupported"),
        "icloud-oauth-not-supported"
    );

    for (provider, client_id) in [
        (CloudProvider::GoogleDrive, "bad_prefix.apps.googleusercontent.com"),
        (CloudProvider::Onedrive, "not-a-guid"),
    ] {
        assert_eq!(
            prepare_authorization(provider, client_id)
                .err()
                .expect("malformed provider client IDs must fail closed"),
            "oauth-client-id-provider-format-invalid"
        );
    }
}

#[test]
fn client_id_bounds_fail_before_loopback_or_provider_work() {
    let oversized = format!("{}{}", "a".repeat(513), ".apps.googleusercontent.com");
    let invalid_clients = [
        "".to_string(),
        format!(" {GOOGLE_CLIENT_ID}"),
        format!("{GOOGLE_CLIENT_ID} "),
        oversized,
        "café.apps.googleusercontent.com".to_string(),
        "abc\u{0007}xyz.apps.googleusercontent.com".replace("\\u{0007}", "\u{0007}"),
    ];

    for client_id in invalid_clients {
        assert_eq!(
            prepare_authorization(CloudProvider::GoogleDrive, &client_id)
                .err()
                .expect("bounded client-ID validation must fail before listener or provider work"),
            "oauth-client-id-invalid"
        );
    }
}
