//! Real Git reference-resolution coverage for the worktree audit.
//!
//! These tests create only a temporary local repository. They exercise exact-OID retention
//! bindings and fail-closed resolution of a missing symbolic reference without fetching,
//! pruning, deleting a branch, removing a worktree, or contacting a provider.

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
        .expect("git object ids are ASCII")
        .trim()
        .to_owned()
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
fn exact_oid_retention_reference_is_bound_without_symbolic_resolution() {
    let root = initialized_repository();
    let head_oid = git_output(root.path(), &["rev-parse", "HEAD"]);

    let report = audit_git_worktrees(
        root.path(),
        std::slice::from_ref(&head_oid),
        GitWorktreeAuditOptions::default(),
        1,
    )
    .unwrap();

    assert_eq!(report.retention_references.len(), 1);
    assert_eq!(report.retention_references[0].reference_ref, head_oid);
    assert_eq!(
        report.retention_references[0].reference_oid,
        report.retention_references[0].reference_ref
    );
    assert_eq!(report.retention_reachable_commit_count, 1);
    assert_eq!(report.worktree_count, 1);
    assert_eq!(report.entries[0].contained_in_reference, Some(true));
    assert!(report.entries[0].head_is_retained_tip);
    assert!(report.evidence_complete);
}

#[test]
fn missing_symbolic_retention_reference_fails_closed() {
    let root = initialized_repository();

    assert_eq!(
        audit_git_worktrees(
            root.path(),
            &["refs/heads/does-not-exist".into()],
            GitWorktreeAuditOptions::default(),
            2,
        )
        .unwrap_err(),
        "git-reference-resolve-failed"
    );
}
