use std::process::Command;

#[test]
fn cli_exposes_only_read_only_transparent_compression_planning() {
    let binary = env!("CARGO_BIN_EXE_disksage-transparent-compress");

    let help = Command::new(binary)
        .arg("--help")
        .output()
        .expect("transparent compression CLI help must execute");
    assert!(help.status.success(), "--help must remain a successful read-only operation");
    assert!(help.stderr.is_empty(), "help must not be rendered as a command failure");
    let help_stdout = String::from_utf8(help.stdout).expect("help stdout is UTF-8");
    assert!(help_stdout.contains("usage: disksage-transparent-compress"));
    assert!(
        !help_stdout.contains("--apply"),
        "a disabled mutation must not be advertised in the public CLI contract"
    );

    let output = Command::new(binary)
        .args([
            "--root",
            "/definitely-not-an-authorized-disksage-root",
            "--apply",
            "--plan-fingerprint",
            "reviewed-plan",
            "--confirmation-phrase",
            "reviewed-phrase",
            "--rationale",
            "reviewed",
        ])
        .output()
        .expect("transparent compression invalid mutation request must terminate");

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("stderr is UTF-8");
    assert!(stderr.contains("usage: disksage-transparent-compress"));
    assert!(
        !stderr.contains("root-authorization-unavailable"),
        "unsupported mutation authority must be rejected at argument admission instead of leaking an internal disabled-mode error"
    );
}
