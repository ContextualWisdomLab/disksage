#![cfg(feature = "cloud-cli")]

//! Black-box regressions for the shipped provider OAuth CLI host/process boundary.
//!
//! These tests launch the real feature-gated binary. They stay on the read-only `--list` path, so
//! they perform no browser, network, credential-store, provider-write, or source-eviction work.

use disksage_lib::cloud::CloudProvider;
use disksage_lib::provider_oauth::{requested_scope, OAuthConnection};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::process::{Command, Output};

const APP_IDENTIFIER: &str = "com.contextualwisdomlab.disksage";
const GOOGLE_CLIENT_ID: &str = "1234567890-abcxyz.apps.googleusercontent.com";

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_disksage-provider-oauth"))
}

fn connection_id(provider: CloudProvider, root_id: &str, root_path: &str) -> String {
    let mut hasher = Sha256::new();
    for value in [provider.as_str(), root_id, root_path] {
        hasher.update(value.as_bytes());
        hasher.update([0]);
    }
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02h}"))
        .collect()
}

fn google_connection(root_path: &Path) -> OAuthConnection {
    let root_path = root_path.to_string_lossy().into_owned();
    OAuthConnection {
        connection_id: connection_id(CloudProvider::GoogleDrive, "google-account", &root_path),
        provider: CloudProvider::GoogleDrive,
        cloud_root_id: "google-account".into(),
        cloud_root_path: root_path,
        client_id: GOOGLE_CLIENT_ID.into(),
        scope: requested_scope(CloudProvider::GoogleDrive)
            .expect("Google Drive exposes its read-only scope")
            .into(),
        connected_at_ms: 123,
    }
}

fn write_private_document(path: &Path, connections: &[OAuthConnection]) {
    let document = serde_json::json!({"version": 1, "connections": connections});
    std::fs::write(
        path,
        serde_json::to_vec(&document).expect("connection document serializes"),
    )
    .expect("connection document writes");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .expect("connection document remains private");
    }
}

fn run_list(home: &Path, connections: &Path) -> Output {
    command()
        .arg("--home")
        .arg(home)
        .arg("--connections")
        .arg(connections)
        .arg("--list")
        .output()
        .expect("provider OAuth CLI starts")
}

#[cfg(windows)]
#[test]
fn read_only_list_uses_userprofile_when_home_is_absent() {
    let temp = tempfile::tempdir().expect("temporary Windows profile root should be created");
    let output = command()
        .env_remove("HOME")
        .env_remove("APPDATA")
        .env("USERPROFILE", temp.path())
        .arg("--list")
        .output()
        .expect("provider OAuth CLI should start");

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("list stdout should remain JSON");
    assert_eq!(value["action"], "list");
    assert_eq!(value["connection_count"], 0);
    assert_eq!(value["connection_document_effect"], "none");
    assert_eq!(value["credential_store_effect"], "none");
    assert_eq!(value["secrets_included"], false);
    assert_eq!(value["cloud_write_executed"], false);
    assert_eq!(value["source_eviction_executed"], false);
}

#[cfg(windows)]
#[test]
fn read_only_list_uses_redirected_roaming_appdata_for_default_connection_document() {
    let temp = tempfile::tempdir().expect("temporary Windows authority root should be created");
    let user_profile = temp.path().join("profile");
    let appdata = temp.path().join("redirected-roaming-appdata");
    let app_directory = appdata.join(APP_IDENTIFIER);
    std::fs::create_dir_all(&user_profile).expect("USERPROFILE fixture should be created");
    std::fs::create_dir_all(&app_directory).expect("redirected APPDATA fixture should be created");

    let connection = google_connection(&temp.path().join("cloud-root"));
    let document = app_directory.join("cloud-oauth-connections.json");
    write_private_document(&document, std::slice::from_ref(&connection));

    let output = command()
        .env_remove("HOME")
        .env("USERPROFILE", &user_profile)
        .env("APPDATA", &appdata)
        .arg("--list")
        .output()
        .expect("provider OAuth CLI should start");

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("list stdout should remain JSON");
    assert_eq!(value["action"], "list");
    assert_eq!(value["connection_count"], 1);
    assert_eq!(value["connections"][0]["connection_id"], connection.connection_id);
    assert_eq!(value["connection_document_effect"], "none");
    assert_eq!(value["credential_store_effect"], "none");
    assert_eq!(value["cloud_write_executed"], false);
    assert_eq!(value["source_eviction_executed"], false);
    assert!(
        !user_profile
            .join("AppData/Roaming")
            .join(APP_IDENTIFIER)
            .join("cloud-oauth-connections.json")
            .exists(),
        "read-only list must not create a stale local-profile connection document"
    );
}

#[cfg(windows)]
#[test]
fn read_only_list_can_use_redirected_roaming_appdata_without_userprofile() {
    let temp = tempfile::tempdir().expect("temporary Windows authority root should be created");
    let appdata = temp.path().join("redirected-roaming-appdata");
    let app_directory = appdata.join(APP_IDENTIFIER);
    std::fs::create_dir_all(&app_directory).expect("redirected APPDATA fixture should be created");

    let connection = google_connection(&temp.path().join("cloud-root"));
    let document = app_directory.join("cloud-oauth-connections.json");
    write_private_document(&document, std::slice::from_ref(&connection));

    let output = command()
        .env_remove("HOME")
        .env_remove("USERPROFILE")
        .env("APPDATA", &appdata)
        .arg("--list")
        .output()
        .expect("provider OAuth CLI should start");

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("list stdout should remain JSON");
    assert_eq!(value["action"], "list");
    assert_eq!(value["connection_count"], 1);
    assert_eq!(value["connections"][0]["connection_id"], connection.connection_id);
    assert_eq!(value["connection_document_effect"], "none");
    assert_eq!(value["credential_store_effect"], "none");
    assert_eq!(value["cloud_write_executed"], false);
    assert_eq!(value["source_eviction_executed"], false);
}

#[cfg(unix)]
#[test]
fn read_only_list_preserves_native_non_utf8_path_operands() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let temp = tempfile::tempdir().expect("temporary native-path root should be created");
    let home = temp
        .path()
        .join(OsString::from_vec(vec![b'h', b'o', b'm', b'e', b'-', 0xff]));
    std::fs::create_dir(&home).expect("native non-UTF-8 home should be created");
    let connections = home.join(OsString::from_vec(vec![
        b'c', b'o', b'n', b'n', b'e', b'c', b't', b'i', b'o', b'n', b's', b'-', 0xfe, b'.', b'j',
        b's', b'o', b'n',
    ]));

    let output = run_list(&home, &connections);

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("list stdout should remain JSON");
    assert_eq!(value["action"], "list");
    assert_eq!(value["connection_count"], 0);
    assert_eq!(value["connection_document_effect"], "none");
    assert_eq!(value["credential_store_effect"], "none");
    assert_eq!(value["secrets_included"], false);
    assert_eq!(value["cloud_write_executed"], false);
    assert_eq!(value["source_eviction_executed"], false);
    assert!(
        !connections.exists(),
        "read-only list must not create the connection document"
    );
}

#[test]
fn read_only_list_serializes_valid_nonempty_document_without_secret_or_mutation_claims() {
    let temp = tempfile::tempdir().expect("temporary app-data root should be created");
    let connection = google_connection(&temp.path().join("cloud-root"));
    let document = temp.path().join("connections.json");
    write_private_document(&document, std::slice::from_ref(&connection));

    let output = run_list(temp.path(), &document);

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("list stdout should remain JSON");
    assert_eq!(value["action"], "list");
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["connection_count"], 1);
    assert_eq!(value["connections"][0]["connection_id"], connection.connection_id);
    assert_eq!(value["connections"][0]["cloud_root_id"], "google-account");
    assert_eq!(value["secrets_included"], false);
    assert_eq!(value["connection_document_effect"], "none");
    assert_eq!(value["credential_store_effect"], "none");
    assert_eq!(value["cloud_write_executed"], false);
    assert_eq!(value["source_eviction_executed"], false);
}

#[test]
fn read_only_list_rejects_duplicate_identity_without_partial_stdout() {
    let temp = tempfile::tempdir().expect("temporary app-data root should be created");
    let connection = google_connection(&temp.path().join("cloud-root"));
    let document = temp.path().join("connections.json");
    write_private_document(&document, &[connection.clone(), connection]);

    let output = run_list(temp.path(), &document);

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(output.stderr, b"oauth-connection-document-duplicate-id\n");
}