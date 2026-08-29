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
use std::path::{Path, PathBuf};

pub const GIT_CLONE_RECLAIM_SCHEMA_KIND: &str = "disksage.git-clone-reclaim-plan";
pub const GIT_CLONE_RECLAIM_VERSION: u32 = 1;
const MAX_APPROVAL_AGE_MS: u64 = 5 * 60 * 1_000;

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
pub fn plan_git_clone_reclaim_with_pull_request_heads(
    repository_root: &Path,
    retention_references: &[String],
    closed_pull_request_heads: &ClosedPullRequestHeads,
    stale_open_pull_request_heads: &StaleOpenPullRequestHeads,
    stale_open_pull_request_cutoff_ms: Option<u64>,
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
    if !primary.closed_pull_request_head && !primary.stale_open_pull_request_head {
        blockers.push("git-clone-pr-head-authority-missing".into());
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
    let live = plan_git_clone_reclaim(
        Path::new(&approved_plan.repository_root),
        retention_references,
        include_closed_pull_requests,
        stale_open_pull_request_cutoff_ms,
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
