use std::path::Path;
use std::process::{Command, Output};

fn run_without_explicit_window(
    repository_root: &Path,
    record_root: &Path,
    evidence_flag: &str,
    evidence_path: &Path,
) -> Output {
    Command::new(env!("CARGO_BIN_EXE_disksage-git-worktree-remove"))
        .arg("--repository-root")
        .arg(repository_root)
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
        .arg(record_root)
        .arg("--enable-orca-protections")
        .arg(evidence_flag)
        .arg(evidence_path)
        .output()
        .expect("remove CLI must execute")
}

fn assert_missing_window_precedes_evidence_io(output: &Output, record_root: &Path) {
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
        "mandatory mutation-boundary validation must precede evidence I/O; status={:?}; binary={}; stdout={}; stderr={}; record_root_exists={}",
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

#[test]
fn explicit_window_validation_precedes_orca_evidence_io() {
    let fixture = tempfile::tempdir().expect("temporary validation-order fixture");
    let repository_root = fixture.path().join("repository");
    std::fs::create_dir(&repository_root).expect("repository fixture directory");
    let missing_evidence = fixture.path().join("missing-orca-terminal.json");
    let record_root = fixture.path().join("records");

    let output = run_without_explicit_window(
        &repository_root,
        &record_root,
        "--orca-terminal-json",
        &missing_evidence,
    );
    assert_missing_window_precedes_evidence_io(&output, &record_root);
}

#[test]
fn explicit_window_validation_precedes_lead_queue_io() {
    let fixture = tempfile::tempdir().expect("temporary validation-order fixture");
    let repository_root = fixture.path().join("repository");
    std::fs::create_dir(&repository_root).expect("repository fixture directory");
    let missing_evidence = fixture.path().join("missing-lead-queue.md");
    let record_root = fixture.path().join("records");

    let output = run_without_explicit_window(
        &repository_root,
        &record_root,
        "--lead-queue-file",
        &missing_evidence,
    );
    assert_missing_window_precedes_evidence_io(&output, &record_root);
}
