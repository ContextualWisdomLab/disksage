#![cfg(feature = "cloud-cli")]

use std::process::Command;

fn assert_help_success(binary: &str, flag: &str, expected_usage: &str) {
    let output = Command::new(binary)
        .env_remove("HOME")
        .arg(flag)
        .output()
        .expect("DiskSage operational CLI must launch for its help contract");

    assert!(
        output.status.success(),
        "{flag} must be a successful terminal action, got status {:?} and stderr {:?}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "successful help must not be projected through stderr"
    );
    let stdout = String::from_utf8(output.stdout).expect("help output must be valid UTF-8");
    assert_eq!(
        stdout,
        format!("{expected_usage}\n"),
        "help output must equal the stable usage synopsis"
    );
}

fn assert_invalid_argument_is_bounded(binary: &str, arguments: &[&str]) {
    let output = Command::new(binary)
        .env_remove("HOME")
        .args(arguments)
        .output()
        .expect("DiskSage operational CLI must launch for invalid argument validation");

    assert!(
        !output.status.success(),
        "an invalid invocation must remain a non-zero failure"
    );
    assert!(
        output.stdout.is_empty(),
        "invalid invocation must not emit successful output on stdout"
    );
    let stderr = String::from_utf8(output.stderr).expect("CLI diagnostics must be valid UTF-8");
    assert!(!stderr.is_empty(), "invalid invocation must remain visible");
    assert!(
        !stderr.contains("not-shown"),
        "invalid diagnostics must not echo arbitrary argument payloads"
    );
}

#[cfg(unix)]
fn assert_non_utf8_argument_is_bounded(binary: &str) {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let opaque = OsString::from_vec(vec![b'-', b'-', b'o', b'p', b'a', b'q', b'u', b'e', 0xff]);
    let output = Command::new(binary)
        .env_remove("HOME")
        .arg(opaque)
        .output()
        .expect("DiskSage operational CLI must launch for non-UTF-8 argument validation");

    assert_eq!(
        output.status.code(),
        Some(2),
        "invalid non-UTF-8 input must use the ordinary bounded argument-error exit"
    );
    assert!(output.stdout.is_empty(), "invalid non-UTF-8 input must not emit successful output");
    let stderr = String::from_utf8(output.stderr).expect("CLI diagnostics must remain valid UTF-8");
    assert!(!stderr.is_empty(), "invalid non-UTF-8 input must remain visible");
    assert!(
        !stderr.contains("panicked") && !stderr.contains("thread 'main'"),
        "invalid host arguments must not escape through a Rust panic"
    );
}

#[test]
fn icloud_sync_health_help_is_successful_without_environment_dependency() {
    let binary = env!("CARGO_BIN_EXE_disksage-icloud-sync-health");
    let expected_usage = "usage: disksage-icloud-sync-health [--db-dir ABSOLUTE_CLOUDDOCS_DB_DIR] [--output ABSOLUTE_NEW_FILE.json]";
    assert_help_success(binary, "--help", expected_usage);
    assert_help_success(binary, "-h", expected_usage);
    assert_invalid_argument_is_bounded(binary, &["--opaque-option=not-shown"]);
    assert_invalid_argument_is_bounded(binary, &["--help", "--opaque-option=not-shown"]);
    #[cfg(unix)]
    assert_non_utf8_argument_is_bounded(binary);
}

#[test]
fn provider_oauth_help_is_successful_without_environment_dependency() {
    let binary = env!("CARGO_BIN_EXE_disksage-provider-oauth");
    let expected_usage = "usage: disksage-provider-oauth [--home ABSOLUTE_PATH] [--connections ABSOLUTE_PATH] (--list | --connect --cloud-root ABSOLUTE_PATH --client-id ID [--manual-browser] | --verify-capacity --cloud-root ABSOLUTE_PATH | --disconnect --cloud-root ABSOLUTE_PATH)";
    assert_help_success(binary, "--help", expected_usage);
    assert_help_success(binary, "-h", expected_usage);
    assert_invalid_argument_is_bounded(binary, &["--opaque-option=not-shown"]);
    assert_invalid_argument_is_bounded(binary, &["--help", "--opaque-option=not-shown"]);
    #[cfg(unix)]
    assert_non_utf8_argument_is_bounded(binary);
}
