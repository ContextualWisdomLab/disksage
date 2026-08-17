//! Coverage for exact-OID and bounded retention-reference admission in the Git worktree audit.
//!
//! These tests use a local temporary repository only and exercise the production public audit API
//! without fetching, pruning, removing worktrees, or deleting branches.

use disksage_lib::git_worktree::{audit_git_worktrees, GitWorktreeAuditOptions};
use std::path::Path;
use std::process::Command;

fn git(cwd: &Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .status()
        .expect("git must be available in the test environment");
    assert!(status.success(), "git {args:?} failed");
}

fn git_output(cwd: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("git must be available in the test environment");
    assert!(output.status.success(), "git {args:?} failed");
    String::from_utf8(output.stdout)
        .expect("git fixture output must be UTF-8")
        .trim()
        .to_string()
}

fn initialized_repository() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    git(temp.path(), &["init", "-q"]);
    git(temp.path(), &["config", "user.name", "DiskSage Test"]);
    git(temp.path(), &["config", "user.email", "disksage@example.invalid"]);
    std::fs::write(temp.path().join("tracked.txt"), b"tracked\n").unwrap();
    git(temp.path(), &["add", "tracked.txt"]);
    git(temp.path(), &["commit", "-q", "-m", "initial"]);
    temp
}

#[test]
fn exact_oid_reference_is_bound_without_symbolic_resolution() {
    let root = initialized_repository();
    let head = git_output(root.path(), &["rev-parse", "HEAD"]);

    let report = audit_git_worktrees(
        root.path(),
        std::slice::from_ref(&head),
        GitWorktreeAuditOptions::default(),
        5_000,
    )
    .unwrap();

    assert_eq!(report.retention_references.len(), 1);
    assert_eq!(report.retention_references[0].reference_ref, head);
    assert_eq!(
        report.retention_references[0].reference_oid,
        report.entries[0].head
    );
    assert!(report.entries[0].head_is_retained_tip);
    assert!(report.retention_reachable_commit_count >= 1);
    assert!(report.evidence_complete);
}

#[test]
fn oversized_reference_set_fails_closed_before_resolution() {
    let root = initialized_repository();
    let references = vec!["HEAD".to_string(); 10_001];

    assert_eq!(
        audit_git_worktrees(
            root.path(),
            &references,
            GitWorktreeAuditOptions::default(),
            5_001,
        )
        .unwrap_err(),
        "git-worktree-retention-reference-count-invalid"
    );
}
