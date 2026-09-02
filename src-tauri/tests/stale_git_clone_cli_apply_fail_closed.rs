use std::process::Command;

#[test]
fn cli_does_not_advertise_or_parse_disabled_stale_clone_removal() {
    let binary = env!("CARGO_BIN_EXE_disksage-stale-git-clone");

    let help = Command::new(binary)
        .arg("--help")
        .output()
        .expect("stale clone CLI help must execute");
    assert!(!help.status.success(), "the current CLI routes help through usage output");
    let help_stderr = String::from_utf8(help.stderr).expect("help stderr is UTF-8");
    assert!(help_stderr.contains("usage: disksage-stale-git-clone"));
    assert!(
        !help_stderr.contains("--apply"),
        "a disabled destructive operation must not appear in the public CLI contract"
    );

    let output = Command::new(binary)
        .args([
            "--repository-root",
            "/definitely-not-a-real-disksage-repository",
            "--apply",
            "--plan-fingerprint",
            "reviewed-plan",
            "--confirmation-phrase",
            "reviewed-confirmation",
            "--rationale",
            "reviewed by operator",
        ])
        .output()
        .expect("stale clone CLI invalid mutation request must terminate");

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("stderr is UTF-8");
    assert!(stderr.contains("usage: disksage-stale-git-clone"));
    assert!(
        !stderr.contains("removal-identity-bound-trash-unavailable"),
        "unsupported destructive authority must be rejected at argument admission rather than leaking an internal disabled-mode error"
    );
}
