use std::process::Command;

#[test]
fn docker_native_cli_execute_fails_closed_without_authority_binding() {
    let output = Command::new(env!("CARGO_BIN_EXE_disksage-container-orphan-plan"))
        .args([
            "--runtime",
            "docker-native",
            "--bin",
            "__disksage_test_missing_docker__",
            "--execute",
            "container",
            "--confirm",
            "irrelevant-unbound-phrase",
            "--rationale",
            "Verify Docker authority binding before any destructive CLI execution.",
        ])
        .output()
        .expect("container orphan CLI must launch");

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("docker-native-cli-execution-requires-authority-binding"),
        "unexpected stderr: {stderr}"
    );
}
