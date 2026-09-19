//! Public Git worktree safety boundary with a hard local-subprocess deadline.
//!
//! The implementation accepts a caller-selected command budget because GitHub evidence collection
//! also uses that value as a whole-operation budget. This facade prevents that aggregate budget
//! from becoming a one-hour `git`, `gh`, `lsof`, or `ps` child-process deadline. Higher-level
//! orchestration may keep a longer total budget, but every call into the local implementation is
//! bounded independently before any subprocess can start.

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

use std::ffi::OsString;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// Maximum wall-clock time one local Git-worktree subprocess may inherit from a caller.
///
/// Two minutes bounds a single command independently while the higher-level GitHub evidence phase
/// may budget several sequential calls.
pub const MAX_LOCAL_COMMAND_TIMEOUT_MS: u64 = 120_000;
const MAX_IGNORED_STATUS_OUTPUT_BYTES: usize = 1024 * 1024;
const IGNORED_ARTIFACT_BLOCKER: &str = "ignored-artifacts-present";
const IGNORED_ARTIFACT_EVIDENCE_GAP: &str = "ignored-artifact-evidence-incomplete";

fn validate_local_command_timeout(timeout_ms: u64) -> Result<(), String> {
    if timeout_ms == 0 || timeout_ms > MAX_LOCAL_COMMAND_TIMEOUT_MS {
        return Err("git-worktree-command-timeout-out-of-bounds".into());
    }
    Ok(())
}

fn validate_local_options(options: GitWorktreeAuditOptions) -> Result<(), String> {
    validate_local_command_timeout(options.command_timeout_ms)
}

fn drain_bounded<R: Read + Send + 'static>(mut reader: R) -> thread::JoinHandle<(Vec<u8>, bool)> {
    thread::spawn(move || {
        let mut retained = Vec::new();
        let mut truncated = false;
        let mut buffer = [0_u8; 16 * 1024];
        loop {
            let Ok(read) = reader.read(&mut buffer) else {
                truncated = true;
                break;
            };
            if read == 0 {
                break;
            }
            let remaining = MAX_IGNORED_STATUS_OUTPUT_BYTES.saturating_sub(retained.len());
            let keep = remaining.min(read);
            retained.extend_from_slice(&buffer[..keep]);
            if keep < read {
                truncated = true;
            }
        }
        (retained, truncated)
    })
}

/// Observe ignored paths separately from the core tracked/untracked cleanliness contract.
///
/// This boundary intentionally inspects only worktrees that the core audit would otherwise remove.
/// A non-ignored status record means the worktree changed between the core status observation and
/// this guard, so the evidence is rejected rather than reclassifying the drift optimistically.
fn ignored_artifacts_present(path: &Path, timeout_ms: u64) -> Result<bool, String> {
    validate_local_command_timeout(timeout_ms)?;
    let mut command = Command::new("git");
    command
        .args([
            OsString::from("status"),
            OsString::from("--porcelain=v1"),
            OsString::from("-z"),
            OsString::from("--ignored=matching"),
            OsString::from("--untracked-files=all"),
            OsString::from("--ignore-submodules=none"),
        ])
        .current_dir(path)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|_| "git-ignored-status-command-spawn-failed".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "git-ignored-status-stdout-capture-failed".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "git-ignored-status-stderr-capture-failed".to_string())?;
    let stdout_thread = drain_bounded(stdout);
    let stderr_thread = drain_bounded(stderr);
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() >= Duration::from_millis(timeout_ms) => {
                let _ = child.kill();
                let _ = child.wait();
                drop(stdout_thread);
                drop(stderr_thread);
                return Err("git-ignored-status-timeout".into());
            }
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                drop(stdout_thread);
                drop(stderr_thread);
                return Err("git-ignored-status-wait-failed".into());
            }
        }
    };
    let (stdout, stdout_truncated) = stdout_thread
        .join()
        .map_err(|_| "git-ignored-status-stdout-reader-failed".to_string())?;
    let (_stderr, stderr_truncated) = stderr_thread
        .join()
        .map_err(|_| "git-ignored-status-stderr-reader-failed".to_string())?;
    if stdout_truncated || stderr_truncated {
        return Err("git-ignored-status-output-truncated".into());
    }
    if !status.success() {
        return Err("git-ignored-status-command-failed".into());
    }

    let mut ignored = false;
    for field in stdout.split(|byte| *byte == 0).filter(|field| !field.is_empty()) {
        if field.starts_with(b"!! ") {
            ignored = true;
        } else {
            return Err("git-ignored-status-drift".into());
        }
    }
    Ok(ignored)
}

fn hash_field(hasher: &mut blake3::Hasher, value: &str) {
    hasher.update(&(value.len() as u64).to_le_bytes());
    hasher.update(value.as_bytes());
}

fn removal_plan_fingerprint(report: &GitWorktreeAuditReport) -> String {
    let mut candidates: Vec<_> = report
        .entries
        .iter()
        .filter(|entry| entry.disposition == GitWorktreeDisposition::RemovalCandidate)
        .collect();
    candidates.sort_by(|left, right| left.path.cmp(&right.path));
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage.git-worktree-removal-plan\0v1\0");
    hash_field(&mut hasher, &report.common_dir);
    hash_field(&mut hasher, &report.removal_authority_fingerprint);
    hasher.update(&(candidates.len() as u64).to_le_bytes());
    for candidate in candidates {
        hash_field(&mut hasher, &candidate.entry_fingerprint);
        hasher.update(&candidate.size.allocated_bytes.to_le_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

fn apply_ignored_artifact_guard(
    mut report: GitWorktreeAuditReport,
    options: GitWorktreeAuditOptions,
) -> GitWorktreeAuditReport {
    let mut guarded = false;
    let mut evidence_gap = false;
    for entry in &mut report.entries {
        if entry.disposition != GitWorktreeDisposition::RemovalCandidate {
            continue;
        }
        match ignored_artifacts_present(Path::new(&entry.path), options.command_timeout_ms) {
            Ok(false) => {}
            Ok(true) => {
                entry.blockers.push(IGNORED_ARTIFACT_BLOCKER.into());
                entry.disposition = GitWorktreeDisposition::Preserve;
                guarded = true;
            }
            Err(reason) => {
                entry.blockers.push(IGNORED_ARTIFACT_EVIDENCE_GAP.into());
                entry.disposition = GitWorktreeDisposition::EvidenceGap;
                report
                    .issues
                    .push(format!("{}:{reason}", entry.path_fingerprint));
                guarded = true;
                evidence_gap = true;
            }
        }
    }
    if !guarded {
        return report;
    }

    report.removal_candidate_count = report
        .entries
        .iter()
        .filter(|entry| entry.disposition == GitWorktreeDisposition::RemovalCandidate)
        .count();
    report.removal_candidate_allocated_bytes = report
        .entries
        .iter()
        .filter(|entry| entry.disposition == GitWorktreeDisposition::RemovalCandidate)
        .fold(0_u64, |total, entry| total.saturating_add(entry.size.allocated_bytes));
    report.preserved_count = report
        .entries
        .iter()
        .filter(|entry| entry.disposition == GitWorktreeDisposition::Preserve)
        .count();
    report.evidence_gap_count = report
        .entries
        .iter()
        .filter(|entry| entry.disposition == GitWorktreeDisposition::EvidenceGap)
        .count();
    report.evidence_complete = report.issues.is_empty() && report.evidence_gap_count == 0;
    report.removal_plan_fingerprint = removal_plan_fingerprint(&report);

    // Until ignored-artifact evidence is moved into the core per-entry fingerprint, mixed plans are
    // review-only. This prevents the core execution re-audit from observing a different candidate
    // set and guarantees that no approval can bridge the temporary ACL boundary.
    if report.removal_candidate_count > 0 {
        report.exact_approval_phrase = None;
        if !evidence_gap {
            report
                .issues
                .push("ignored-artifact-mixed-plan-review-required".into());
            report.evidence_complete = false;
        }
    } else {
        report.exact_approval_phrase = None;
    }
    report
}

fn ensure_candidates_still_have_no_ignored_artifacts(
    approved_report: &GitWorktreeAuditReport,
    timeout_ms: u64,
) -> Result<(), String> {
    if approved_report.entries.iter().any(|entry| {
        entry
            .blockers
            .iter()
            .any(|blocker| blocker == IGNORED_ARTIFACT_BLOCKER || blocker == IGNORED_ARTIFACT_EVIDENCE_GAP)
    }) {
        return Err("git-worktree-ignored-artifact-guarded-plan-not-executable".into());
    }
    for candidate in approved_report
        .entries
        .iter()
        .filter(|entry| entry.disposition == GitWorktreeDisposition::RemovalCandidate)
    {
        match ignored_artifacts_present(Path::new(&candidate.path), timeout_ms) {
            Ok(false) => {}
            Ok(true) => return Err("git-worktree-ignored-artifact-drift".into()),
            Err(_) => return Err("git-worktree-ignored-artifact-evidence-incomplete".into()),
        }
    }
    Ok(())
}

/// Probe active use only with a bounded local process deadline.
#[cfg(unix)]
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

#[cfg(not(unix))]
pub(crate) fn active_use_evidence(
    path: &Path,
    timeout_ms: u64,
    max_pids: usize,
    recursive: bool,
) -> GitWorktreeActiveUseEvidence {
    if validate_local_command_timeout(timeout_ms).is_err() {
        return GitWorktreeActiveUseEvidence {
            method: if recursive {
                "process-observation-recursive"
            } else {
                "process-observation-file"
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
    .map(|report| apply_ignored_artifact_guard(report, options))
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
    .map(|report| apply_ignored_artifact_guard(report, options))
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
    .map(|report| apply_ignored_artifact_guard(report, options))
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
    .map(|report| apply_ignored_artifact_guard(report, options))
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
    ensure_candidates_still_have_no_ignored_artifacts(approved_report, options.command_timeout_ms)?;
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
    ensure_candidates_still_have_no_ignored_artifacts(approved_report, options.command_timeout_ms)?;
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
    ensure_candidates_still_have_no_ignored_artifacts(approved_report, options.command_timeout_ms)?;
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
