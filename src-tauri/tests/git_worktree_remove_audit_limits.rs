#![cfg(feature = "cloud-cli")]

use std::process::Command;

#[test]
fn removal_cli_accepts_the_exact_audit_resource_limits() {
    let repository_root = tempfile::tempdir().expect("temporary repository root must be created");
    let record_root = tempfile::tempdir().expect("temporary record root must be created");

    let output = Command::new(env!("CARGO_BIN_EXE_disksage-git-worktree-remove"))
        .arg("--repository-root")
        .arg(repository_root.path())
        .args(["--reference-ref", "origin/develop"])
        .args(["--command-timeout-ms", "1234"])
        .args(["--size-scan-timeout-ms", "5678"])
        .args(["--max-worktrees", "17"])
        .args(["--max-entries-per-worktree", "2345"])
        .args(["--max-active-pids", "9"])
        .args([
            "--approved-removal-plan-fingerprint",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ])
        .args([
            "--confirmation-exact-approval-phrase",
            "DiskSage stale worktree approval",
        ])
        .args(["--reviewed-by", "human:test"])
        .args(["--rationale", "operator reviewed the exact bounded audit"])
        .arg("--record-root")
        .arg(record_root.path())
        .output()
        .expect("worktree removal CLI must launch");

    assert_ne!(
        output.status.code(),
        Some(64),
        "resource limits accepted by the audit CLI must also cross the removal CLI parser; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
