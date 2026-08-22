#![cfg(feature = "cloud-cli")]

//! Black-box process regressions for the shipped provider OAuth CLI boundary.
//!
//! These tests launch the actual feature-gated binary. Help and malformed-host cases terminate
//! before browser, credential-store, provider-network, or cloud-root discovery work; the normal
//! success cases use the read-only `--list` action against local connection-document paths.

use std::process::Command;

const USAGE: &str = "usage: disksage-provider-oauth [--home ABSOLUTE_PATH] [--connections ABSOLUTE_PATH] (--list | --connect --cloud-root ABSOLUTE_PATH --client-id ID [--manual-browser] | --verify-capacity --cloud-root ABSOLUTE_PATH | --disconnect --cloud-root ABSOLUTE_PATH)";

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_disksage-provider-oauth"))
}

fn assert_bounded_failure(args: &[&str], expected: &str) {
    let output = command()
        .args(args)
        .output()
        .expect("provider OAuth CLI should start");

    assert_eq!(output.status.code(), Some(1), "args: {args:?}");
    assert!(output.stdout.is_empty(), "args: {args:?}");
    assert_eq!(output.stderr, format!("{expected}\n").as_bytes(), "args: {args:?}");
}

#[test]
fn sole_help_flags_exit_zero_on_stdout_without_stderr() {
    for flag in ["--help", "-h"] {
        let output = command().arg(flag).output().expect("provider OAuth CLI should start");

        assert_eq!(output.status.code(), Some(0), "flag: {flag}");
        assert_eq!(output.stdout, format!("{USAGE}\n").as_bytes(), "flag: {flag}");
        assert!(output.stderr.is_empty(), "flag: {flag}");
    }
}

#[test]
fn help_mixed_with_domain_arguments_remains_a_bounded_failure() {
    assert_bounded_failure(&["--help", "--list"], "help must be used alone");
}

#[test]
fn malformed_and_conflicting_requests_keep_static_nonzero_diagnostics() {
    assert_bounded_failure(&["--definitely-unknown"], "unknown argument");
    assert_bounded_failure(&["--home"], "--home requires a value");
    assert_bounded_failure(
        &["--list", "--disconnect"],
        "actions are mutually exclusive",
    );
    assert_bounded_failure(
        &["--home", "relative-home", "--list"],
        "--home must be absolute",
    );
}

#[test]
fn duplicate_option_values_fail_before_domain_work() {
    for (args, expected) in [
        (
            vec!["--home", "first", "--home", "second", "--list"],
            "--home may be supplied once",
        ),
        (
            vec!["--connections", "first", "--connections", "second", "--list"],
            "--connections may be supplied once",
        ),
        (
            vec!["--cloud-root", "first", "--cloud-root", "second", "--disconnect"],
            "--cloud-root may be supplied once",
        ),
        (
            vec!["--client-id", "first", "--client-id", "second", "--connect"],
            "--client-id may be supplied once",
        ),
    ] {
        assert_bounded_failure(&args, expected);
    }
}

#[test]
fn action_specific_argument_contracts_fail_closed_before_provider_work() {
    let temp = tempfile::tempdir().expect("temporary app-data root should be created");
    let home = temp.path().to_string_lossy().into_owned();
    let cloud_root = temp.path().join("cloud-root").to_string_lossy().into_owned();
    let connections = temp.path().join("connections.json").to_string_lossy().into_owned();

    assert_bounded_failure(&[], "exactly one action is required");
    assert_bounded_failure(
        &["--home", &home, "--connections", "relative.json", "--list"],
        "--connections must be absolute",
    );
    assert_bounded_failure(
        &["--home", &home, "--disconnect", "--cloud-root", "relative-root"],
        "--cloud-root must be absolute",
    );
    assert_bounded_failure(
        &["--home", &home, "--list", "--cloud-root", &cloud_root],
        "--list does not accept root, client, or browser arguments",
    );
    assert_bounded_failure(
        &["--home", &home, "--connections", &connections, "--list", "--manual-browser"],
        "--list does not accept root, client, or browser arguments",
    );
    assert_bounded_failure(
        &["--home", &home, "--connect", "--client-id", "public-client"],
        "--connect requires --cloud-root and --client-id",
    );
    assert_bounded_failure(
        &["--home", &home, "--connect", "--cloud-root", &cloud_root],
        "--connect requires --cloud-root and --client-id",
    );
    assert_bounded_failure(
        &[
            "--home",
            &home,
            "--verify-capacity",
            "--cloud-root",
            &cloud_root,
            "--client-id",
            "unexpected",
        ],
        "capacity verification and disconnect require only --cloud-root",
    );
    assert_bounded_failure(
        &[
            "--home",
            &home,
            "--disconnect",
            "--cloud-root",
            &cloud_root,
            "--manual-browser",
        ],
        "capacity verification and disconnect require only --cloud-root",
    );
}

#[test]
fn missing_environment_home_fails_closed_before_read_only_list() {
    let output = command()
        .env_remove("HOME")
        .env_remove("USERPROFILE")
        .arg("--list")
        .output()
        .expect("provider OAuth CLI should start");

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(output.stderr, b"home-directory-unavailable\n");
}

#[test]
fn read_only_list_keeps_machine_json_on_stdout() {
    let temp = tempfile::tempdir().expect("temporary app-data root should be created");
    let connections = temp.path().join("connections.json");
    let output = command()
        .arg("--home")
        .arg(temp.path())
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
    assert_eq!(value["secrets_included"], false);
    assert_eq!(value["cloud_write_executed"], false);
    assert_eq!(value["source_eviction_executed"], false);
}

#[test]
fn read_only_list_uses_platform_default_connection_path_when_not_overridden() {
    let temp = tempfile::tempdir().expect("temporary app-data root should be created");
    let output = command()
        .arg("--home")
        .arg(temp.path())
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
}

#[cfg(windows)]
#[test]
fn read_only_list_uses_userprofile_when_windows_home_is_absent() {
    let temp = tempfile::tempdir().expect("temporary Windows profile root should be created");
    let output = command()
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
}

#[cfg(unix)]
#[test]
fn non_utf8_host_argument_is_rejected_without_panic_or_reflection() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let sensitive = OsString::from_vec(vec![0xff, b'/', b'p', b'r', b'i', b'v', b'a', b't', b'e']);
    let output = command()
        .arg(sensitive)
        .output()
        .expect("provider OAuth CLI should start");

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(output.stderr, b"argument-encoding-invalid\n");
}
