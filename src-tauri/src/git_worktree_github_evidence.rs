//! One-deadline acquisition of GitHub pull-request evidence for Git worktree decisions.
//!
//! The forge-evidence phase owns a separate whole-operation budget. `command_timeout_ms` remains a
//! local child-process deadline and is never reused as the aggregate budget. Callers reuse the
//! returned evidence for one audit or one live re-audit and keep filesystem scanning on its own
//! separately bounded option.

use std::path::Path;
use std::time::Instant;

use crate::git_worktree::{
    self, ClosedPullRequestHeads, GitWorktreeAuditOptions, PullRequestCommitMembership,
    StaleOpenPullRequestHeads, MAX_LOCAL_COMMAND_TIMEOUT_MS,
};

/// Maximum wall-clock budget for the complete GitHub evidence phase.
///
/// Individual `gh`/Git subprocesses stay independently bounded by
/// [`MAX_LOCAL_COMMAND_TIMEOUT_MS`], so this budget can cover several sequential API queries
/// without granting any one local child the whole phase deadline.
pub const GITHUB_EVIDENCE_OPERATION_TIMEOUT_MS: u64 = 600_000;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GitHubPullRequestEvidence {
    pub closed_heads: ClosedPullRequestHeads,
    pub stale_open_heads: StaleOpenPullRequestHeads,
    pub pull_request_commits: PullRequestCommitMembership,
}

fn remaining_local_options(
    options: GitWorktreeAuditOptions,
    started: Instant,
) -> Result<GitWorktreeAuditOptions, String> {
    let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let remaining_operation_ms = GITHUB_EVIDENCE_OPERATION_TIMEOUT_MS.saturating_sub(elapsed_ms);
    if remaining_operation_ms == 0 {
        return Err("github-pr-evidence-timeout".into());
    }
    Ok(GitWorktreeAuditOptions {
        command_timeout_ms: options
            .command_timeout_ms
            .min(MAX_LOCAL_COMMAND_TIMEOUT_MS)
            .min(remaining_operation_ms),
        ..options
    })
}

/// Collect every requested GitHub PR evidence stream under one aggregate wall-clock budget while
/// retaining a distinct, shorter deadline for each local subprocess.
pub fn collect(
    repository_root: &Path,
    include_closed_pull_requests: bool,
    stale_open_pull_request_cutoff_ms: Option<u64>,
    options: GitWorktreeAuditOptions,
) -> Result<GitHubPullRequestEvidence, String> {
    if !include_closed_pull_requests && stale_open_pull_request_cutoff_ms.is_none() {
        return Ok(GitHubPullRequestEvidence::default());
    }

    let started = Instant::now();
    let closed_heads = if include_closed_pull_requests {
        git_worktree::github_closed_pull_request_heads_with_options(
            repository_root,
            remaining_local_options(options, started)?,
        )?
    } else {
        Default::default()
    };

    let exact = git_worktree::github_exact_pull_request_commit_membership(
        repository_root,
        remaining_local_options(options, started)?.command_timeout_ms,
    )?;
    let mut pull_request_commits = git_worktree::github_pull_request_commit_membership_with_exact(
        repository_root,
        remaining_local_options(options, started)?,
        exact,
    )?;
    if !include_closed_pull_requests {
        pull_request_commits.completed.clear();
    }

    let stale_open_heads = if let Some(cutoff_ms) = stale_open_pull_request_cutoff_ms {
        let remaining = remaining_local_options(options, started)?;
        git_worktree::github_stale_open_pull_request_heads(
            repository_root,
            cutoff_ms,
            remaining.command_timeout_ms,
        )?
    } else {
        Default::default()
    };

    Ok(GitHubPullRequestEvidence {
        closed_heads,
        stale_open_heads,
        pull_request_commits,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregate_evidence_budget_never_becomes_one_local_command_deadline() {
        let options = GitWorktreeAuditOptions {
            command_timeout_ms: 3_600_000,
            ..GitWorktreeAuditOptions::default()
        };
        let local = remaining_local_options(options, Instant::now()).unwrap();
        assert_eq!(local.command_timeout_ms, MAX_LOCAL_COMMAND_TIMEOUT_MS);
        assert!(local.command_timeout_ms < GITHUB_EVIDENCE_OPERATION_TIMEOUT_MS);
    }

    #[test]
    fn caller_local_deadline_is_preserved_when_below_the_cap() {
        let options = GitWorktreeAuditOptions {
            command_timeout_ms: 100,
            ..GitWorktreeAuditOptions::default()
        };
        let started = Instant::now();
        let first = remaining_local_options(options, started).unwrap();
        assert!(first.command_timeout_ms <= 100);
        std::thread::sleep(std::time::Duration::from_millis(5));
        let later = remaining_local_options(options, started).unwrap();
        assert!(later.command_timeout_ms <= first.command_timeout_ms);
    }
}
