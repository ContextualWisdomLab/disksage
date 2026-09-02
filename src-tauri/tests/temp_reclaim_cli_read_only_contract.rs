use std::process::Command;

#[test]
fn temp_reclaim_cli_exposes_only_the_read_only_plan_until_reversible_authority_exists() {
    let binary = env!("CARGO_BIN_EXE_disksage-temp-reclaim");

    let help = Command::new(binary)
        .arg("--help")
        .output()
        .expect("temp reclaim CLI help must execute");
    assert!(help.status.success(), "--help must be a successful read-only operation");
    let stdout = String::from_utf8(help.stdout).expect("help output must be UTF-8");
    assert_eq!(stdout.trim(), "usage: disksage-temp-reclaim");
    assert!(
        !stdout.contains("--execute"),
        "a permanently disabled destructive mode must not be advertised to customers"
    );

    let execute = Command::new(binary)
        .args(["--execute", "fingerprint", "phrase", "rationale"])
        .output()
        .expect("temp reclaim CLI invalid execution request must terminate");
    assert!(!execute.status.success(), "disabled execution must fail closed");
    let stderr = String::from_utf8(execute.stderr).expect("error output must be UTF-8");
    assert!(stderr.contains("usage: disksage-temp-reclaim"));
    assert!(
        !stderr.contains("private-approval"),
        "customer-facing failure must describe the supported interface, not an internal disabled authority"
    );
}
