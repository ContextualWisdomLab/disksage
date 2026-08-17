//! Coverage for bounded retention-reference admission in the Git worktree audit.
//!
//! The existing reference-resolution suite owns exact-OID and symbolic resolution behavior. This
//! regression covers the separate public-input cardinality boundary without fetching, pruning,
//! removing worktrees, or deleting branches.

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
