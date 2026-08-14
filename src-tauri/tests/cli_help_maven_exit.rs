//! Black-box process contracts for DiskSage Maven operational CLIs.

use std::process::Command;

/// Require one help flag to terminate successfully with the exact stable usage text.
fn assert_help_success(binary: &str, flag: &str, expected_usage: &str) {
    let output = Command::new(binary)
        .env_remove("HOME")
        .arg(flag)
        .output()
        .expect("DiskSage Maven CLI must launch for its help contract");

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

/// Require an unknown option to fail visibly without reflecting its opaque payload.
fn assert_invalid_argument_is_bounded(binary: &str) {
    let output = Command::new(binary)
        .env_remove("HOME")
        .arg("--opaque-option=not-shown")
        .output()
        .expect("DiskSage Maven CLI must launch for invalid argument validation");

    assert!(
        !output.status.success(),
        "an unknown argument must remain a non-zero failure"
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

/// Require a mixed help and invalid request to remain a bounded failure.
fn assert_help_does_not_hide_invalid_argument(binary: &str) {
    let output = Command::new(binary)
        .env_remove("HOME")
        .args(["--help", "--opaque-option=not-shown"])
        .output()
        .expect("DiskSage Maven CLI must launch for mixed help validation");

    assert!(
        !output.status.success(),
        "help must not turn an otherwise invalid invocation into success"
    );
    assert!(
        output.stdout.is_empty(),
        "mixed invalid invocation must not emit successful help on stdout"
    );
    let stderr = String::from_utf8(output.stderr).expect("CLI diagnostics must be valid UTF-8");
    assert!(!stderr.is_empty(), "mixed invalid invocation must remain visible");
    assert!(
        !stderr.contains("not-shown"),
        "mixed invalid diagnostics must not echo arbitrary argument payloads"
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
        .expect("DiskSage Maven CLI must launch for non-UTF-8 argument validation");

    assert_eq!(
        output.status.code(),
        Some(2),
        "invalid non-UTF-8 input must use the ordinary bounded argument-error exit"
    );
    assert!(
        output.stdout.is_empty(),
        "invalid non-UTF-8 input must not emit successful output"
    );
    let stderr = String::from_utf8(output.stderr).expect("CLI diagnostics must remain valid UTF-8");
    assert!(!stderr.is_empty(), "invalid non-UTF-8 input must remain visible");
    assert!(
        !stderr.contains("panicked") && !stderr.contains("thread 'main'"),
        "invalid host arguments must not escape through a Rust panic"
    );
}

/// Prove the Maven audit command's exact help and bounded invalid-input contract.
#[test]
fn maven_cache_audit_help_is_successful_and_invalid_arguments_are_bounded() {
    let binary = env!("CARGO_BIN_EXE_disksage-maven-cache-audit");
    let expected_usage = "usage: disksage-maven-cache-audit --repository-root ABSOLUTE_PATH [--output NEW_ABSOLUTE_JSON_PATH] [--max-entries N] [--max-candidates N] [--max-issues N]";
    assert_help_success(binary, "--help", expected_usage);
    assert_help_success(binary, "-h", expected_usage);
    assert_invalid_argument_is_bounded(binary);
    assert_help_does_not_hide_invalid_argument(binary);
    #[cfg(unix)]
    assert_non_utf8_argument_is_bounded(binary);
}

/// Prove the Maven prune command's exact help and bounded invalid-input contract.
#[test]
fn maven_cache_prune_help_is_successful_and_invalid_arguments_are_bounded() {
    let binary = env!("CARGO_BIN_EXE_disksage-maven-cache-prune");
    let expected_usage = "usage: disksage-maven-cache-prune --repository-root ABSOLUTE_PATH --expected-candidate-set-fingerprint HEX [--apply] [--max-entries N] [--output NEW_ABSOLUTE_JSON_PATH]";
    assert_help_success(binary, "--help", expected_usage);
    assert_help_success(binary, "-h", expected_usage);
    assert_invalid_argument_is_bounded(binary);
    assert_help_does_not_hide_invalid_argument(binary);
    #[cfg(unix)]
    assert_non_utf8_argument_is_bounded(binary);
}
