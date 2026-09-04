//! Credential-free coverage for public provider OAuth failure boundaries.
//!
//! These regressions exercise shipped public API paths that must fail before browser callbacks,
//! provider network I/O, keyring access, or durable mutation when their prerequisite authority is
//! missing or mismatched.

#![cfg(not(coverage))]

use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::provider_oauth::{
    disconnect, finish_authorization, prepare_authorization, refreshed_access_token,
};

const GOOGLE_CLIENT_ID: &str = "1234567890-abcxyz.apps.googleusercontent.com";

fn root(provider: CloudProvider) -> CloudRoot {
    #[cfg(windows)]
    let path = r"C:\Cloud\Coverage";
    #[cfg(not(windows))]
    let path = "/Cloud/Coverage";

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

#[test]
fn finish_authorization_rejects_provider_root_mismatch_before_callback_or_network_work() {
    let pending = prepare_authorization(CloudProvider::GoogleDrive, GOOGLE_CLIENT_ID)
        .expect("preparation should bind only local loopback listeners");
    let temp = tempfile::tempdir().unwrap();
    let document = temp.path().join("connections.json");

    assert_eq!(
        finish_authorization(pending, &root(CloudProvider::Onedrive), &document, 1).unwrap_err(),
        "provider-oauth-root-mismatch"
    );
    assert!(
        !document.exists(),
        "provider mismatch must fail before durable connection publication"
    );
}

#[test]
fn missing_connection_blocks_refresh_and_disconnect_before_keyring_or_provider_work() {
    let temp = tempfile::tempdir().unwrap();
    let document = temp.path().join("connections.json");
    let google_root = root(CloudProvider::GoogleDrive);

    assert_eq!(
        refreshed_access_token(&document, &google_root).unwrap_err(),
        "provider-oauth-connection-missing"
    );
    assert_eq!(
        disconnect(&document, &google_root).unwrap_err(),
        "provider-oauth-connection-missing"
    );
    assert!(
        !document.exists(),
        "read-only missing-connection failures must not create durable OAuth state"
    );
}
