//! One-deadline acquisition of GitHub pull-request evidence for Git worktree decisions.
//!
//! `command_timeout_ms` is the maximum wall-clock budget for the complete forge-evidence phase,
//! not a fresh allowance for each GitHub lookup. Callers reuse the returned evidence for one audit
//! or one live re-audit and keep local filesystem scanning on its separately bounded options.

use std::path::Path;
use std::time::Instant;

use crate::git_worktree::{
    self, ClosedPullRequestHeads, GitWorktreeAuditOptions, PullRequestCommitMembership,
    StaleOpenPullRequestHeads,
};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GitHubPullRequestEvidence {
    pub closed_heads: ClosedPullRequestHeads,
    pub stale_open_heads: StaleOpenPullRequestHeads,
    pub pull_request_commits: PullRequestCommitMembership,
}

fn remaining_options(
    options: GitWorktreeAuditOptions,
    started: Instant,
) -> Result<GitWorktreeAuditOptions, String> {
    let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let remaining_ms = options.command_timeout_ms.saturating_sub(elapsed_ms);
    if remaining_ms == 0 {
        return Err("github-pr-evidence-timeout".into());
    }
    Ok(GitWorktreeAuditOptions {
        command_timeout_ms: remaining_ms,
        ..options
    })
}

/// Collect every requested GitHub PR evidence stream under one caller-supplied wall-clock budget.
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
            remaining_options(options, started)?,
        )?
    } else {
        Default::default()
    };

    let mut pull_request_commits = git_worktree::github_pull_request_commit_membership(
        repository_root,
        remaining_options(options, started)?,
    )?;
    if !include_closed_pull_requests {
        pull_request_commits.completed.clear();
    }

    let stale_open_heads = if let Some(cutoff_ms) = stale_open_pull_request_cutoff_ms {
        let remaining = remaining_options(options, started)?;
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
    fn remaining_budget_never_refreshes_the_original_timeout() {
        let options = GitWorktreeAuditOptions {
            command_timeout_ms: 100,
            ..GitWorktreeAuditOptions::default()
        };
        let started = Instant::now();
        let first = remaining_options(options, started).unwrap();
        assert!(first.command_timeout_ms <= 100);
        std::thread::sleep(std::time::Duration::from_millis(5));
        let later = remaining_options(options, started).unwrap();
        assert!(later.command_timeout_ms < first.command_timeout_ms);
    }
}
