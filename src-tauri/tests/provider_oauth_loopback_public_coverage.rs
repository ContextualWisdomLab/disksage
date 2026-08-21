//! Credential-free process-boundary coverage for OAuth loopback preparation.
//!
//! This exercises the real ephemeral listener and provider-specific authorization URL generation
//! but never launches a browser, waits for a callback, accesses the credential store, or sends a
//! provider network request.

#![cfg(not(coverage))]

use disksage_lib::cloud::CloudProvider;
use disksage_lib::provider_oauth::prepare_authorization;

const GOOGLE_CLIENT_ID: &str = "1234567890-abcxyz.apps.googleusercontent.com";
const MICROSOFT_CLIENT_ID: &str = "12345678-1234-4abc-8def-1234567890ab";

#[test]
fn google_authorization_preparation_binds_ephemeral_loopback_and_read_only_scope() {
    let pending = prepare_authorization(CloudProvider::GoogleDrive, GOOGLE_CLIENT_ID).unwrap();
    let url = pending.authorization_url();

    assert!(url.starts_with("https://accounts.google.com/o/oauth2/v2/auth?"));
    assert!(url.contains("client_id=1234567890-abcxyz.apps.googleusercontent.com"));
    assert!(url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A"));
    assert!(url.contains("scope=https%3A%2F%2Fwww.googleapis.com%2Fauth%2Fdrive.metadata.readonly"));
    assert!(url.contains("code_challenge_method=S256"));
    assert!(url.contains("access_type=offline"));
    assert!(url.contains("prompt=consent"));
    assert!(url.contains("include_granted_scopes=true"));
    assert!(!url.contains("drive.file"));
}

#[test]
fn onedrive_authorization_preparation_uses_localhost_and_files_read_only_scope() {
    let pending = prepare_authorization(CloudProvider::Onedrive, MICROSOFT_CLIENT_ID).unwrap();
    let url = pending.authorization_url();

    assert!(url.starts_with("https://login.microsoftonline.com/common/oauth2/v2.0/authorize?"));
    assert!(url.contains("client_id=12345678-1234-4abc-8def-1234567890ab"));
    assert!(url.contains("redirect_uri=http%3A%2F%2Flocalhost%3A"));
    assert!(url.contains("scope=Files.Read%20offline_access"));
    assert!(url.contains("response_mode=query"));
    assert!(url.contains("prompt=select_account"));
    assert!(url.contains("code_challenge_method=S256"));
    assert!(!url.contains("ReadWrite"));
}

#[test]
fn loopback_preparation_rejects_unsupported_provider_and_bad_client_before_authorization() {
    assert_eq!(
        prepare_authorization(CloudProvider::Icloud, MICROSOFT_CLIENT_ID)
            .err()
            .expect("iCloud has no OAuth preparation path"),
        "icloud-oauth-not-supported"
    );
    assert_eq!(
        prepare_authorization(CloudProvider::GoogleDrive, "bad_client.apps.googleusercontent.com")
            .err()
            .expect("malformed Google client id must fail before listener authority is returned"),
        "oauth-client-id-provider-format-invalid"
    );
}
