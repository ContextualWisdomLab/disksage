//! Black-box regression tests for the Podman reclaim planner command-line boundary.
//!
//! These tests execute the shipped binary so argument decoding, exit status, JSON
//! projection, and redaction behavior are verified together without requiring Podman.

use std::process::Command;

/// Returns the compiled Podman reclaim planner binary used by Cargo integration tests.
fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_disksage-podman-reclaim-plan"))
}

#[test]
fn unknown_argument_does_not_echo_a_sensitive_value() {
    let sensitive_value = "/Users/private/customer-data";
    let output = command()
        .arg(sensitive_value)
        .output()
        .expect("the Podman reclaim planner binary should start");

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("stderr should remain UTF-8");
    assert!(stderr.contains("unknown option"));
    assert!(!stderr.contains(sensitive_value));
}

#[test]
fn sole_help_flags_emit_usage_on_stdout() {
    for help_flag in ["--help", "-h"] {
        let output = command()
            .arg(help_flag)
            .output()
            .expect("the Podman reclaim planner binary should start");

        assert_eq!(output.status.code(), Some(0), "help flag: {help_flag}");
        let stdout = String::from_utf8(output.stdout).expect("stdout should remain UTF-8");
        let stderr = String::from_utf8(output.stderr).expect("stderr should remain UTF-8");
        assert!(
            stdout.contains("Usage: disksage-podman-reclaim-plan"),
            "help flag: {help_flag}, stdout: {stdout}"
        );
        assert!(stderr.is_empty(), "help flag: {help_flag}, stderr: {stderr}");
    }
}

#[test]
fn required_values_and_timeout_bounds_fail_closed() {
    let cases: &[(&[&str], &str)] = &[
        (&["--machine"], "--machine requires a name"),
        (&["--podman-bin"], "--podman-bin requires a path"),
        (
            &["--timeout-seconds"],
            "--timeout-seconds requires an integer",
        ),
        (
            &["--timeout-seconds", "not-an-integer"],
            "--timeout-seconds requires an integer",
        ),
        (
            &["--timeout-seconds", "0"],
            "--timeout-seconds must be between 1 and 60",
        ),
        (
            &["--timeout-seconds", "61"],
            "--timeout-seconds must be between 1 and 60",
        ),
    ];

    for (arguments, expected_error) in cases {
        let output = command()
            .args(*arguments)
            .output()
            .expect("the Podman reclaim planner binary should start");

        assert_eq!(output.status.code(), Some(2), "arguments: {arguments:?}");
        assert!(output.stdout.is_empty(), "arguments: {arguments:?}");
        let stderr = String::from_utf8(output.stderr).expect("stderr should remain UTF-8");
        assert!(
            stderr.contains(expected_error),
            "arguments: {arguments:?}, stderr: {stderr}"
        );
        assert!(!stderr.contains("panicked"), "arguments: {arguments:?}");
    }
}

#[test]
fn explicit_probe_options_emit_pretty_fail_closed_json_when_podman_is_missing() {
    let temp = tempfile::tempdir().expect("temporary directory should be created");
    let missing_podman = temp.path().join("missing-podman");
    let output = command()
        .arg("--machine")
        .arg("coverage-machine")
        .arg("--podman-bin")
        .arg(&missing_podman)
        .arg("--timeout-seconds")
        .arg("1")
        .arg("--pretty")
        .output()
        .expect("the Podman reclaim planner binary should start");

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).expect("stdout should remain UTF-8");
    assert!(stdout.contains("\n  \""), "pretty JSON was not emitted: {stdout}");
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("stdout should be JSON");
    assert_eq!(value["schema_kind"], "disksage.podman-reclaim-plan");
    assert_eq!(value["evidence_complete"], false);
    let issues = value["issues"]
        .as_array()
        .expect("issues should be a JSON array");
    assert!(issues.iter().any(|issue| {
        issue
            .as_str()
            .is_some_and(|text| text.starts_with("podman-machine-inspect-spawn:"))
    }));
}

#[test]
fn default_probe_options_emit_compact_fail_closed_json_when_podman_is_missing() {
    let temp = tempfile::tempdir().expect("temporary directory should be created");
    let missing_podman = temp.path().join("missing-podman");
    let output = command()
        .arg("--podman-bin")
        .arg(&missing_podman)
        .output()
        .expect("the Podman reclaim planner binary should start");

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).expect("stdout should remain UTF-8");
    assert!(
        !stdout.contains("\n  \""),
        "compact JSON unexpectedly used pretty indentation: {stdout}"
    );
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("stdout should be JSON");
    assert_eq!(value["schema_kind"], "disksage.podman-reclaim-plan");
    assert_eq!(value["evidence_complete"], false);
}

#[cfg(unix)]
#[test]
fn non_utf8_argument_is_rejected_without_panicking() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let output = command()
        .arg(OsString::from_vec(vec![0xff, b'x']))
        .output()
        .expect("the Podman reclaim planner binary should start");

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("stderr should remain UTF-8");
    assert!(stderr.contains("unknown option"));
    assert!(!stderr.contains("panicked"));
}

#[cfg(unix)]
#[test]
fn non_utf8_utf8_only_option_values_are_rejected_without_panicking() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    for (option, expected_error) in [
        ("--machine", "--machine requires a UTF-8 name"),
        (
            "--timeout-seconds",
            "--timeout-seconds requires a UTF-8 integer",
        ),
    ] {
        let output = command()
            .arg(option)
            .arg(OsString::from_vec(vec![0xff, b'x']))
            .output()
            .expect("the Podman reclaim planner binary should start");

        assert_eq!(output.status.code(), Some(2), "option: {option}");
        assert!(output.stdout.is_empty(), "option: {option}");
        let stderr = String::from_utf8(output.stderr).expect("stderr should remain UTF-8");
        assert!(
            stderr.contains(expected_error),
            "option: {option}, stderr: {stderr}"
        );
        assert!(!stderr.contains("panicked"), "option: {option}");
    }
}
