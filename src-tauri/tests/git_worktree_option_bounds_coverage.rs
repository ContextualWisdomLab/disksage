//! Boundary coverage for Git worktree audit configuration.
//!
//! These regressions exercise the public audit entrypoint before any Git subprocess or filesystem
//! mutation can occur. Invalid resource budgets must fail closed with stable reason codes, and a
//! syntactically valid budget must still reject a relative repository root.

use disksage_lib::git_worktree::{audit_git_worktrees, GitWorktreeAuditOptions};
use std::path::Path;

fn audit_error(options: GitWorktreeAuditOptions) -> String {
    audit_git_worktrees(Path::new("."), &["HEAD".into()], options, 1)
        .expect_err("invalid audit configuration must fail closed")
}

#[test]
fn rejects_command_timeout_bounds_before_repository_access() {
    let mut options = GitWorktreeAuditOptions::default();
    options.command_timeout_ms = 0;
    assert_eq!(
        audit_error(options),
        "git-worktree-command-timeout-out-of-bounds"
    );

    options = GitWorktreeAuditOptions::default();
    options.command_timeout_ms = 300_001;
    assert_eq!(
        audit_error(options),
        "git-worktree-command-timeout-out-of-bounds"
    );
}

#[test]
fn rejects_size_timeout_bounds_before_repository_access() {
    let mut options = GitWorktreeAuditOptions::default();
    options.size_scan_timeout_ms = 0;
    assert_eq!(
        audit_error(options),
        "git-worktree-size-timeout-out-of-bounds"
    );

    options = GitWorktreeAuditOptions::default();
    options.size_scan_timeout_ms = 600_001;
    assert_eq!(
        audit_error(options),
        "git-worktree-size-timeout-out-of-bounds"
    );
}

#[test]
fn rejects_worktree_count_bounds_before_repository_access() {
    let mut options = GitWorktreeAuditOptions::default();
    options.max_worktrees = 0;
    assert_eq!(
        audit_error(options),
        "git-worktree-count-limit-out-of-bounds"
    );

    options = GitWorktreeAuditOptions::default();
    options.max_worktrees = 10_001;
    assert_eq!(
        audit_error(options),
        "git-worktree-count-limit-out-of-bounds"
    );
}

#[test]
fn rejects_entry_count_bounds_before_repository_access() {
    let mut options = GitWorktreeAuditOptions::default();
    options.max_entries_per_worktree = 0;
    assert_eq!(
        audit_error(options),
        "git-worktree-entry-limit-out-of-bounds"
    );

    options = GitWorktreeAuditOptions::default();
    options.max_entries_per_worktree = 20_000_001;
    assert_eq!(
        audit_error(options),
        "git-worktree-entry-limit-out-of-bounds"
    );
}

#[test]
fn rejects_active_pid_bounds_before_repository_access() {
    let mut options = GitWorktreeAuditOptions::default();
    options.max_active_pids = 0;
    assert_eq!(
        audit_error(options),
        "git-worktree-active-pid-limit-out-of-bounds"
    );

    options = GitWorktreeAuditOptions::default();
    options.max_active_pids = 4_097;
    assert_eq!(
        audit_error(options),
        "git-worktree-active-pid-limit-out-of-bounds"
    );
}

#[test]
fn valid_resource_bounds_still_require_an_absolute_repository_root() {
    assert_eq!(
        audit_error(GitWorktreeAuditOptions::default()),
        "git-worktree-repository-root-not-absolute"
    );
}
