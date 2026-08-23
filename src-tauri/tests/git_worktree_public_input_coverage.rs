//! Public-boundary coverage for Git worktree audit input admission.
//!
//! Invalid resource limits and a relative repository root must fail before Git or filesystem
//! domain work. These regressions exercise the shipped library boundary rather than private
//! parser helpers, so exact coverage reflects the same validation used by the operational CLI.

use disksage_lib::git_worktree::{audit_git_worktrees, GitWorktreeAuditOptions};
use std::path::Path;

fn expect_option_error(options: GitWorktreeAuditOptions, expected: &str) {
    let error = audit_git_worktrees(
        Path::new("relative-repository-is-never-reached"),
        &["HEAD".to_string()],
        options,
        1,
    )
    .expect_err("invalid options must fail before repository access");
    assert_eq!(error, expected);
}

#[test]
fn public_audit_rejects_every_resource_bound_before_domain_work() {
    let defaults = GitWorktreeAuditOptions::default();

    for command_timeout_ms in [0, 300_001] {
        expect_option_error(
            GitWorktreeAuditOptions {
                command_timeout_ms,
                ..defaults
            },
            "git-worktree-command-timeout-out-of-bounds",
        );
    }

    for size_scan_timeout_ms in [0, 600_001] {
        expect_option_error(
            GitWorktreeAuditOptions {
                size_scan_timeout_ms,
                ..defaults
            },
            "git-worktree-size-timeout-out-of-bounds",
        );
    }

    for max_worktrees in [0, 10_001] {
        expect_option_error(
            GitWorktreeAuditOptions {
                max_worktrees,
                ..defaults
            },
            "git-worktree-count-limit-out-of-bounds",
        );
    }

    for max_entries_per_worktree in [0, 20_000_001] {
        expect_option_error(
            GitWorktreeAuditOptions {
                max_entries_per_worktree,
                ..defaults
            },
            "git-worktree-entry-limit-out-of-bounds",
        );
    }

    for max_active_pids in [0, 4_097] {
        expect_option_error(
            GitWorktreeAuditOptions {
                max_active_pids,
                ..defaults
            },
            "git-worktree-active-pid-limit-out-of-bounds",
        );
    }
}

#[test]
fn public_audit_rejects_relative_repository_root_after_valid_options() {
    let error = audit_git_worktrees(
        Path::new("relative-repository"),
        &["HEAD".to_string()],
        GitWorktreeAuditOptions::default(),
        1,
    )
    .expect_err("relative repository roots must fail before Git execution");
    assert_eq!(error, "git-worktree-repository-root-not-absolute");
}
