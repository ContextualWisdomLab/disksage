#![cfg(feature = "cloud-cli")]

//! Black-box host-boundary regressions for the shipped provider OAuth CLI.
//!
//! These cases terminate before browser launch, provider network I/O, credential-store access, or
//! cloud mutation. The Windows case performs only the read-only `--list` action in an isolated
//! temporary profile.

use std::process::Command;

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_disksage-provider-oauth"))
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
