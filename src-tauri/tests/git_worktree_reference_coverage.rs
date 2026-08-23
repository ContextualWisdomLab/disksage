//! Coverage for bounded retention-reference admission in the Git worktree audit.
//!
//! The existing reference-resolution suite owns exact-OID and symbolic resolution behavior. These
//! regressions cover public-input cardinality and lexical admission before references can reach a
//! Git subprocess, without fetching, pruning, removing worktrees, or deleting branches.

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

#[test]
fn malformed_reference_values_fail_closed_before_git_resolution() {
    let root = initialized_repository();
    let oversized = "a".repeat(1_025);
    for reference in [
        "".to_string(),
        "-option-shaped".to_string(),
        "refs/heads/main\nsecond-command".to_string(),
        "refs/heads/main\0suffix".to_string(),
        oversized,
    ] {
        assert_eq!(
            audit_git_worktrees(
                root.path(),
                &[reference],
                GitWorktreeAuditOptions::default(),
                5_002,
            )
            .unwrap_err(),
            "git-worktree-reference-invalid"
        );
    }

    assert_eq!(
        git(root.path(), &["worktree", "list", "--porcelain"])
            .lines()
            .filter(|line| line.starts_with("worktree "))
            .count(),
        1,
        "reference admission must not mutate the disposable repository"
    );
}
