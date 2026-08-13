use std::process::Command;

fn assert_help_success(binary: &str, flag: &str, usage_marker: &str) {
    let output = Command::new(binary)
        .arg(flag)
        .output()
        .expect("DiskSage Maven cache CLI must launch for its help contract");

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

fn assert_help_does_not_hide_invalid_argument(binary: &str) {
    let output = Command::new(binary)
        .args(["--help", "--opaque-option=not-shown"])
        .output()
        .expect("DiskSage Maven cache CLI must launch for invalid help composition");

    assert!(
        !output.status.success(),
        "help must not turn an otherwise invalid invocation into success"
    );
    assert!(
        output.stdout.is_empty(),
        "invalid invocation must not emit successful help on stdout"
    );
    let stderr = String::from_utf8(output.stderr).expect("CLI diagnostics must be valid UTF-8");
    assert!(!stderr.is_empty(), "invalid invocation must remain visible");
    assert!(
        !stderr.contains("not-shown"),
        "invalid diagnostics must not echo arbitrary argument payloads"
    );
}

fn assert_unknown_argument_payload_is_redacted(binary: &str) {
    let output = Command::new(binary)
        .arg("--opaque-option=not-shown")
        .output()
        .expect("DiskSage Maven cache CLI must launch for invalid argument validation");

    assert!(
        !output.status.success(),
        "unknown arguments must remain a non-zero validation failure"
    );
    assert!(
        output.stdout.is_empty(),
        "invalid invocation must not emit machine-readable success output"
    );
    let stderr = String::from_utf8(output.stderr).expect("CLI diagnostics must be valid UTF-8");
    assert!(!stderr.is_empty(), "invalid invocation must remain visible");
    assert!(
        !stderr.contains("not-shown"),
        "invalid diagnostics must not reflect opaque user-supplied argument payloads"
    );
}

#[test]
fn maven_cache_audit_help_is_successful_and_invalid_payloads_stay_bounded() {
    let binary = env!("CARGO_BIN_EXE_disksage-maven-cache-audit");
    assert_help_success(binary, "--help", "usage: disksage-maven-cache-audit");
    assert_help_success(binary, "-h", "usage: disksage-maven-cache-audit");
    assert_help_does_not_hide_invalid_argument(binary);
    assert_unknown_argument_payload_is_redacted(binary);
}

#[test]
fn maven_cache_prune_help_is_successful_and_invalid_payloads_stay_bounded() {
    let binary = env!("CARGO_BIN_EXE_disksage-maven-cache-prune");
    assert_help_success(binary, "--help", "usage: disksage-maven-cache-prune");
    assert_help_success(binary, "-h", "usage: disksage-maven-cache-prune");
    assert_help_does_not_hide_invalid_argument(binary);
    assert_unknown_argument_payload_is_redacted(binary);
}
