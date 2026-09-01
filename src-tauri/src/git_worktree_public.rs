//! Public Git worktree safety boundary with a hard local-subprocess deadline.
//!
//! The implementation accepts a caller-selected command budget because GitHub evidence collection
//! also uses that value as a whole-operation budget. This facade prevents that aggregate budget
//! from becoming a one-hour `git`, `gh`, `lsof`, or `ps` child-process deadline. Higher-level
//! orchestration may keep a longer total budget, but every call into the local implementation is
//! capped independently before any subprocess can start.

pub use crate::git_worktree_impl::{
    approve_stale_worktree_removal, prepare_worktree_record_directory, public_summary,
    validate_reference, write_immutable_worktree_record, ClosedPullRequestHeads,
    GitWorktreeActiveUseEvidence, GitWorktreeAuditEntry, GitWorktreeAuditOptions,
    GitWorktreeAuditPublicSummary, GitWorktreeAuditReport, GitWorktreeDisposition,
    GitWorktreeReferenceBinding, GitWorktreeRemovalApproval, GitWorktreeRemovalItemResult,
    GitWorktreeRemovalResult, GitWorktreeSizeEvidence, PullRequestCommitMembership,
    PullRequestCommits, StaleOpenPullRequestHeads, GIT_WORKTREE_AUDIT_SCHEMA_KIND,
    MAX_REFERENCE_BYTES,
};

use std::path::Path;

/// Maximum wall-clock time one local Git-worktree subprocess may inherit from a caller.
///
/// Two minutes matches DiskSage's other bounded maintenance commands while leaving long-running
/// GitHub evidence acquisition free to budget multiple independently bounded calls.
pub const MAX_LOCAL_COMMAND_TIMEOUT_MS: u64 = 120_000;

fn validate_local_command_timeout(timeout_ms: u64) -> Result<(), String> {
    if timeout_ms == 0 || timeout_ms > MAX_LOCAL_COMMAND_TIMEOUT_MS {
        return Err("git-worktree-command-timeout-out-of-bounds".into());
    }
    Ok(())
}

fn validate_local_options(options: GitWorktreeAuditOptions) -> Result<(), String> {
    validate_local_command_timeout(options.command_timeout_ms)
}

/// Probe active use only with a bounded local process deadline.
pub fn active_use_evidence(
    path: &Path,
    timeout_ms: u64,
    max_pids: usize,
    recursive: bool,
) -> GitWorktreeActiveUseEvidence {
    if validate_local_command_timeout(timeout_ms).is_err() {
        return GitWorktreeActiveUseEvidence {
            method: if recursive {
                "lsof-recursive-pid"
            } else {
                "lsof-file-pid"
            }
            .into(),
            assessed: false,
            evidence_complete: false,
            active: false,
            observed_pids: Vec::new(),
            results_truncated: false,
            error: Some("git-worktree-command-timeout-out-of-bounds".into()),
        };
    }
    crate::git_worktree_impl::active_use_evidence(path, timeout_ms, max_pids, recursive)
}

/// Resolve closed PR heads without allowing an aggregate caller budget to become one `gh` timeout.
pub fn github_closed_pull_request_heads(
    repository_root: &Path,
    timeout_ms: u64,
) -> Result<ClosedPullRequestHeads, String> {
    validate_local_command_timeout(timeout_ms)?;
    crate::git_worktree_impl::github_closed_pull_request_heads(repository_root, timeout_ms)
}

/// Resolve closed PR heads under locally bounded command options.
pub fn github_closed_pull_request_heads_with_options(
    repository_root: &Path,
    options: GitWorktreeAuditOptions,
) -> Result<ClosedPullRequestHeads, String> {
    validate_local_options(options)?;
    crate::git_worktree_impl::github_closed_pull_request_heads_with_options(repository_root, options)
}

/// Resolve PR commit membership under locally bounded command options.
pub fn github_pull_request_commit_membership(
    repository_root: &Path,
    options: GitWorktreeAuditOptions,
) -> Result<PullRequestCommitMembership, String> {
    validate_local_options(options)?;
    crate::git_worktree_impl::github_pull_request_commit_membership(repository_root, options)
}

pub(crate) fn github_exact_pull_request_commit_membership(
    repository_root: &Path,
    timeout_ms: u64,
) -> Result<PullRequestCommitMembership, String> {
    validate_local_command_timeout(timeout_ms)?;
    crate::git_worktree_impl::github_exact_pull_request_commit_membership(
        repository_root,
        timeout_ms,
    )
}

pub(crate) fn github_pull_request_commit_membership_with_exact(
    repository_root: &Path,
    options: GitWorktreeAuditOptions,
    exact: PullRequestCommitMembership,
) -> Result<PullRequestCommitMembership, String> {
    validate_local_options(options)?;
    crate::git_worktree_impl::github_pull_request_commit_membership_with_exact(
        repository_root,
        options,
        exact,
    )
}

/// Resolve stale-open PR heads with a bounded local `gh` deadline.
pub fn github_stale_open_pull_request_heads(
    repository_root: &Path,
    cutoff_ms: u64,
    timeout_ms: u64,
) -> Result<StaleOpenPullRequestHeads, String> {
    validate_local_command_timeout(timeout_ms)?;
    crate::git_worktree_impl::github_stale_open_pull_request_heads(
        repository_root,
        cutoff_ms,
        timeout_ms,
    )
}

/// Audit linked worktrees only after bounding every local subprocess deadline.
pub fn audit_git_worktrees(
    repository_root: &Path,
    retention_references: &[String],
    options: GitWorktreeAuditOptions,
    generated_at_ms: u64,
) -> Result<GitWorktreeAuditReport, String> {
    validate_local_options(options)?;
    crate::git_worktree_impl::audit_git_worktrees(
        repository_root,
        retention_references,
        options,
        generated_at_ms,
    )
}

/// Audit with closed-PR authority only after bounding every local subprocess deadline.
pub fn audit_git_worktrees_with_closed_pull_request_heads(
    repository_root: &Path,
    retention_references: &[String],
    closed_pull_request_heads: &ClosedPullRequestHeads,
    options: GitWorktreeAuditOptions,
    generated_at_ms: u64,
) -> Result<GitWorktreeAuditReport, String> {
    validate_local_options(options)?;
    crate::git_worktree_impl::audit_git_worktrees_with_closed_pull_request_heads(
        repository_root,
        retention_references,
        closed_pull_request_heads,
        options,
        generated_at_ms,
    )
}

/// Audit with closed and stale-open PR authority under bounded local command options.
pub fn audit_git_worktrees_with_pull_request_heads(
    repository_root: &Path,
    retention_references: &[String],
    closed_pull_request_heads: &ClosedPullRequestHeads,
    stale_open_pull_request_heads: &StaleOpenPullRequestHeads,
    stale_open_pull_request_cutoff_ms: Option<u64>,
    options: GitWorktreeAuditOptions,
    generated_at_ms: u64,
) -> Result<GitWorktreeAuditReport, String> {
    validate_local_options(options)?;
    crate::git_worktree_impl::audit_git_worktrees_with_pull_request_heads(
        repository_root,
        retention_references,
        closed_pull_request_heads,
        stale_open_pull_request_heads,
        stale_open_pull_request_cutoff_ms,
        options,
        generated_at_ms,
    )
}

/// Audit exact PR membership under bounded local command options.
pub fn audit_git_worktrees_with_pull_request_membership(
    repository_root: &Path,
    retention_references: &[String],
    closed_pull_request_heads: &ClosedPullRequestHeads,
    stale_open_pull_request_heads: &StaleOpenPullRequestHeads,
    pull_request_commits: &PullRequestCommitMembership,
    stale_open_pull_request_cutoff_ms: Option<u64>,
    options: GitWorktreeAuditOptions,
    generated_at_ms: u64,
) -> Result<GitWorktreeAuditReport, String> {
    validate_local_options(options)?;
    crate::git_worktree_impl::audit_git_worktrees_with_pull_request_membership(
        repository_root,
        retention_references,
        closed_pull_request_heads,
        stale_open_pull_request_heads,
        pull_request_commits,
        stale_open_pull_request_cutoff_ms,
        options,
        generated_at_ms,
    )
}

/// Execute stale-worktree removal only with bounded local command deadlines.
pub fn execute_stale_worktree_removal(
    approved_report: &GitWorktreeAuditReport,
    approval: &GitWorktreeRemovalApproval,
    confirmation_exact_approval_phrase: &str,
    options: GitWorktreeAuditOptions,
    requested_at_ms: u64,
) -> Result<GitWorktreeRemovalResult, String> {
    validate_local_options(options)?;
    crate::git_worktree_impl::execute_stale_worktree_removal(
        approved_report,
        approval,
        confirmation_exact_approval_phrase,
        options,
        requested_at_ms,
    )
}

/// Execute with fresh closed-PR evidence while keeping each local child process bounded.
pub fn execute_stale_worktree_removal_with_github_closed_pull_requests(
    approved_report: &GitWorktreeAuditReport,
    approval: &GitWorktreeRemovalApproval,
    confirmation_exact_approval_phrase: &str,
    include_closed_pull_requests: bool,
    options: GitWorktreeAuditOptions,
    requested_at_ms: u64,
) -> Result<GitWorktreeRemovalResult, String> {
    validate_local_options(options)?;
    crate::git_worktree_impl::execute_stale_worktree_removal_with_github_closed_pull_requests(
        approved_report,
        approval,
        confirmation_exact_approval_phrase,
        include_closed_pull_requests,
        options,
        requested_at_ms,
    )
}

/// Execute with fresh PR evidence while keeping each local child process bounded.
pub fn execute_stale_worktree_removal_with_github_pull_requests(
    approved_report: &GitWorktreeAuditReport,
    approval: &GitWorktreeRemovalApproval,
    confirmation_exact_approval_phrase: &str,
    include_closed_pull_requests: bool,
    stale_open_pull_request_cutoff_ms: Option<u64>,
    options: GitWorktreeAuditOptions,
    requested_at_ms: u64,
) -> Result<GitWorktreeRemovalResult, String> {
    validate_local_options(options)?;
    crate::git_worktree_impl::execute_stale_worktree_removal_with_github_pull_requests(
        approved_report,
        approval,
        confirmation_exact_approval_phrase,
        include_closed_pull_requests,
        stale_open_pull_request_cutoff_ms,
        options,
        requested_at_ms,
    )
}
