#![cfg(all(feature = "cloud-cli", unix, not(target_os = "macos")))]

//! Black-box Linux/XDG regression for the shipped provider OAuth CLI data-home boundary.
//!
//! The test launches only the read-only `--list` action. It performs no browser, network,
//! credential-store, provider-write, source-eviction, or filesystem-mutation operation.

use disksage_lib::cloud::CloudProvider;
use disksage_lib::provider_oauth::{requested_scope, OAuthConnection};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::process::Command;

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
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn google_connection(root_path: &Path) -> OAuthConnection {
    let root_path = root_path.to_string_lossy().into_owned();
    OAuthConnection {
        connection_id: connection_id(CloudProvider::GoogleDrive, "xdg-google-account", &root_path),
        provider: CloudProvider::GoogleDrive,
        cloud_root_id: "xdg-google-account".into(),
        cloud_root_path: root_path,
        client_id: GOOGLE_CLIENT_ID.into(),
        scope: requested_scope(CloudProvider::GoogleDrive)
            .expect("Google Drive exposes its read-only scope")
            .into(),
        connected_at_ms: 123,
    }
}

fn write_private_document(path: &Path, connection: &OAuthConnection) {
    std::fs::create_dir_all(path.parent().expect("connection document has a parent"))
        .expect("XDG app-data parent should be created");
    let document = serde_json::json!({"version": 1, "connections": [connection]});
    std::fs::write(
        path,
        serde_json::to_vec(&connection_document).expect("connection document serializes"),
    )
    .expect("connection document writes");

    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .expect("connection document remains private");
}

#[test]
fn read_only_list_uses_absolute_xdg_data_home_for_default_connection_document() {
    let temp = tempfile::tempdir().expect("temporary Linux data-home root should be created");
    let home = temp.path().join("home");
    let xdg_data_home = temp.path().join("xdg-data");
    std::fs::create_dir(&home).expect("HOME fixture should be created");

    let connection = google_connection(&temp.path().join("cloud-root"));
    let document = xdg_data_home
        .join(APP_IDENTIFIER)
        .join("cloud-oauth-connections.json");
    write_private_document(&document, &connection);

    let output = command()
        .env("HOME", &home)
        .env("XDG_DATA_HOME", &xdg_data_home)
        .env_remove("USERPROFILE")
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

#[test]
fn read_only_list_can_use_absolute_xdg_data_home_without_home() {
    let temp = tempfile::tempdir().expect("temporary Linux data-home root should be created");
    let xdg_data_home = temp.path().join("xdg-data");
    let connection = google_connection(&temp.path().join("cloud-root"));
    let document = xdg_data_home
        .join(APP_IDENTIFIER)
        .join("cloud-oauth-connections.json");
    write_private_document(&document, &connection);

    let output = command()
        .env_remove("HOME")
        .env_remove("USERPROFILE")
        .env("XDG_DATA_HOME", &xdg_data_home)
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

#[test]
fn explicit_home_keeps_its_connection_default_when_xdg_data_home_is_set() {
    let temp = tempfile::tempdir().expect("temporary Linux authority root should be created");
    let environment_home = temp.path().join("environment-home");
    let explicit_home = temp.path().join("explicit-home");
    let xdg_data_home = temp.path().join("xdg-data");
    std::fs::create_dir(&environment_home).expect("environment HOME fixture should be created");

    let connection = google_connection(&temp.path().join("cloud-root"));
    let document = explicit_home
        .join(".local/share")
        .join(APP_IDENTIFIER)
        .join("cloud-oauth-connections.json");
    write_private_document(&document, &connection);

    let output = command()
        .env("HOME", &environment_home)
        .env("XDG_DATA_HOME", &xdg_data_home)
        .env_remove("USERPROFILE")
        .arg("--home")
        .arg(&explicit_home)
        .arg("--list")
        .output()
        .expect("provider OAuth CLI should start");

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("list stdout should remain JSON");
    assert_eq!(value["connection_count"], 1);
    assert_eq!(value["connections"][0]["connection_id"], connection.connection_id);
}

#[test]
fn relative_xdg_data_home_is_ignored_in_favor_of_home_default() {
    let temp = tempfile::tempdir().expect("temporary Linux home should be created");
    let home = temp.path().join("home");
    let connection = google_connection(&temp.path().join("cloud-root"));
    let document = home
        .join(".local/share")
        .join(APP_IDENTIFIER)
        .join("cloud-oauth-connections.json");
    write_private_document(&document, &connection);

    let output = command()
        .env("HOME", &home)
        .env("XDG_DATA_HOME", "relative-xdg-data")
        .env_remove("USERPROFILE")
        .arg("--list")
        .output()
        .expect("provider OAuth CLI should start");

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("list stdout should remain JSON");
    assert_eq!(value["connection_count"], 1);
    assert_eq!(value["connections"][0]["connection_id"], connection.connection_id);
}