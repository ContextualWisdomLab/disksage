//! Coverage for the bounded registered-worktree cardinality guard.
//!
//! The fixture creates a second worktree inside temporary storage, then configures the public audit
//! for a single permitted registration. DiskSage must fail closed after the real Git worktree list
//! exceeds that bound; no worktree is removed and no branch is deleted.

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
fn real_registered_worktree_count_above_configured_limit_fails_closed() {
    let root = initialized_repository();
    git(root.path(), &["branch", "count-limit-secondary"]);

    let secondary_parent = tempfile::tempdir().unwrap();
    let secondary = secondary_parent.path().join("secondary-worktree");
    let secondary_text = secondary.to_string_lossy().into_owned();
    git(
        root.path(),
        &["worktree", "add", "-q", &secondary_text, "count-limit-secondary"],
    );

    let options = GitWorktreeAuditOptions {
        max_worktrees: 1,
        ..GitWorktreeAuditOptions::default()
    };
    assert_eq!(
        audit_git_worktrees(root.path(), &["HEAD".into()], options, 6_000).unwrap_err(),
        "git-worktree-list-exceeds-limit"
    );

    assert!(secondary.exists());
    git(root.path(), &["worktree", "remove", "--force", &secondary_text]);
}
