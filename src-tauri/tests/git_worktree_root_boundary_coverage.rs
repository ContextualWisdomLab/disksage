//! Coverage for real-directory admission at the Git worktree audit boundary.
//!
//! The production audit rejects symlinked and non-directory repository roots before any Git command
//! can act on them. The fixtures are temporary and never mutate user repositories.

use disksage_lib::git_worktree::{audit_git_worktrees, GitWorktreeAuditOptions};

#[cfg(unix)]
#[test]
fn symlinked_repository_root_fails_closed_before_git_execution() {
    use std::os::unix::fs::symlink;

    let target = tempfile::tempdir().unwrap();
    let parent = tempfile::tempdir().unwrap();
    let link = parent.path().join("repo-link");
    symlink(target.path(), &link).unwrap();

    assert_eq!(
        audit_git_worktrees(
            &link,
            &["HEAD".into()],
            GitWorktreeAuditOptions::default(),
            8_000,
        )
        .unwrap_err(),
        "worktree-path-not-real-directory"
    );
}

#[test]
fn regular_file_repository_root_fails_closed_before_git_execution() {
    let parent = tempfile::tempdir().unwrap();
    let file = parent.path().join("not-a-repository-directory");
    std::fs::write(&file, b"not a directory\n").unwrap();

    assert_eq!(
        audit_git_worktrees(
            &file,
            &["HEAD".into()],
            GitWorktreeAuditOptions::default(),
            8_001,
        )
        .unwrap_err(),
        "worktree-path-not-real-directory"
    );
}
