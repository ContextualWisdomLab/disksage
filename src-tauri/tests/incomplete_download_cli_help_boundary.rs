//! Black-box host-argument regressions for incomplete-download operator CLIs.
//!
//! Help is a terminal, side-effect-free operator boundary: it must not require HOME,
//! cloud discovery, provider capacity, private evidence, or execution authority. Host
//! arguments that cannot be decoded as UTF-8 must fail closed without a Rust panic.

use std::process::Command;

fn assert_terminal_help(binary: &str, expected_usage: &str) {
    for help_flag in ["--help", "-h"] {
        let output = Command::new(binary)
            .arg(help_flag)
            .env_remove("HOME")
            .env_remove("USERPROFILE")
            .output()
            .expect("the shipped incomplete-download binary should start");

        assert_eq!(
            output.status.code(),
            Some(0),
            "help must be terminal success for {help_flag}: stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            output.stderr.is_empty(),
            "help must not be reported as an execution failure for {help_flag}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).expect("help output should be UTF-8");
        assert!(
            stdout.contains(expected_usage),
            "help output should contain the shipped usage contract: {stdout}"
        );
    }
}

#[cfg(unix)]
fn assert_non_utf8_argument_fails_closed(binary: &str) {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let output = Command::new(binary)
        .arg(OsString::from_vec(vec![0xff, b'x']))
        .output()
        .expect("the shipped incomplete-download binary should start");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("stderr should remain UTF-8");
    assert!(stderr.contains("invalid-argument-encoding"), "stderr: {stderr}");
    assert!(!stderr.contains("panicked"), "stderr: {stderr}");
}

#[test]
fn destination_plan_help_is_terminal_without_home_or_provider_io() {
    assert_terminal_help(
        env!("CARGO_BIN_EXE_disksage-incomplete-download-destination-plan"),
        "usage: disksage-incomplete-download-destination-plan",
    );
}

#[test]
fn materialize_help_is_terminal_without_home_or_mutation_authority() {
    assert_terminal_help(
        env!("CARGO_BIN_EXE_disksage-incomplete-download-materialize"),
        "usage: disksage-incomplete-download-materialize",
    );
}

#[cfg(unix)]
#[test]
fn destination_plan_rejects_non_utf8_host_arguments_without_panicking() {
    assert_non_utf8_argument_fails_closed(env!(
        "CARGO_BIN_EXE_disksage-incomplete-download-destination-plan"
    ));
}

#[cfg(unix)]
#[test]
fn materialize_rejects_non_utf8_host_arguments_without_panicking() {
    assert_non_utf8_argument_fails_closed(env!(
        "CARGO_BIN_EXE_disksage-incomplete-download-materialize"
    ));
}
