#![cfg(feature = "cloud-cli")]

//! Black-box Windows regression for the shipped provider OAuth CLI home-authority boundary.
//!
//! The platform-neutral `environment_home_from` contract is not enough to prove that the packaged
//! process actually observes Windows `USERPROFILE` when `HOME` is absent. This test launches the
//! real feature-gated binary and stays on the read-only `--list` path, so it performs no browser,
//! network, credential-store, provider-write, or source-eviction work.

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
