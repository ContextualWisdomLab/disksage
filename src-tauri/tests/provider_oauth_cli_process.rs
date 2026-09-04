#![cfg(feature = "cloud-cli")]

//! Black-box regressions for the shipped provider OAuth CLI host-authority boundary.
//!
//! These tests launch the real feature-gated binary and stay on the read-only `--list` path, so
//! they perform no browser, network, credential-store, provider-write, or source-eviction work.

#[cfg(windows)]
#[test]
fn read_only_list_uses_userprofile_when_home_is_absent() {
    use std::process::Command;

    let temp = tempfile::tempdir().expect("temporary Windows profile root should be created");
    let output = Command::new(env!("CARGO_BIN_EXE_disksage-provider-oauth"))
        .env_remove("HOME")
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

#[cfg(unix)]
#[test]
fn read_only_list_preserves_native_non_utf8_path_operands() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    use std::process::Command;

    let temp = tempfile::tempdir().expect("temporary native-path root should be created");
    let home = temp
        .path()
        .join(OsString::from_vec(vec![b'h', b'o', b'm', b'e', b'-', 0xff]));
    std::fs::create_dir(&home).expect("native non-UTF-8 home should be created");
    let connections = home.join(OsString::from_vec(vec![
        b'c', b'o', b'n', b'n', b'e', b'c', b't', b'i', b'o', b'n', b's', b'-', 0xfe, b'.', b'j',
        b's', b'o', b'n',
    ]));

    let output = Command::new(env!("CARGO_BIN_EXE_disksage-provider-oauth"))
        .arg("--home")
        .arg(&home)
        .arg("--connections")
        .arg(&connections)
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
    assert!(
        !connections.exists(),
        "read-only list must not create the connection document"
    );
}
