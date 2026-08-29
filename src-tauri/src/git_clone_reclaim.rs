//! Evidence-bound reclamation for a standalone Git clone left on a stale pull-request head.
//!
//! This module never discovers or guesses an age threshold. The operator supplies an explicit
//! cutoff, GitHub resolves the exact same-repository branch and head OID, and DiskSage moves only a
//! clean, inactive, single-worktree clone to the operating-system Trash after a fresh re-audit.

use crate::git_worktree::{
    self, ClosedPullRequestHeads, GitWorktreeActiveUseEvidence, GitWorktreeAuditOptions,
    GitWorktreeAuditReport, GitWorktreeSizeEvidence, StaleOpenPullRequestHeads,
};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};

pub const GIT_CLONE_RECLAIM_SCHEMA_KIND: &str = "disksage.git-clone-reclaim-plan";
pub const GIT_CLONE_RECLAIM_VERSION: u32 = 1;
const MAX_APPROVAL_AGE_MS: u64 = 5 * 60 * 1_000;
const MAX_DEFAULT_BRANCH_EVIDENCE_AGE_MS: u64 = 5 * 60 * 1_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefaultBranchEvidence {
    pub reference: String,
    pub oid: String,
    pub observed_at_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CloneInventoryOptions {
    pub max_depth: usize,
    pub max_entries: usize,
    pub max_clones: usize,
    pub repository_probe_timeout_ms: u64,
}

impl Default for CloneInventoryOptions {
    fn default() -> Self {
        Self {
            max_depth: 4,
            max_entries: 100_000,
            max_clones: 1_000,
            repository_probe_timeout_ms: 10_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CloneInventoryReport {
    pub roots: Vec<String>,
    pub clone_roots: Vec<String>,
    pub visited_entries: usize,
    pub evidence_complete: bool,
    pub issues: Vec<String>,
    pub filesystem_mutation_executed: bool,
}

/// Discover standalone clone roots beneath multiple customer-selected roots without following
/// symlinks or descending into repositories. Limits are evidence boundaries, not age heuristics.
pub fn inventory_standalone_clones(
    roots: &[PathBuf],
    options: CloneInventoryOptions,
) -> Result<CloneInventoryReport, String> {
    if roots.is_empty()
        || roots.len() > 32
        || options.max_depth > 8
        || options.max_entries == 0
        || options.max_entries > 1_000_000
        || options.max_clones == 0
        || options.max_clones > 10_000
        || options.repository_probe_timeout_ms == 0
        || options.repository_probe_timeout_ms > 300_000
    {
        return Err("git-clone-inventory-options-invalid".into());
    }
    let mut queue = VecDeque::new();
    let mut normalized_roots = Vec::new();
    for root in roots {
        if !root.is_absolute() {
            return Err("git-clone-inventory-root-not-absolute".into());
        }
        let metadata = std::fs::symlink_metadata(root)
            .map_err(|_| "git-clone-inventory-root-unavailable".to_string())?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err("git-clone-inventory-root-unsafe".into());
        }
        let canonical = std::fs::canonicalize(root)
            .map_err(|_| "git-clone-inventory-root-unavailable".to_string())?;
        normalized_roots.push(canonical.to_string_lossy().into_owned());
    }
    normalized_roots.sort();
    normalized_roots.dedup();
    let mut unique_roots = normalized_roots
        .iter()
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    unique_roots.sort_by_key(|path| path.components().count());
    let mut bounded_roots = Vec::<PathBuf>::new();
    for root in unique_roots {
        if !bounded_roots.iter().any(|parent| root.starts_with(parent)) {
            bounded_roots.push(root);
        }
    }
    for root in bounded_roots {
        queue.push_back((root, 0usize));
    }
    let mut clone_roots = Vec::new();
    let mut visited_entries = 0usize;
    let mut issues = Vec::new();
    while let Some((directory, depth)) = queue.pop_front() {
        let git_entry = directory.join(".git");
        let has_git_directory = std::fs::symlink_metadata(&git_entry)
            .is_ok_and(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink());
        if has_git_directory {
            match git_worktree::is_standalone_repository_root(
                &directory,
                options.repository_probe_timeout_ms,
            ) {
                Ok(true) => {
                    clone_roots.push(directory.to_string_lossy().into_owned());
                    if clone_roots.len() > options.max_clones {
                        clone_roots.truncate(options.max_clones);
                        issues.push("git-clone-inventory-clone-limit-exceeded".into());
                        break;
                    }
                    continue;
                }
                Ok(false) => {}
                Err(_) => {
                    issues.push("git-clone-inventory-repository-probe-failed".into());
                    continue;
                }
            }
        }
        let entries = match std::fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(_) => {
                issues.push("git-clone-inventory-directory-unreadable".into());
                continue;
            }
        };
        for entry in entries {
            visited_entries = visited_entries.saturating_add(1);
            if visited_entries > options.max_entries {
                issues.push("git-clone-inventory-entry-limit-exceeded".into());
                queue.clear();
                break;
            }
            let Ok(entry) = entry else {
                issues.push("git-clone-inventory-entry-unreadable".into());
                continue;
            };
            let Ok(kind) = entry.file_type() else {
                issues.push("git-clone-inventory-entry-type-unavailable".into());
                continue;
            };
            if kind.is_dir() && !kind.is_symlink() && entry.file_name() != ".git" {
                if depth >= options.max_depth {
                    issues.push("git-clone-inventory-depth-limit-exceeded".into());
                } else {
                    queue.push_back((entry.path(), depth + 1));
                }
            }
        }
    }
    clone_roots.sort();
    clone_roots.dedup();
    issues.sort();
    issues.dedup();
    Ok(CloneInventoryReport {
        roots: normalized_roots,
        clone_roots,
        visited_entries,
        evidence_complete: issues.is_empty(),
        issues,
        filesystem_mutation_executed: false,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitCloneReclaimPlan {
    pub schema_kind: String,
    pub version: u32,
    pub generated_at_ms: u64,
    pub repository_root: String,
    pub repository_object_id: String,
    pub head: String,
    pub branch: String,
    pub closed_pull_request_head: bool,
    pub stale_open_pull_request_head: bool,
    pub stale_open_pull_request_cutoff_ms: Option<u64>,
    #[serde(default)]
    pub default_branch_reference: Option<String>,
    #[serde(default)]
    pub default_branch_oid: Option<String>,
    #[serde(default)]
    pub default_branch_observed_at_ms: Option<u64>,
    #[serde(default)]
    pub head_is_default_branch_ancestor: bool,
    pub size: GitWorktreeSizeEvidence,
    pub active_use: GitWorktreeActiveUseEvidence,
    pub authority_fingerprint: String,
    pub plan_fingerprint: String,
    pub exact_approval_phrase: Option<String>,
    pub eligible_after_human_approval: bool,
    pub blockers: Vec<String>,
    pub filesystem_mutation_executed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitCloneReclaimApproval {
    pub version: u32,
    pub approval_id: String,
    pub plan_fingerprint: String,
    pub exact_approval_phrase: String,
    pub approved_at_ms: u64,
    pub approved_by: String,
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitCloneReclaimResult {
    pub version: u32,
    pub approval_id: String,
    pub plan_fingerprint: String,
    pub requested_at_ms: u64,
    pub completed_at_ms: u64,
    pub allocated_bytes_upper_bound: u64,
    pub trash_move_executed: bool,
    pub path_absence_verified: bool,
    pub branch_delete_command_executed: bool,
    pub git_prune_executed: bool,
    pub physically_reclaimed_bytes: Option<u64>,
}

fn hash_field(hasher: &mut blake3::Hasher, value: &str) {
    hasher.update(&(value.len() as u64).to_le_bytes());
    hasher.update(value.as_bytes());
}

fn valid_human_text(value: &str, max_chars: usize) -> bool {
    !value.trim().is_empty()
        && value == value.trim()
        && value.chars().count() <= max_chars
        && !value.chars().any(char::is_control)
}

fn plan_fingerprint(
    report: &GitWorktreeAuditReport,
    repository_object_id: &str,
    head: &str,
    branch: &str,
    size: &GitWorktreeSizeEvidence,
    default_branch_evidence: Option<&DefaultBranchEvidence>,
    head_is_default_branch_ancestor: bool,
    blockers: &[String],
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage.git-clone-reclaim-plan\0v1\0");
    for value in [
        &report.repository_root,
        &report.common_dir,
        &report.removal_authority_fingerprint,
        repository_object_id,
        head,
        branch,
        &size.allocated_bytes.to_string(),
        &size.logical_bytes.to_string(),
    ] {
        hash_field(&mut hasher, value);
    }
    if let Some(evidence) = default_branch_evidence {
        hash_field(&mut hasher, &evidence.reference);
        hash_field(&mut hasher, &evidence.oid);
    }
    hash_field(
        &mut hasher,
        if head_is_default_branch_ancestor {
            "1"
        } else {
            "0"
        },
    );
    for blocker in blockers {
        hash_field(&mut hasher, blocker);
    }
    hasher.finalize().to_hex().to_string()
}

fn approval_id(plan: &GitCloneReclaimPlan, approved_at_ms: u64, approved_by: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage.git-clone-reclaim-approval\0v1\0");
    hash_field(&mut hasher, &plan.plan_fingerprint);
    hash_field(&mut hasher, &approved_at_ms.to_string());
    hash_field(&mut hasher, approved_by);
    hasher.finalize().to_hex().to_string()
}

/// Return whether the requested root is a regular standalone clone rather than a linked
/// worktree or a repository whose administrative directory is redirected through a symlink.
///
/// `git-worktree list` identifies the checkout, while this check binds the administrative
/// directory to the canonical root. Keeping both observations is important: a linked worktree
/// may look like an ordinary checkout at its own path, and a symlinked `.git` can change what a
/// later Trash operation affects without changing the displayed repository path.
fn has_bounded_standalone_git_directory(repository_root: &Path, common_dir: &Path) -> bool {
    let git_entry = repository_root.join(".git");
    let Ok(metadata) = std::fs::symlink_metadata(&git_entry) else {
        return false;
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return false;
    }
    common_dir
        .parent()
        .is_some_and(|parent| parent == repository_root)
        && std::fs::canonicalize(git_entry).ok().as_deref() == Some(common_dir)
}

/// Validate the append-only journal destination before the source clone can be moved.
///
/// The journal is part of the rollback contract. It must live outside the clone being moved,
/// have a real private parent, and be either absent or a regular file. This prevents an
/// application-data misconfiguration from moving the journal into Trash together with its source
/// or from appending through a symlink.
fn validate_journal_destination(repository_root: &Path, journal_path: &Path) -> Result<(), String> {
    if !journal_path.is_absolute()
        || journal_path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("git-clone-journal-path-invalid".into());
    }
    let parent = journal_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .ok_or_else(|| "git-clone-journal-parent-invalid".to_string())?;
    let parent_metadata = std::fs::symlink_metadata(parent)
        .map_err(|_| "git-clone-journal-parent-unavailable".to_string())?;
    if !parent_metadata.is_dir() || parent_metadata.file_type().is_symlink() {
        return Err("git-clone-journal-parent-unsafe".into());
    }
    let canonical_source = std::fs::canonicalize(repository_root)
        .map_err(|_| "git-clone-source-root-unavailable".to_string())?;
    let canonical_parent = std::fs::canonicalize(parent)
        .map_err(|_| "git-clone-journal-parent-unavailable".to_string())?;
    if canonical_parent.starts_with(&canonical_source) {
        return Err("git-clone-journal-inside-source".into());
    }
    match std::fs::symlink_metadata(journal_path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                Err("git-clone-journal-file-unsafe".into())
            } else {
                Ok(())
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("git-clone-journal-file-unavailable".into()),
    }
}

/// Build a plan from already-resolved GitHub PR heads. This is also the deterministic test seam.
pub fn plan_git_clone_reclaim_with_authority(
    repository_root: &Path,
    retention_references: &[String],
    closed_pull_request_heads: &ClosedPullRequestHeads,
    stale_open_pull_request_heads: &StaleOpenPullRequestHeads,
    stale_open_pull_request_cutoff_ms: Option<u64>,
    default_branch_evidence: Option<&DefaultBranchEvidence>,
    options: GitWorktreeAuditOptions,
    generated_at_ms: u64,
) -> Result<GitCloneReclaimPlan, String> {
    let report = git_worktree::audit_git_worktrees_with_pull_request_heads(
        repository_root,
        retention_references,
        closed_pull_request_heads,
        stale_open_pull_request_heads,
        stale_open_pull_request_cutoff_ms,
        options,
        generated_at_ms,
    )?;
    let primary = report
        .entries
        .iter()
        .find(|entry| entry.primary)
        .ok_or_else(|| "git-clone-primary-worktree-missing".to_string())?;
    let repository_path = PathBuf::from(&report.repository_root);
    let common_dir = PathBuf::from(&report.common_dir);
    let branch = primary.branch.clone().unwrap_or_default();
    let active_use = git_worktree::active_use_evidence(
        &repository_path,
        options.command_timeout_ms,
        options.max_active_pids,
        true,
    );
    let repository_object_id = crate::safety::filesystem_object_id(&repository_path)
        .map_err(|_| "git-clone-object-identity-unavailable".to_string())?;
    let default_evidence_fresh = default_branch_evidence.is_some_and(|evidence| {
        evidence.observed_at_ms <= generated_at_ms
            && generated_at_ms.saturating_sub(evidence.observed_at_ms)
                <= MAX_DEFAULT_BRANCH_EVIDENCE_AGE_MS
    });
    let mut default_branch_probe_incomplete = false;
    let head_is_default_branch_ancestor = if default_evidence_fresh {
        let evidence = default_branch_evidence.expect("checked Some above");
        match git_worktree::exact_reference_contains_commit(
            &repository_path,
            &evidence.reference,
            &evidence.oid,
            &primary.head,
            options.command_timeout_ms,
        ) {
            Ok(value) => value,
            Err(_) => {
                default_branch_probe_incomplete = true;
                false
            }
        }
    } else {
        false
    };
    let mut blockers = Vec::new();
    if !report.evidence_complete {
        blockers.push("git-clone-audit-evidence-incomplete".into());
    }
    if report.worktree_count != 1 {
        blockers.push("git-clone-linked-worktrees-present".into());
    }
    if primary.bare || primary.detached || !primary.audit_origin {
        blockers.push("git-clone-primary-shape-unsupported".into());
    }
    if branch.is_empty() {
        blockers.push("git-clone-branch-missing".into());
    }
    if primary.status_clean != Some(true) {
        blockers.push("git-clone-working-tree-not-clean".into());
    }
    if !primary.closed_pull_request_head
        && !primary.stale_open_pull_request_head
        && !head_is_default_branch_ancestor
    {
        blockers.push("git-clone-pr-head-authority-missing".into());
    }
    if default_branch_evidence.is_some() && !default_evidence_fresh {
        blockers.push("git-clone-default-branch-evidence-stale".into());
    }
    if default_branch_probe_incomplete {
        blockers.push("git-clone-default-branch-ancestry-evidence-incomplete".into());
    }
    if primary.head_is_retained_tip {
        blockers.push("git-clone-head-is-retained-tip".into());
    }
    if primary.actor_cwd_inside != Some(false) {
        blockers.push("git-clone-actor-cwd-evidence-incomplete-or-active".into());
    }
    if !primary.size.evidence_complete {
        blockers.push("git-clone-size-evidence-incomplete".into());
    }
    if !active_use.evidence_complete {
        blockers.push("git-clone-active-use-evidence-incomplete".into());
    } else if active_use.active {
        blockers.push("git-clone-active-use-detected".into());
    }
    if !has_bounded_standalone_git_directory(&repository_path, &common_dir) {
        blockers.push("git-clone-git-directory-not-real-or-bounded".into());
    }
    if crate::safety::is_protected(&repository_path) {
        blockers.push("git-clone-path-protected".into());
    }
    blockers.sort();
    blockers.dedup();
    let fingerprint = plan_fingerprint(
        &report,
        &repository_object_id,
        &primary.head,
        &branch,
        &primary.size,
        default_branch_evidence,
        head_is_default_branch_ancestor,
        &blockers,
    );
    let eligible = blockers.is_empty();
    Ok(GitCloneReclaimPlan {
        schema_kind: GIT_CLONE_RECLAIM_SCHEMA_KIND.into(),
        version: GIT_CLONE_RECLAIM_VERSION,
        generated_at_ms,
        repository_root: report.repository_root,
        repository_object_id,
        head: primary.head.clone(),
        branch,
        closed_pull_request_head: primary.closed_pull_request_head,
        stale_open_pull_request_head: primary.stale_open_pull_request_head,
        stale_open_pull_request_cutoff_ms,
        default_branch_reference: default_branch_evidence.map(|value| value.reference.clone()),
        default_branch_oid: default_branch_evidence.map(|value| value.oid.clone()),
        default_branch_observed_at_ms: default_branch_evidence.map(|value| value.observed_at_ms),
        head_is_default_branch_ancestor,
        size: primary.size.clone(),
        active_use,
        authority_fingerprint: report.removal_authority_fingerprint,
        exact_approval_phrase: eligible.then(|| {
            format!(
                "DiskSage stale clone 1 {} 승인 {fingerprint}",
                primary.size.allocated_bytes
            )
        }),
        plan_fingerprint: fingerprint,
        eligible_after_human_approval: eligible,
        blockers,
        filesystem_mutation_executed: false,
    })
}

/// Build a plan from exact pull-request evidence without default-branch ancestry authority.
pub fn plan_git_clone_reclaim_with_pull_request_heads(
    repository_root: &Path,
    retention_references: &[String],
    closed_pull_request_heads: &ClosedPullRequestHeads,
    stale_open_pull_request_heads: &StaleOpenPullRequestHeads,
    stale_open_pull_request_cutoff_ms: Option<u64>,
    options: GitWorktreeAuditOptions,
    generated_at_ms: u64,
) -> Result<GitCloneReclaimPlan, String> {
    plan_git_clone_reclaim_with_authority(
        repository_root,
        retention_references,
        closed_pull_request_heads,
        stale_open_pull_request_heads,
        stale_open_pull_request_cutoff_ms,
        None,
        options,
        generated_at_ms,
    )
}

/// Resolve current GitHub evidence and build a read-only standalone-clone reclaim plan.
pub fn plan_git_clone_reclaim(
    repository_root: &Path,
    retention_references: &[String],
    include_closed_pull_requests: bool,
    stale_open_pull_request_cutoff_ms: Option<u64>,
    options: GitWorktreeAuditOptions,
    generated_at_ms: u64,
) -> Result<GitCloneReclaimPlan, String> {
    let closed = if include_closed_pull_requests {
        git_worktree::github_closed_pull_request_heads_with_options(repository_root, options)?
    } else {
        ClosedPullRequestHeads::new()
    };
    let stale_open = if let Some(cutoff_ms) = stale_open_pull_request_cutoff_ms {
        git_worktree::github_stale_open_pull_request_heads(
            repository_root,
            cutoff_ms,
            options.command_timeout_ms,
        )?
    } else {
        StaleOpenPullRequestHeads::new()
    };
    plan_git_clone_reclaim_with_pull_request_heads(
        repository_root,
        retention_references,
        &closed,
        &stale_open,
        stale_open_pull_request_cutoff_ms,
        options,
        generated_at_ms,
    )
}

/// Resolve fresh GitHub PR evidence first; query default-branch ancestry only when PR evidence does
/// not already authorize the primary clone. Provider failure is therefore isolated from an exact
/// closed/stale-open PR authority path while ancestry-only candidates remain fail-closed.
pub fn plan_git_clone_reclaim_with_default_branch(
    repository_root: &Path,
    retention_references: &[String],
    include_closed_pull_requests: bool,
    stale_open_pull_request_cutoff_ms: Option<u64>,
    options: GitWorktreeAuditOptions,
    generated_at_ms: u64,
) -> Result<GitCloneReclaimPlan, String> {
    let closed = if include_closed_pull_requests {
        git_worktree::github_closed_pull_request_heads_with_options(repository_root, options)?
    } else {
        ClosedPullRequestHeads::new()
    };
    let stale_open = if let Some(cutoff_ms) = stale_open_pull_request_cutoff_ms {
        git_worktree::github_stale_open_pull_request_heads(
            repository_root,
            cutoff_ms,
            options.command_timeout_ms,
        )?
    } else {
        StaleOpenPullRequestHeads::new()
    };
    let pr_plan = plan_git_clone_reclaim_with_authority(
        repository_root,
        retention_references,
        &closed,
        &stale_open,
        stale_open_pull_request_cutoff_ms,
        None,
        options,
        generated_at_ms,
    )?;
    if pr_plan.closed_pull_request_head || pr_plan.stale_open_pull_request_head {
        return Ok(pr_plan);
    }
    let (reference, oid) = git_worktree::github_default_branch_reference_oid(
        repository_root,
        options.command_timeout_ms,
    )?;
    let evidence = DefaultBranchEvidence {
        reference,
        oid,
        observed_at_ms: generated_at_ms,
    };
    plan_git_clone_reclaim_with_authority(
        repository_root,
        retention_references,
        &closed,
        &stale_open,
        stale_open_pull_request_cutoff_ms,
        Some(&evidence),
        options,
        generated_at_ms,
    )
}

pub fn approve_git_clone_reclaim(
    plan: &GitCloneReclaimPlan,
    exact_approval_phrase: &str,
    approved_at_ms: u64,
    approved_by: &str,
    rationale: &str,
) -> Result<GitCloneReclaimApproval, String> {
    if !plan.eligible_after_human_approval
        || plan.exact_approval_phrase.as_deref() != Some(exact_approval_phrase)
        || approved_at_ms < plan.generated_at_ms
    {
        return Err("git-clone-approval-plan-mismatch".into());
    }
    if !valid_human_text(approved_by, 256) || !valid_human_text(rationale, 1_000) {
        return Err("git-clone-approval-text-invalid".into());
    }
    Ok(GitCloneReclaimApproval {
        version: GIT_CLONE_RECLAIM_VERSION,
        approval_id: approval_id(plan, approved_at_ms, approved_by),
        plan_fingerprint: plan.plan_fingerprint.clone(),
        exact_approval_phrase: exact_approval_phrase.into(),
        approved_at_ms,
        approved_by: approved_by.into(),
        rationale: rationale.into(),
    })
}

/// Re-resolve GitHub and filesystem evidence, then move the exact clone object to OS Trash.
fn execute_git_clone_reclaim_with_authority(
    approved_plan: &GitCloneReclaimPlan,
    approval: &GitCloneReclaimApproval,
    retention_references: &[String],
    include_closed_pull_requests: bool,
    stale_open_pull_request_cutoff_ms: Option<u64>,
    default_branch_evidence: Option<&DefaultBranchEvidence>,
    options: GitWorktreeAuditOptions,
    journal_path: &Path,
    requested_at_ms: u64,
) -> Result<GitCloneReclaimResult, String> {
    if approval.version != GIT_CLONE_RECLAIM_VERSION
        || approval.plan_fingerprint != approved_plan.plan_fingerprint
        || approved_plan.exact_approval_phrase.as_deref()
            != Some(approval.exact_approval_phrase.as_str())
        || requested_at_ms < approval.approved_at_ms
        || requested_at_ms.saturating_sub(approval.approved_at_ms) > MAX_APPROVAL_AGE_MS
    {
        return Err("git-clone-execution-approval-invalid-or-stale".into());
    }
    validate_journal_destination(Path::new(&approved_plan.repository_root), journal_path)?;
    let repository_root = Path::new(&approved_plan.repository_root);
    let closed = if include_closed_pull_requests {
        git_worktree::github_closed_pull_request_heads_with_options(repository_root, options)?
    } else {
        ClosedPullRequestHeads::new()
    };
    let stale_open = if let Some(cutoff_ms) = stale_open_pull_request_cutoff_ms {
        git_worktree::github_stale_open_pull_request_heads(
            repository_root,
            cutoff_ms,
            options.command_timeout_ms,
        )?
    } else {
        StaleOpenPullRequestHeads::new()
    };
    let live = plan_git_clone_reclaim_with_authority(
        repository_root,
        retention_references,
        &closed,
        &stale_open,
        stale_open_pull_request_cutoff_ms,
        default_branch_evidence,
        options,
        requested_at_ms,
    )?;
    if live.plan_fingerprint != approved_plan.plan_fingerprint
        || live.repository_object_id != approved_plan.repository_object_id
        || !live.eligible_after_human_approval
    {
        return Err("git-clone-live-plan-mismatch".into());
    }
    crate::safety::trash_delete_if_identity(
        Path::new(&live.repository_root),
        &live.repository_object_id,
        live.size.allocated_bytes,
        journal_path,
        requested_at_ms,
    )
    .map_err(|error| format!("git-clone-trash-failed:{error}"))?;
    let path_absence_verified = matches!(
        std::fs::symlink_metadata(&live.repository_root),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound
    );
    if !path_absence_verified {
        return Err("git-clone-trash-path-still-present".into());
    }
    Ok(GitCloneReclaimResult {
        version: GIT_CLONE_RECLAIM_VERSION,
        approval_id: approval.approval_id.clone(),
        plan_fingerprint: live.plan_fingerprint,
        requested_at_ms,
        completed_at_ms: crate::cloud::system_now_ms(),
        allocated_bytes_upper_bound: live.size.allocated_bytes,
        trash_move_executed: true,
        path_absence_verified,
        branch_delete_command_executed: false,
        git_prune_executed: false,
        physically_reclaimed_bytes: None,
    })
}

/// Revalidate PR authority and execute the existing reversible clone cleanup path.
pub fn execute_git_clone_reclaim(
    approved_plan: &GitCloneReclaimPlan,
    approval: &GitCloneReclaimApproval,
    retention_references: &[String],
    include_closed_pull_requests: bool,
    stale_open_pull_request_cutoff_ms: Option<u64>,
    options: GitWorktreeAuditOptions,
    journal_path: &Path,
    requested_at_ms: u64,
) -> Result<GitCloneReclaimResult, String> {
    execute_git_clone_reclaim_with_authority(
        approved_plan,
        approval,
        retention_references,
        include_closed_pull_requests,
        stale_open_pull_request_cutoff_ms,
        None,
        options,
        journal_path,
        requested_at_ms,
    )
}

/// Revalidate whichever authority path the approved plan actually bound. PR-authorized plans omit
/// default-branch fields and re-query only PR evidence; ancestry-authorized plans must reproduce
/// the exact provider reference/OID before execution.
pub fn execute_git_clone_reclaim_with_default_branch(
    approved_plan: &GitCloneReclaimPlan,
    approval: &GitCloneReclaimApproval,
    retention_references: &[String],
    include_closed_pull_requests: bool,
    stale_open_pull_request_cutoff_ms: Option<u64>,
    options: GitWorktreeAuditOptions,
    journal_path: &Path,
    requested_at_ms: u64,
) -> Result<GitCloneReclaimResult, String> {
    match (
        approved_plan.default_branch_reference.as_deref(),
        approved_plan.default_branch_oid.as_deref(),
        approved_plan.default_branch_observed_at_ms,
    ) {
        (None, None, None)
            if approved_plan.closed_pull_request_head
                || approved_plan.stale_open_pull_request_head =>
        {
            execute_git_clone_reclaim_with_authority(
                approved_plan,
                approval,
                retention_references,
                include_closed_pull_requests,
                stale_open_pull_request_cutoff_ms,
                None,
                options,
                journal_path,
                requested_at_ms,
            )
        }
        (Some(expected_reference), Some(expected_oid), Some(observed_at_ms)) => {
            let (reference, oid) = git_worktree::github_default_branch_reference_oid(
                Path::new(&approved_plan.repository_root),
                options.command_timeout_ms,
            )?;
            if reference != expected_reference || oid != expected_oid {
                return Err("git-clone-default-branch-provider-drift".into());
            }
            let evidence = DefaultBranchEvidence {
                reference,
                oid,
                observed_at_ms,
            };
            execute_git_clone_reclaim_with_authority(
                approved_plan,
                approval,
                retention_references,
                include_closed_pull_requests,
                stale_open_pull_request_cutoff_ms,
                Some(&evidence),
                options,
                journal_path,
                requested_at_ms,
            )
        }
        _ => Err("git-clone-default-branch-evidence-missing".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn git(repository: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .args(args)
            .current_dir(repository)
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().into()
    }

    #[cfg(unix)]
    #[test]
    fn fresh_exact_default_branch_ancestor_authorizes_clean_pushed_clone() {
        let repository = tempfile::tempdir().unwrap();
        git(repository.path(), &["init", "-b", "main"]);
        git(
            repository.path(),
            &["config", "user.email", "clone@example.invalid"],
        );
        git(
            repository.path(),
            &["config", "user.name", "DiskSage Clone Test"],
        );
        std::fs::write(repository.path().join("tracked.txt"), b"base\n").unwrap();
        git(repository.path(), &["add", "tracked.txt"]);
        git(repository.path(), &["commit", "-m", "base"]);
        let ancestor = git(repository.path(), &["rev-parse", "HEAD"]);
        git(repository.path(), &["switch", "-c", "default-next"]);
        std::fs::write(repository.path().join("tracked.txt"), b"default next\n").unwrap();
        git(repository.path(), &["commit", "-am", "default next"]);
        let default_oid = git(repository.path(), &["rev-parse", "HEAD"]);
        git(
            repository.path(),
            &["update-ref", "refs/remotes/origin/main", &default_oid],
        );
        git(repository.path(), &["switch", "--detach", &ancestor]);
        git(repository.path(), &["switch", "-c", "completed-local"]);
        let evidence = DefaultBranchEvidence {
            reference: "refs/remotes/origin/main".into(),
            oid: default_oid,
            observed_at_ms: 10,
        };
        let plan = plan_git_clone_reclaim_with_authority(
            repository.path(),
            &["refs/remotes/origin/main".into()],
            &ClosedPullRequestHeads::new(),
            &StaleOpenPullRequestHeads::new(),
            None,
            Some(&evidence),
            GitWorktreeAuditOptions::default(),
            11,
        )
        .unwrap();
        assert!(plan.eligible_after_human_approval, "{:?}", plan.blockers);
        assert!(plan.head_is_default_branch_ancestor);
        let refreshed_evidence = DefaultBranchEvidence {
            observed_at_ms: 12,
            ..evidence
        };
        let refreshed_plan = plan_git_clone_reclaim_with_authority(
            repository.path(),
            &["refs/remotes/origin/main".into()],
            &ClosedPullRequestHeads::new(),
            &StaleOpenPullRequestHeads::new(),
            None,
            Some(&refreshed_evidence),
            GitWorktreeAuditOptions::default(),
            13,
        )
        .unwrap();
        assert_eq!(plan.plan_fingerprint, refreshed_plan.plan_fingerprint);
    }

    #[cfg(unix)]
    #[test]
    fn unpushed_diverged_head_never_receives_default_branch_authority() {
        let repository = tempfile::tempdir().unwrap();
        git(repository.path(), &["init", "-b", "main"]);
        git(
            repository.path(),
            &["config", "user.email", "clone@example.invalid"],
        );
        git(
            repository.path(),
            &["config", "user.name", "DiskSage Clone Test"],
        );
        std::fs::write(repository.path().join("tracked.txt"), b"main\n").unwrap();
        git(repository.path(), &["add", "tracked.txt"]);
        git(repository.path(), &["commit", "-m", "main"]);
        let default_oid = git(repository.path(), &["rev-parse", "HEAD"]);
        git(
            repository.path(),
            &["update-ref", "refs/remotes/origin/main", &default_oid],
        );
        git(repository.path(), &["switch", "-c", "unpushed"]);
        std::fs::write(repository.path().join("tracked.txt"), b"unpushed\n").unwrap();
        git(repository.path(), &["commit", "-am", "unpushed"]);
        let evidence = DefaultBranchEvidence {
            reference: "refs/remotes/origin/main".into(),
            oid: default_oid,
            observed_at_ms: 10,
        };
        let plan = plan_git_clone_reclaim_with_authority(
            repository.path(),
            &["refs/remotes/origin/main".into()],
            &ClosedPullRequestHeads::new(),
            &StaleOpenPullRequestHeads::new(),
            None,
            Some(&evidence),
            GitWorktreeAuditOptions::default(),
            11,
        )
        .unwrap();
        assert!(!plan.eligible_after_human_approval);
        assert!(!plan.head_is_default_branch_ancestor);
        assert!(plan
            .blockers
            .contains(&"git-clone-pr-head-authority-missing".into()));
        assert!(!plan
            .blockers
            .contains(&"git-clone-default-branch-evidence-stale".into()));
    }

    #[cfg(unix)]
    #[test]
    fn active_clone_never_receives_default_branch_authority() {
        let repository = tempfile::tempdir().unwrap();
        git(repository.path(), &["init", "-b", "main"]);
        git(
            repository.path(),
            &["config", "user.email", "clone@example.invalid"],
        );
        git(
            repository.path(),
            &["config", "user.name", "DiskSage Clone Test"],
        );
        std::fs::write(repository.path().join("tracked.txt"), b"main\n").unwrap();
        git(repository.path(), &["add", "tracked.txt"]);
        git(repository.path(), &["commit", "-m", "main"]);
        let head = git(repository.path(), &["rev-parse", "HEAD"]);
        git(
            repository.path(),
            &["update-ref", "refs/remotes/origin/main", &head],
        );
        let evidence = DefaultBranchEvidence {
            reference: "refs/remotes/origin/main".into(),
            oid: head,
            observed_at_ms: 10,
        };
        let mut child = Command::new("sh")
            .args(["-c", "cd -- \"$1\" && exec sleep 5", "disksage-active"])
            .arg(repository.path())
            .spawn()
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(100));
        let plan = plan_git_clone_reclaim_with_authority(
            repository.path(),
            &["refs/remotes/origin/main".into()],
            &ClosedPullRequestHeads::new(),
            &StaleOpenPullRequestHeads::new(),
            None,
            Some(&evidence),
            GitWorktreeAuditOptions::default(),
            11,
        )
        .unwrap();
        let _ = child.kill();
        let _ = child.wait();
        assert!(!plan.eligible_after_human_approval);
        assert!(plan
            .blockers
            .contains(&"git-clone-active-use-detected".into()));
    }

    #[test]
    fn multi_root_inventory_is_bounded_and_does_not_follow_symlinks() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let first_repo = first.path().join("nested/repo");
        let second_repo = second.path().join("repo");
        std::fs::create_dir_all(&first_repo).unwrap();
        std::fs::create_dir_all(&second_repo).unwrap();
        git(&first_repo, &["init", "-b", "main"]);
        git(&second_repo, &["init", "-b", "main"]);
        std::fs::create_dir_all(second.path().join("not-a-repo/.git")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(
            first.path().join("nested"),
            second.path().join("linked-outside"),
        )
        .unwrap();
        let report = inventory_standalone_clones(
            &[first.path().to_path_buf(), second.path().to_path_buf()],
            CloneInventoryOptions::default(),
        )
        .unwrap();
        assert!(report.evidence_complete, "{:?}", report.issues);
        assert_eq!(report.clone_roots.len(), 2);
        assert!(!report
            .clone_roots
            .iter()
            .any(|path| path.ends_with("not-a-repo")));
        assert!(!report.filesystem_mutation_executed);
    }

    #[test]
    fn depth_truncation_is_reported_as_incomplete_evidence() {
        let root = tempfile::tempdir().unwrap();
        let hidden = root.path().join("one/two/repo");
        std::fs::create_dir_all(&hidden).unwrap();
        git(&hidden, &["init", "-b", "main"]);
        let report = inventory_standalone_clones(
            &[root.path().to_path_buf()],
            CloneInventoryOptions {
                max_depth: 1,
                ..CloneInventoryOptions::default()
            },
        )
        .unwrap();
        assert!(!report.evidence_complete);
        assert!(report
            .issues
            .contains(&"git-clone-inventory-depth-limit-exceeded".into()));
        assert!(report.clone_roots.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn exact_closed_pr_head_authorizes_only_clean_inactive_single_clone() {
        let repository = tempfile::tempdir().unwrap();
        git(repository.path(), &["init", "-b", "main"]);
        git(
            repository.path(),
            &["config", "user.email", "clone@example.invalid"],
        );
        git(
            repository.path(),
            &["config", "user.name", "DiskSage Clone Test"],
        );
        std::fs::write(repository.path().join("tracked.txt"), b"main\n").unwrap();
        git(repository.path(), &["add", "tracked.txt"]);
        git(repository.path(), &["commit", "-m", "main"]);
        git(repository.path(), &["switch", "-c", "old-pr"]);
        std::fs::write(repository.path().join("tracked.txt"), b"old pr\n").unwrap();
        git(repository.path(), &["commit", "-am", "old pr"]);
        let head = git(repository.path(), &["rev-parse", "HEAD"]);
        let closed = ClosedPullRequestHeads::from([("refs/heads/old-pr".into(), head)]);

        let plan = plan_git_clone_reclaim_with_pull_request_heads(
            repository.path(),
            &["refs/heads/main".into()],
            &closed,
            &StaleOpenPullRequestHeads::new(),
            None,
            GitWorktreeAuditOptions::default(),
            10,
        )
        .unwrap();

        assert!(plan.eligible_after_human_approval, "{:?}", plan.blockers);
        assert!(plan.closed_pull_request_head);
        assert!(!plan.stale_open_pull_request_head);
        assert!(plan.exact_approval_phrase.is_some());
        assert!(!plan.filesystem_mutation_executed);
        assert!(repository.path().exists());
    }

    #[cfg(unix)]
    #[test]
    fn dirty_clone_never_receives_approval_authority() {
        let repository = tempfile::tempdir().unwrap();
        git(repository.path(), &["init", "-b", "main"]);
        git(
            repository.path(),
            &["config", "user.email", "clone@example.invalid"],
        );
        git(
            repository.path(),
            &["config", "user.name", "DiskSage Clone Test"],
        );
        std::fs::write(repository.path().join("tracked.txt"), b"main\n").unwrap();
        git(repository.path(), &["add", "tracked.txt"]);
        git(repository.path(), &["commit", "-m", "main"]);
        git(repository.path(), &["switch", "-c", "old-pr"]);
        std::fs::write(repository.path().join("tracked.txt"), b"old pr\n").unwrap();
        git(repository.path(), &["commit", "-am", "old pr"]);
        let head = git(repository.path(), &["rev-parse", "HEAD"]);
        std::fs::write(repository.path().join("untracked.txt"), b"keep me\n").unwrap();
        let closed = ClosedPullRequestHeads::from([("refs/heads/old-pr".into(), head)]);

        let plan = plan_git_clone_reclaim_with_pull_request_heads(
            repository.path(),
            &["refs/heads/main".into()],
            &closed,
            &StaleOpenPullRequestHeads::new(),
            None,
            GitWorktreeAuditOptions::default(),
            10,
        )
        .unwrap();

        assert!(!plan.eligible_after_human_approval);
        assert!(plan
            .blockers
            .contains(&"git-clone-working-tree-not-clean".into()));
        assert!(approve_git_clone_reclaim(&plan, "wrong", 11, "human:test", "reviewed").is_err());
        assert!(repository.path().join("untracked.txt").exists());
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_git_directory_is_not_a_standalone_clone() {
        let repository = tempfile::tempdir().unwrap();
        git(repository.path(), &["init", "-b", "main"]);
        git(
            repository.path(),
            &["config", "user.email", "clone@example.invalid"],
        );
        git(
            repository.path(),
            &["config", "user.name", "DiskSage Clone Test"],
        );
        std::fs::write(repository.path().join("tracked.txt"), b"main\n").unwrap();
        git(repository.path(), &["add", "tracked.txt"]);
        git(repository.path(), &["commit", "-m", "main"]);
        let head = git(repository.path(), &["rev-parse", "HEAD"]);
        let git_directory = repository.path().join(".git");
        let real_git_directory = repository.path().join(".git-real");
        std::fs::rename(&git_directory, &real_git_directory).unwrap();
        std::os::unix::fs::symlink(".git-real", &git_directory).unwrap();

        let closed = ClosedPullRequestHeads::from([("refs/heads/main".into(), head)]);
        let plan = plan_git_clone_reclaim_with_pull_request_heads(
            repository.path(),
            &["refs/heads/main".into()],
            &closed,
            &StaleOpenPullRequestHeads::new(),
            None,
            GitWorktreeAuditOptions::default(),
            10,
        )
        .unwrap();

        assert!(!plan.eligible_after_human_approval);
        assert!(plan
            .blockers
            .contains(&"git-clone-git-directory-not-real-or-bounded".into()));
    }

    #[test]
    fn journal_destination_must_be_outside_the_clone() {
        let repository = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let inside = repository.path().join("journal.jsonl");
        assert_eq!(
            validate_journal_destination(repository.path(), &inside).unwrap_err(),
            "git-clone-journal-inside-source"
        );

        let relative = Path::new("journal.jsonl");
        assert_eq!(
            validate_journal_destination(repository.path(), relative).unwrap_err(),
            "git-clone-journal-path-invalid"
        );

        let journal = outside.path().join("journal.jsonl");
        assert!(validate_journal_destination(repository.path(), &journal).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn stale_open_authority_requires_an_explicit_cutoff() {
        let repository = tempfile::tempdir().unwrap();
        git(repository.path(), &["init", "-b", "main"]);
        git(
            repository.path(),
            &["config", "user.email", "clone@example.invalid"],
        );
        git(
            repository.path(),
            &["config", "user.name", "DiskSage Clone Test"],
        );
        std::fs::write(repository.path().join("tracked.txt"), b"main\n").unwrap();
        git(repository.path(), &["add", "tracked.txt"]);
        git(repository.path(), &["commit", "-m", "main"]);
        git(repository.path(), &["switch", "-c", "open-pr"]);
        std::fs::write(repository.path().join("tracked.txt"), b"open\n").unwrap();
        git(repository.path(), &["commit", "-am", "open"]);
        let head = git(repository.path(), &["rev-parse", "HEAD"]);
        let stale = StaleOpenPullRequestHeads::from([(
            ("refs/heads/open-pr".into(), head),
            std::collections::BTreeSet::from([1]),
        )]);

        let error = plan_git_clone_reclaim_with_pull_request_heads(
            repository.path(),
            &["refs/heads/main".into()],
            &ClosedPullRequestHeads::new(),
            &stale,
            None,
            GitWorktreeAuditOptions::default(),
            10,
        )
        .unwrap_err();
        assert_eq!(error, "git-worktree-stale-open-pull-request-heads-invalid");
    }
}
