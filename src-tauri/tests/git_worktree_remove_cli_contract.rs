//! Buyer-visible mutation-boundary contracts for the shipped stale-worktree removal CLI.
//!
//! These tests deliberately use real Git linked worktrees for destructive-boundary behavior. They
//! keep plan-drift rejection separate from argument-validation failures so a Windows CI failure
//! identifies the causal contract without weakening either assertion set.

use std::fs;
use std::path::Path;
use std::process::Command;

fn git(cwd: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_AUTHOR_NAME", "DiskSage Test")
        .env("GIT_AUTHOR_EMAIL", "disksage@example.invalid")
        .env("GIT_COMMITTER_NAME", "DiskSage Test")
        .env("GIT_COMMITTER_EMAIL", "disksage@example.invalid")
        .output()
        .expect("git should spawn");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn real_linked_worktree() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let temp = tempfile::tempdir().expect("temporary fixture root");
    let repository = temp.path().join("repository");
    let secondary = temp.path().join("secondary");
    fs::create_dir(&repository).expect("create repository root");

    git(&repository, &["init", "-q", "-b", "main"]);
    fs::write(repository.join("evidence.txt"), b"first\n").expect("write first revision");
    git(&repository, &["add", "evidence.txt"]);
    git(&repository, &["commit", "-q", "-m", "first"]);
    fs::write(repository.join("evidence.txt"), b"second\n").expect("write second revision");
    git(&repository, &["commit", "-q", "-am", "second"]);
    git(&repository, &["branch", "stale", "HEAD~1"]);
    git(
        &repository,
        &["worktree", "add", "-q", secondary.to_str().unwrap(), "stale"],
    );
    (temp, repository, secondary)
}

#[test]
fn stale_approved_plan_is_rejected_when_live_protection_changes_removal_authority() {
    let (temp, repository, secondary) = real_linked_worktree();
    let preserved_file = secondary.join("evidence.txt");
    let preserved_bytes = fs::read(&preserved_file).expect("snapshot linked worktree file");

    let audit_binary = env!("CARGO_BIN_EXE_disksage-git-worktree-audit");
    let baseline = Command::new(audit_binary)
        .arg("--repository-root")
        .arg(&repository)
        .arg("--reference-ref")
        .arg("main")
        .output()
        .expect("shipped audit CLI should produce the reviewed removal plan");
    assert!(
        baseline.status.success(),
        "baseline audit must be executable: {}",
        String::from_utf8_lossy(&baseline.stderr)
    );
    let baseline_summary: serde_json::Value =
        serde_json::from_slice(&baseline.stdout).expect("baseline audit public JSON");
    assert_eq!(
        baseline_summary["removal_candidate_count"],
        1,
        "{baseline_summary:#}"
    );
    let plan_fingerprint = baseline_summary["removal_plan_fingerprint"]
        .as_str()
        .expect("reviewed plan fingerprint");
    let approval_phrase = baseline_summary["exact_approval_phrase"]
        .as_str()
        .expect("reviewed exact approval phrase");

    let live_path = fs::canonicalize(&secondary).expect("canonical live worktree path");
    let terminal_json = temp.path().join("orca-terminals-after-review.json");
    fs::write(
        &terminal_json,
        serde_json::to_vec(&serde_json::json!({
            "result": {"terminals": [{"worktreePath": live_path}]}
        }))
        .expect("serialize post-review terminal fixture"),
    )
    .expect("write post-review terminal fixture");
    let record_root = temp.path().join("records");

    let remove_binary = env!("CARGO_BIN_EXE_disksage-git-worktree-remove");
    let removal = Command::new(remove_binary)
        .arg("--repository-root")
        .arg(&repository)
        .arg("--reference-ref")
        .arg("main")
        .arg("--enable-orca-protections")
        .arg("--recent-write-window-secs")
        .arg("3600")
        .arg("--orca-terminal-json")
        .arg(&terminal_json)
        .arg("--approved-removal-plan-fingerprint")
        .arg(plan_fingerprint)
        .arg("--confirmation-exact-approval-phrase")
        .arg(approval_phrase)
        .arg("--reviewed-by")
        .arg("human:disksage-test")
        .arg("--rationale")
        .arg("Verify post-review live protection before mutation.")
        .arg("--record-root")
        .arg(&record_root)
        .output()
        .expect("shipped remove CLI should start");

    assert!(!removal.status.success(), "new live protection must veto removal");
    let removal_stderr = String::from_utf8_lossy(&removal.stderr);
    assert!(
        removal_stderr.contains("git-worktree-removal-plan-fingerprint-mismatch"),
        "remove CLI must reacquire live protection and reject the reviewed plan before approval/mutation; stderr={removal_stderr}"
    );
    assert!(
        !removal_stderr.contains("unknown option"),
        "protection inputs must remain a supported mutation-time buyer contract; stderr={removal_stderr}"
    );
    assert!(secondary.exists(), "live worktree path must remain after the veto");
    assert_eq!(
        fs::read(&preserved_file).expect("read worktree after remove-CLI veto"),
        preserved_bytes,
        "remove-CLI veto must preserve the exact checked-out bytes"
    );
    assert!(
        !record_root.exists(),
        "plan mismatch caused by fresh protection must happen before approval/result records"
    );
}

#[test]
fn orca_enabled_remove_requires_explicit_recent_write_window_before_execution() {
    let temp = tempfile::tempdir().expect("temporary fixture root");
    let repository = temp.path().join("repository");
    let live_path = temp.path().join("live-worktree");
    fs::create_dir(&repository).expect("create repository root");
    fs::create_dir(&live_path).expect("create live-path fixture");
    let terminal_json = temp.path().join("orca-terminals.json");
    fs::write(
        &terminal_json,
        serde_json::to_vec(&serde_json::json!({
            "result": {"terminals": [{"worktreePath": live_path}]}
        }))
        .expect("serialize terminal fixture"),
    )
    .expect("write terminal fixture");
    let record_root = temp.path().join("records");

    let remove_binary = env!("CARGO_BIN_EXE_disksage-git-worktree-remove");
    let no_window = Command::new(remove_binary)
        .arg("--repository-root")
        .arg(&repository)
        .arg("--reference-ref")
        .arg("main")
        .arg("--enable-orca-protections")
        .arg("--orca-terminal-json")
        .arg(&terminal_json)
        .arg("--approved-removal-plan-fingerprint")
        .arg("a".repeat(64))
        .arg("--confirmation-exact-approval-phrase")
        .arg("DiskSage stale worktree approval")
        .arg("--reviewed-by")
        .arg("human:disksage-test")
        .arg("--rationale")
        .arg("No implicit recent-write window is permitted.")
        .arg("--record-root")
        .arg(&record_root)
        .output()
        .expect("shipped remove CLI should start for fail-closed validation");

    let stdout = String::from_utf8_lossy(&no_window.stdout);
    let stderr = String::from_utf8_lossy(&no_window.stderr);
    assert!(
        !no_window.status.success(),
        "Orca-enabled removal without an explicit recent-write window must fail; status={:?}; binary={remove_binary:?}; stdout={stdout:?}; stderr={stderr:?}; record_root_exists={}",
        no_window.status,
        record_root.exists()
    );
    assert!(
        stderr.contains("--recent-write-window-secs"),
        "remove CLI Orca pack must require an explicit recent-write window; status={:?}; binary={remove_binary:?}; stdout={stdout:?}; stderr={stderr:?}; record_root_exists={}",
        no_window.status,
        record_root.exists()
    );
    assert!(
        !record_root.exists(),
        "argument validation must fail before creating approval/result records; status={:?}; binary={remove_binary:?}; stdout={stdout:?}; stderr={stderr:?}; record_root_exists={}",
        no_window.status,
        record_root.exists()
    );
}
