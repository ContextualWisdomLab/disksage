#![cfg(windows)]

use std::process::Command;

#[test]
fn windows_prune_reaches_the_bounded_command_runner() {
    let output = Command::new(env!("CARGO_BIN_EXE_disksage-podman-reclaim-plan"))
        .args([
            "--execute-dangling",
            "--confirmation-phrase",
            "unused-on-purpose",
            "--rationale",
            "reviewed Windows execution boundary",
            "--podman-bin",
            r"C:\disksage-test-missing-podman.exe",
        ])
        .output()
        .expect("the shipped Podman reclaim CLI must start");

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("stderr must remain UTF-8");
    assert!(
        stderr.contains("podman-prune-machine-inspect-spawn"),
        "Windows must enter the bounded process runner before a missing executable is rejected: {stderr}"
    );
    assert!(
        !stderr.contains("podman-prune-process-tree-control-unavailable"),
        "Windows process-tree containment is implemented and must not be rejected as unsupported"
    );
}
