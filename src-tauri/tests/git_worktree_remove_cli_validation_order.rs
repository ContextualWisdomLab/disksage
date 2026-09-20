use std::process::Command;

#[test]
fn explicit_window_validation_precedes_orca_evidence_io() {
    let fixture = tempfile::tempdir().expect("temporary validation-order fixture");
    let repository_root = fixture.path().join("repository");
    std::fs::create_dir(&repository_root).expect("repository fixture directory");
    let missing_orca_terminal_json = fixture.path().join("missing-orca-terminal.json");
    let record_root = fixture.path().join("records");

    let output = Command::new(env!("CARGO_BIN_EXE_disksage-git-worktree-remove"))
        .arg("--repository-root")
        .arg(&repository_root)
        .arg("--reference-ref")
        .arg("HEAD")
        .arg("--approved-removal-plan-fingerprint")
        .arg("a".repeat(64))
        .arg("--confirmation-exact-approval-phrase")
        .arg("DiskSage stale worktree approval")
        .arg("--reviewed-by")
        .arg("human:test")
        .arg("--rationale")
        .arg("validation ordering must fail closed before evidence I/O")
        .arg("--record-root")
        .arg(&record_root)
        .arg("--enable-orca-protections")
        .arg("--orca-terminal-json")
        .arg(&missing_orca_terminal_json)
        .output()
        .expect("remove CLI must execute");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "missing explicit recent-write window must fail; status={:?}; stdout={}; stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        stderr
    );
    assert!(
        stderr.contains("--recent-write-window-secs"),
        "mandatory mutation-boundary validation must precede Orca evidence I/O; status={:?}; binary={}; stdout={}; stderr={}; record_root_exists={}",
        output.status,
        env!("CARGO_BIN_EXE_disksage-git-worktree-remove"),
        String::from_utf8_lossy(&output.stdout),
        stderr,
        record_root.exists()
    );
    assert!(
        !record_root.exists(),
        "validation failure must not create approval/result evidence"
    );
}
