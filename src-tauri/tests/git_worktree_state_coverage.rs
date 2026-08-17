//! Coverage for detached Git worktree state emitted by real porcelain output.
//!
//! Existing integration coverage owns locked-worktree behavior. This regression exercises the
//! separate branchless detached-candidate path using only isolated temporary repositories and
//! worktrees, without deleting user data, branches, or repository history.

use disksage_lib::git_worktree::{
    audit_git_worktrees, GitWorktreeAuditOptions, GitWorktreeDisposition,
};
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

#[cfg(unix)]
#[test]
fn detached_merged_secondary_worktree_remains_a_branchless_candidate() {
    let root = initialized_repository();
    let initial_head = git_output(root.path(), &["rev-parse", "HEAD"]);
    std::fs::write(root.path().join("retained.txt"), b"retained\n").unwrap();
    git(root.path(), &["add", "retained.txt"]);
    git(root.path(), &["commit", "-q", "-m", "advance retained tip"]);

    let secondary_parent = tempfile::tempdir().unwrap();
    let secondary = secondary_parent.path().join("detached-worktree");
    let secondary_text = secondary.to_string_lossy().into_owned();
    git(
        root.path(),
        &["worktree", "add", "-q", "--detach", &secondary_text, &initial_head],
    );

    let report = audit_git_worktrees(
        root.path(),
        &["HEAD".into()],
        GitWorktreeAuditOptions::default(),
        3_000,
    )
    .unwrap();
    let entry = report
        .entries
        .iter()
        .find(|entry| entry.path == secondary_text)
        .expect("detached secondary worktree must be audited");

    assert!(entry.detached);
    assert_eq!(entry.branch, None);
    assert_eq!(entry.contained_in_reference, Some(true));
    assert!(!entry.head_is_retained_tip);
    assert_eq!(entry.disposition, GitWorktreeDisposition::RemovalCandidate);
    assert!(entry.blockers.is_empty());
    assert_eq!(report.removal_candidate_count, 1);
    assert!(!report.filesystem_mutation_executed);

    git(root.path(), &["worktree", "remove", "--force", &secondary_text]);
}
