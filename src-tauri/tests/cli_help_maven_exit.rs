//! Black-box process contracts for DiskSage Maven operational CLIs.

use std::process::Command;

fn assert_help_success(binary: &str, flag: &str, usage_marker: &str) {
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
    assert!(
        stdout.contains(usage_marker),
        "help output must contain the stable usage synopsis"
    );
}

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

#[test]
fn maven_cache_audit_help_is_successful_and_invalid_arguments_are_bounded() {
    let binary = env!("CARGO_BIN_EXE_disksage-maven-cache-audit");
    assert_help_success(binary, "--help", "usage: disksage-maven-cache-audit");
    assert_help_success(binary, "-h", "usage: disksage-maven-cache-audit");
    assert_invalid_argument_is_bounded(binary);
    assert_help_does_not_hide_invalid_argument(binary);
}

#[test]
fn maven_cache_prune_help_is_successful_and_invalid_arguments_are_bounded() {
    let binary = env!("CARGO_BIN_EXE_disksage-maven-cache-prune");
    assert_help_success(binary, "--help", "usage: disksage-maven-cache-prune");
    assert_help_success(binary, "-h", "usage: disksage-maven-cache-prune");
    assert_invalid_argument_is_bounded(binary);
    assert_help_does_not_hide_invalid_argument(binary);
}
