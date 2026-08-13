use std::process::Command;

fn assert_help_success(binary: &str, flag: &str, usage_marker: &str) {
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
    assert!(
        stdout.contains(usage_marker),
        "help output must contain the stable usage synopsis"
    );
}

fn assert_help_does_not_hide_invalid_argument(binary: &str) {
    let output = Command::new(binary)
        .env_remove("HOME")
        .args(["--help", "--opaque-option=not-shown"])
        .output()
        .expect("DiskSage operational CLI must launch for invalid help composition");

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

#[test]
fn icloud_sync_health_help_is_successful_without_environment_dependency() {
    let binary = env!("CARGO_BIN_EXE_disksage-icloud-sync-health");
    assert_help_success(binary, "--help", "usage: disksage-icloud-sync-health");
    assert_help_success(binary, "-h", "usage: disksage-icloud-sync-health");
    assert_help_does_not_hide_invalid_argument(binary);
}

#[test]
fn provider_oauth_help_is_successful_without_environment_dependency() {
    let binary = env!("CARGO_BIN_EXE_disksage-provider-oauth");
    assert_help_success(binary, "--help", "usage: disksage-provider-oauth");
    assert_help_success(binary, "-h", "usage: disksage-provider-oauth");
    assert_help_does_not_hide_invalid_argument(binary);
}
