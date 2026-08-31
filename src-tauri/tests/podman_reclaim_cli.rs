//! Black-box regression tests for the Podman reclaim planner command-line boundary.
//!
//! These tests execute the shipped binary so argument decoding, exit status, and
//! redaction behavior are verified together.

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
fn execute_dangling_requires_confirmation_before_calling_podman() {
    let output = command()
        .args(["--execute-dangling", "--rationale", "reviewed"])
        .output()
        .expect("the Podman reclaim planner binary should start");

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("stderr should remain UTF-8");
    assert!(stderr.contains("requires --confirmation-phrase"));
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
