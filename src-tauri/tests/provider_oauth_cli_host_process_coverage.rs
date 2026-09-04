#![cfg(feature = "cloud-cli")]

//! Black-box regressions for the shipped provider OAuth CLI.
//!
//! These cases stop before browser launch, provider network I/O, credential-store mutation, cloud
//! write, or source eviction. Connection-document cases use only isolated local filesystem state.

use disksage_lib::cloud::CloudProvider;
use disksage_lib::provider_oauth::{requested_scope, OAuthConnection};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::process::{Command, Output};

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

#[test]
fn sole_help_is_a_successful_stdout_contract() {
    let output = command()
        .arg("--help")
        .output()
        .expect("provider OAuth CLI starts");

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).expect("help is UTF-8");
    assert!(stdout.starts_with("usage: disksage-provider-oauth "));
    assert!(stdout.contains("[--write-access]"));
}

#[test]
fn help_mixed_with_domain_arguments_is_a_bounded_failure() {
    let output = command()
        .args(["--help", "--list"])
        .output()
        .expect("provider OAuth CLI starts");

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(output.stderr, b"help must be used alone\n");
}

#[test]
fn read_only_list_serializes_a_valid_connection_without_secret_or_mutation_claims() {
    let temp = tempfile::tempdir().expect("isolated app-data root exists");
    let connection = google_connection(&temp.path().join("cloud-root"));
    let document = temp.path().join("connections.json");
    write_private_document(&document, std::slice::from_ref(&connection));

    let output = run_list(temp.path(), &document);

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("list stdout remains machine JSON");
    assert_eq!(value["action"], "list");
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["connection_count"], 1);
    assert_eq!(value["connections"][0]["connection_id"], connection.connection_id);
    assert_eq!(value["secrets_included"], false);
    assert_eq!(value["connection_document_effect"], "none");
    assert_eq!(value["credential_store_effect"], "none");
    assert_eq!(value["cloud_write_executed"], false);
    assert_eq!(value["source_eviction_executed"], false);
}

#[test]
fn read_only_list_rejects_duplicate_identity_without_partial_stdout() {
    let temp = tempfile::tempdir().expect("isolated app-data root exists");
    let connection = google_connection(&temp.path().join("cloud-root"));
    let document = temp.path().join("connections.json");
    write_private_document(&document, &[connection.clone(), connection]);

    let output = run_list(temp.path(), &document);

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(output.stderr, b"oauth-connection-document-duplicate-id\n");
}

#[cfg(windows)]
#[test]
fn read_only_list_falls_back_to_userprofile_when_home_is_absent() {
    let temp = tempfile::tempdir().expect("isolated Windows profile exists");
    let output = command()
        .env_remove("HOME")
        .env("USERPROFILE", temp.path())
        .arg("--list")
        .output()
        .expect("provider OAuth CLI starts");

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("list stdout remains machine JSON");
    assert_eq!(value["action"], "list");
    assert_eq!(value["connection_count"], 0);
    assert_eq!(value["connection_document_effect"], "none");
    assert_eq!(value["credential_store_effect"], "none");
    assert_eq!(value["cloud_write_executed"], false);
    assert_eq!(value["source_eviction_executed"], false);
}

#[cfg(unix)]
#[test]
fn native_non_utf8_filesystem_arguments_remain_lossless() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let temp = tempfile::tempdir().expect("isolated native-path root exists");
    let home = temp
        .path()
        .join(OsString::from_vec(vec![b'h', b'o', b'm', b'e', b'-', 0xff]));
    std::fs::create_dir(&home).expect("native non-UTF-8 home exists");
    let connections = home.join(OsString::from_vec(vec![
        b'c', b'o', b'n', b'n', b'e', b'c', b't', b'i', b'o', b'n', b's', b'-', 0xfe, b'.', b'j',
        b's', b'o', b'n',
    ]));

    let output = command()
        .arg("--home")
        .arg(&home)
        .arg("--connections")
        .arg(&connections)
        .arg("--list")
        .output()
        .expect("provider OAuth CLI starts");

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("list stdout remains machine JSON");
    assert_eq!(value["action"], "list");
    assert_eq!(value["connection_count"], 0);
    assert_eq!(value["connection_document_effect"], "none");
    assert!(!connections.exists(), "read-only list must not create the document");
}

#[cfg(unix)]
#[test]
fn non_utf8_host_argument_fails_without_panic_or_reflection() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let sensitive = OsString::from_vec(vec![0xff, b'/', b'p', b'r', b'i', b'v', b'a', b't', b'e']);
    let output = command()
        .arg(sensitive)
        .output()
        .expect("provider OAuth CLI starts");

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(output.stderr, b"argument-encoding-invalid\n");
}
