//! Fingerprint-bound execution for stale Git worktree removal.
//!
//! The read-only audit remains the authority for candidate classification. This module adds an
//! explicit, attributed approval gate and a write-ahead journal. It never deletes branches, never
//! runs `git worktree prune`, and never passes `--force` to `git worktree remove`.

use crate::git_worktree::{
    audit_git_worktrees, GitWorktreeAuditEntry, GitWorktreeAuditOptions, GitWorktreeAuditReport,
    GitWorktreeDisposition,
};
use serde_json::json;
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

pub const GIT_WORKTREE_REMOVAL_SCHEMA_KIND: &str = "disksage.git-worktree-removal/v1";
pub const GIT_WORKTREE_REMOVAL_APPROVAL_VERSION: u32 = 1;
const MAX_COMMAND_OUTPUT_BYTES: usize = 4 * 1024 * 1024;
const MAX_APPROVED_BY_BYTES: usize = 256;
const MIN_RATIONALE_BYTES: usize = 12;
const MAX_RATIONALE_BYTES: usize = 2_000;
const MAX_JOURNAL_EVENT_BYTES: usize = 8 * 1024 * 1024;
const POLL_INTERVAL_MS: u64 = 10;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitWorktreeRemovalApproval {
    pub version: u32,
    pub approval_id: String,
    pub removal_plan_fingerprint: String,
    pub retention_reference_set_fingerprint: String,
    pub approved_candidate_count: usize,
    pub approved_allocated_bytes: u64,
    pub exact_approval_phrase: String,
    pub approved_by: String,
    pub rationale: String,
    pub approved_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitWorktreeRemovedEntry {
    pub path_fingerprint: String,
    pub entry_fingerprint: String,
    pub head: String,
    pub observed_allocated_bytes: u64,
    pub removal_command_succeeded: bool,
    pub branch_delete_executed: bool,
    pub worktree_prune_executed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitWorktreeRemovalReport {
    pub schema_kind: String,
    pub version: u32,
    pub approval_id: String,
    pub executed_at_ms: u64,
    pub removal_plan_fingerprint: String,
    pub retention_reference_set_fingerprint: String,
    pub approved_candidate_count: usize,
    pub approved_allocated_bytes: u64,
    pub removed_candidate_count: usize,
    pub removed_observed_allocated_bytes: u64,
    pub remaining_approved_candidate_count: usize,
    pub removed_entries: Vec<GitWorktreeRemovedEntry>,
    pub stop_reason: Option<String>,
    pub complete: bool,
    pub write_ahead_journal_created: bool,
    pub write_ahead_journal_sealed: bool,
    pub filesystem_mutation_executed: bool,
    pub branch_delete_executed: bool,
    pub worktree_prune_executed: bool,
    pub force_remove_used: bool,
    pub reclaimed_bytes_verified: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct GitWorktreeRemovalPublicSummary {
    pub schema_kind: String,
    pub version: u32,
    pub approval_id: String,
    pub executed_at_ms: u64,
    pub removal_plan_fingerprint: String,
    pub retention_reference_set_fingerprint: String,
    pub approved_candidate_count: usize,
    pub approved_allocated_bytes: u64,
    pub removed_candidate_count: usize,
    pub removed_observed_allocated_bytes: u64,
    pub remaining_approved_candidate_count: usize,
    pub stop_reason: Option<String>,
    pub complete: bool,
    pub write_ahead_journal_created: bool,
    pub write_ahead_journal_sealed: bool,
    pub filesystem_mutation_executed: bool,
    pub branch_delete_executed: bool,
    pub worktree_prune_executed: bool,
    pub force_remove_used: bool,
    pub reclaimed_bytes_verified: bool,
    pub local_paths_redacted: bool,
    pub branch_names_redacted: bool,
    pub commit_heads_redacted: bool,
    pub approval_identity_and_rationale_redacted: bool,
    pub notices: Vec<String>,
}

struct CommandResult {
    status_code: Option<i32>,
    timed_out: bool,
    stdout_truncated: bool,
    stderr_truncated: bool,
}

struct Journal {
    file: File,
    path: PathBuf,
    parent: PathBuf,
}

fn valid_hex64(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn valid_bounded_text(
    value: &str,
    min_bytes: usize,
    max_bytes: usize,
    required_prefix: Option<&str>,
) -> bool {
    value.len() >= min_bytes
        && value.len() <= max_bytes
        && required_prefix.is_none_or(|prefix| value.starts_with(prefix))
        && !value.chars().any(char::is_control)
}

fn approval_id_for(approval: &GitWorktreeRemovalApproval) -> Result<String, String> {
    let mut unsigned = approval.clone();
    unsigned.approval_id.clear();
    let encoded = serde_json::to_vec(&unsigned)
        .map_err(|_| "git-worktree-removal-approval-json-invalid".to_string())?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage.git-worktree-removal-approval\0v1\0");
    hasher.update(&encoded);
    Ok(hasher.finalize().to_hex().to_string())
}

pub fn git_worktree_removal_approval_integrity_valid(
    approval: &GitWorktreeRemovalApproval,
) -> bool {
    approval.version == GIT_WORKTREE_REMOVAL_APPROVAL_VERSION
        && valid_hex64(&approval.approval_id)
        && valid_hex64(&approval.removal_plan_fingerprint)
        && valid_hex64(&approval.retention_reference_set_fingerprint)
        && approval.approved_candidate_count > 0
        && approval.approved_allocated_bytes > 0
        && approval.approved_at_ms > 0
        && valid_bounded_text(
            &approval.approved_by,
            "human:".len() + 1,
            MAX_APPROVED_BY_BYTES,
            Some("human:"),
        )
        && valid_bounded_text(
            &approval.rationale,
            MIN_RATIONALE_BYTES,
            MAX_RATIONALE_BYTES,
            None,
        )
        && valid_bounded_text(&approval.exact_approval_phrase, 1, 4_096, None)
        && approval_id_for(approval).is_ok_and(|expected| expected == approval.approval_id)
}

pub fn create_git_worktree_removal_approval(
    audit: &GitWorktreeAuditReport,
    exact_approval_phrase: &str,
    approved_by: &str,
    rationale: &str,
    approved_at_ms: u64,
) -> Result<GitWorktreeRemovalApproval, String> {
    if !audit.evidence_complete
        || audit.evidence_gap_count != 0
        || audit.removal_candidate_count == 0
        || audit.removal_candidate_allocated_bytes == 0
        || audit.filesystem_mutation_executed
        || audit.exact_approval_phrase.as_deref() != Some(exact_approval_phrase)
    {
        return Err("git-worktree-removal-audit-not-approvable".into());
    }
    if !valid_bounded_text(
        approved_by,
        "human:".len() + 1,
        MAX_APPROVED_BY_BYTES,
        Some("human:"),
    ) {
        return Err("git-worktree-removal-approved-by-invalid".into());
    }
    if !valid_bounded_text(rationale, MIN_RATIONALE_BYTES, MAX_RATIONALE_BYTES, None) {
        return Err("git-worktree-removal-rationale-invalid".into());
    }
    if approved_at_ms == 0 {
        return Err("git-worktree-removal-approved-at-invalid".into());
    }
    let mut approval = GitWorktreeRemovalApproval {
        version: GIT_WORKTREE_REMOVAL_APPROVAL_VERSION,
        approval_id: String::new(),
        removal_plan_fingerprint: audit.removal_plan_fingerprint.clone(),
        retention_reference_set_fingerprint: audit.retention_reference_set_fingerprint.clone(),
        approved_candidate_count: audit.removal_candidate_count,
        approved_allocated_bytes: audit.removal_candidate_allocated_bytes,
        exact_approval_phrase: exact_approval_phrase.to_string(),
        approved_by: approved_by.to_string(),
        rationale: rationale.to_string(),
        approved_at_ms,
    };
    approval.approval_id = approval_id_for(&approval)?;
    if !git_worktree_removal_approval_integrity_valid(&approval) {
        return Err("git-worktree-removal-approval-integrity-invalid".into());
    }
    Ok(approval)
}

fn validate_approval_against_audit(
    approval: &GitWorktreeRemovalApproval,
    audit: &GitWorktreeAuditReport,
    confirmed_plan_fingerprint: &str,
) -> Result<(), String> {
    if !git_worktree_removal_approval_integrity_valid(approval) {
        return Err("git-worktree-removal-approval-integrity-invalid".into());
    }
    if !valid_hex64(confirmed_plan_fingerprint)
        || confirmed_plan_fingerprint != approval.removal_plan_fingerprint
        || confirmed_plan_fingerprint != audit.removal_plan_fingerprint
        || approval.retention_reference_set_fingerprint != audit.retention_reference_set_fingerprint
        || approval.approved_candidate_count != audit.removal_candidate_count
        || approval.approved_allocated_bytes != audit.removal_candidate_allocated_bytes
        || audit.exact_approval_phrase.as_deref() != Some(&approval.exact_approval_phrase)
        || !audit.evidence_complete
        || audit.evidence_gap_count != 0
        || audit.filesystem_mutation_executed
    {
        return Err("git-worktree-removal-approval-plan-mismatch".into());
    }
    Ok(())
}

fn candidate_entries(audit: &GitWorktreeAuditReport) -> Vec<GitWorktreeAuditEntry> {
    let mut candidates: Vec<_> = audit
        .entries
        .iter()
        .filter(|entry| entry.disposition == GitWorktreeDisposition::RemovalCandidate)
        .cloned()
        .collect();
    candidates.sort_by(|left, right| {
        left.path_fingerprint
            .cmp(&right.path_fingerprint)
            .then_with(|| left.entry_fingerprint.cmp(&right.entry_fingerprint))
    });
    candidates
}

fn candidate_fingerprints(audit: &GitWorktreeAuditReport) -> BTreeSet<String> {
    candidate_entries(audit)
        .into_iter()
        .map(|entry| entry.entry_fingerprint)
        .collect()
}

fn drain_bounded<R: Read + Send + 'static>(mut reader: R) -> thread::JoinHandle<(Vec<u8>, bool)> {
    thread::spawn(move || {
        let mut stored = Vec::new();
        let mut truncated = false;
        let mut buffer = [0u8; 16 * 1024];
        loop {
            let Ok(read) = reader.read(&mut buffer) else {
                truncated = true;
                break;
            };
            if read == 0 {
                break;
            }
            let remaining = MAX_COMMAND_OUTPUT_BYTES.saturating_sub(stored.len());
            let retained = remaining.min(read);
            stored.extend_from_slice(&buffer[..retained]);
            if retained < read {
                truncated = true;
            }
        }
        (stored, truncated)
    })
}

fn run_bounded_command(
    program: &str,
    args: &[OsString],
    cwd: &Path,
    timeout_ms: u64,
) -> Result<CommandResult, String> {
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(cwd)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|_| "git-worktree-removal-command-spawn-failed".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "git-worktree-removal-stdout-capture-failed".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "git-worktree-removal-stderr-capture-failed".to_string())?;
    let stdout_thread = drain_bounded(stdout);
    let stderr_thread = drain_bounded(stderr);
    let started = Instant::now();
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if started.elapsed() >= Duration::from_millis(timeout_ms) => {
                timed_out = true;
                let _ = child.kill();
                break child.wait().ok();
            }
            Ok(None) => thread::sleep(Duration::from_millis(POLL_INTERVAL_MS)),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let (_stdout, stdout_truncated) = stdout_thread
        .join()
        .map_err(|_| "git-worktree-removal-stdout-reader-failed".to_string())?;
    let (_stderr, stderr_truncated) = stderr_thread
        .join()
        .map_err(|_| "git-worktree-removal-stderr-reader-failed".to_string())?;
    Ok(CommandResult {
        status_code: status.and_then(|value| value.code()),
        timed_out,
        stdout_truncated,
        stderr_truncated,
    })
}

fn git_remove_worktree(
    repository_root: &Path,
    worktree_path: &Path,
    timeout_ms: u64,
) -> Result<(), String> {
    let args = git_remove_args(worktree_path);
    let result = run_bounded_command("git", &args, repository_root, timeout_ms)?;
    if result.timed_out {
        return Err("git-worktree-removal-command-timeout".into());
    }
    if result.stdout_truncated || result.stderr_truncated {
        return Err("git-worktree-removal-command-output-truncated".into());
    }
    if result.status_code != Some(0) {
        return Err("git-worktree-removal-command-failed".into());
    }
    Ok(())
}

fn git_remove_args(worktree_path: &Path) -> Vec<OsString> {
    vec![
        OsString::from("worktree"),
        OsString::from("remove"),
        OsString::from("--"),
        worktree_path.as_os_str().to_os_string(),
    ]
}

fn safe_journal_path(path: &Path, audit: &GitWorktreeAuditReport) -> Result<PathBuf, String> {
    if !path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err("git-worktree-removal-journal-path-unsafe".into());
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| "git-worktree-removal-journal-parent-missing".to_string())?;
    let name = path
        .file_name()
        .ok_or_else(|| "git-worktree-removal-journal-name-invalid".to_string())?;
    if !matches!(
        Path::new(name).components().collect::<Vec<_>>().as_slice(),
        [Component::Normal(_)]
    ) {
        return Err("git-worktree-removal-journal-name-invalid".into());
    }
    let metadata = std::fs::symlink_metadata(parent)
        .map_err(|_| "git-worktree-removal-journal-parent-unavailable".to_string())?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("git-worktree-removal-journal-parent-unsafe".into());
    }
    let canonical_parent = std::fs::canonicalize(parent)
        .map_err(|_| "git-worktree-removal-journal-parent-unavailable".to_string())?;
    let canonical_path = canonical_parent.join(name);
    if canonical_path.starts_with(&audit.common_dir)
        || audit
            .entries
            .iter()
            .any(|entry| canonical_path.starts_with(&entry.path))
    {
        return Err("git-worktree-removal-journal-overlaps-repository".into());
    }
    Ok(canonical_path)
}

impl Journal {
    fn create(path: &Path, audit: &GitWorktreeAuditReport) -> Result<Self, String> {
        let path = safe_journal_path(path, audit)?;
        let parent = path
            .parent()
            .ok_or_else(|| "git-worktree-removal-journal-parent-missing".to_string())?
            .to_path_buf();
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options
            .open(&path)
            .map_err(|_| "git-worktree-removal-journal-create-failed".to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = file
                .metadata()
                .map_err(|_| "git-worktree-removal-journal-metadata-failed".to_string())?
                .permissions()
                .mode()
                & 0o777;
            if mode != 0o600
                || File::open(&parent)
                    .and_then(|directory| directory.sync_all())
                    .is_err()
            {
                drop(file);
                let _ = std::fs::remove_file(&path);
                return Err("git-worktree-removal-journal-durability-failed".into());
            }
        }
        Ok(Self { file, path, parent })
    }

    fn append(&mut self, value: &serde_json::Value) -> Result<(), String> {
        let mut encoded = serde_json::to_vec(value)
            .map_err(|_| "git-worktree-removal-journal-json-invalid".to_string())?;
        if encoded.len() > MAX_JOURNAL_EVENT_BYTES {
            return Err("git-worktree-removal-journal-event-too-large".into());
        }
        encoded.push(b'\n');
        self.file
            .write_all(&encoded)
            .and_then(|_| self.file.sync_all())
            .map_err(|_| "git-worktree-removal-journal-write-failed".to_string())
    }

    fn seal(&mut self) -> bool {
        if self.file.sync_all().is_err() {
            return false;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let permissions = std::fs::Permissions::from_mode(0o400);
            if std::fs::set_permissions(&self.path, permissions).is_err() {
                return false;
            }
            if File::open(&self.parent)
                .and_then(|directory| directory.sync_all())
                .is_err()
            {
                return false;
            }
        }
        #[cfg(not(unix))]
        {
            let Ok(metadata) = std::fs::metadata(&self.path) else {
                return false;
            };
            let mut permissions = metadata.permissions();
            permissions.set_readonly(true);
            if std::fs::set_permissions(&self.path, permissions).is_err() {
                return false;
            }
        }
        true
    }
}

fn report(
    approval: &GitWorktreeRemovalApproval,
    executed_at_ms: u64,
    removed_entries: Vec<GitWorktreeRemovedEntry>,
    remaining: usize,
    stop_reason: Option<String>,
    journal_sealed: bool,
) -> GitWorktreeRemovalReport {
    let removed_observed_allocated_bytes = removed_entries
        .iter()
        .map(|entry| entry.observed_allocated_bytes)
        .sum();
    let complete = remaining == 0
        && stop_reason.is_none()
        && removed_entries.len() == approval.approved_candidate_count;
    GitWorktreeRemovalReport {
        schema_kind: GIT_WORKTREE_REMOVAL_SCHEMA_KIND.into(),
        version: 1,
        approval_id: approval.approval_id.clone(),
        executed_at_ms,
        removal_plan_fingerprint: approval.removal_plan_fingerprint.clone(),
        retention_reference_set_fingerprint: approval.retention_reference_set_fingerprint.clone(),
        approved_candidate_count: approval.approved_candidate_count,
        approved_allocated_bytes: approval.approved_allocated_bytes,
        removed_candidate_count: removed_entries.len(),
        removed_observed_allocated_bytes,
        remaining_approved_candidate_count: remaining,
        filesystem_mutation_executed: !removed_entries.is_empty(),
        removed_entries,
        stop_reason,
        complete,
        write_ahead_journal_created: true,
        write_ahead_journal_sealed: journal_sealed,
        branch_delete_executed: false,
        worktree_prune_executed: false,
        force_remove_used: false,
        reclaimed_bytes_verified: false,
    }
}

fn journal_header(
    audit: &GitWorktreeAuditReport,
    approval: &GitWorktreeRemovalApproval,
    candidates: &[GitWorktreeAuditEntry],
    executed_at_ms: u64,
) -> serde_json::Value {
    json!({
        "event": "execution-start",
        "schema_kind": GIT_WORKTREE_REMOVAL_SCHEMA_KIND,
        "version": 1,
        "repository_root": audit.repository_root,
        "common_dir": audit.common_dir,
        "executed_at_ms": executed_at_ms,
        "approval": approval,
        "retention_references": audit.retention_references,
        "candidates": candidates.iter().map(|entry| json!({
            "path": entry.path,
            "path_fingerprint": entry.path_fingerprint,
            "entry_fingerprint": entry.entry_fingerprint,
            "head": entry.head,
            "branch": entry.branch,
            "observed_allocated_bytes": entry.size.allocated_bytes,
        })).collect::<Vec<_>>(),
        "branch_delete_authorized": false,
        "worktree_prune_authorized": false,
        "force_remove_authorized": false,
    })
}

/// Execute an exact, attributed stale-worktree removal plan.
///
/// The caller must provide a create-new journal path outside every audited worktree and the common
/// Git directory. The function re-audits before every removal, requires the exact remaining
/// candidate fingerprint set to match the approved plan, invokes `git worktree remove` without
/// `--force`, re-audits after every successful command, and stops on the first discrepancy.
pub fn execute_git_worktree_removal(
    repository_root: &Path,
    retention_references: &[String],
    audit_options: GitWorktreeAuditOptions,
    approval: &GitWorktreeRemovalApproval,
    confirmed_plan_fingerprint: &str,
    journal_path: &Path,
    executed_at_ms: u64,
) -> Result<GitWorktreeRemovalReport, String> {
    if executed_at_ms == 0 || executed_at_ms < approval.approved_at_ms {
        return Err("git-worktree-removal-execution-time-invalid".into());
    }
    let initial = audit_git_worktrees(
        repository_root,
        retention_references,
        audit_options,
        executed_at_ms,
    )?;
    validate_approval_against_audit(approval, &initial, confirmed_plan_fingerprint)?;
    let candidates = candidate_entries(&initial);
    if candidates.len() != approval.approved_candidate_count {
        return Err("git-worktree-removal-candidate-count-mismatch".into());
    }
    let mut expected_remaining = candidate_fingerprints(&initial);
    if expected_remaining.len() != candidates.len() {
        return Err("git-worktree-removal-candidate-fingerprint-collision".into());
    }

    let mut journal = Journal::create(journal_path, &initial)?;
    journal.append(&journal_header(
        &initial,
        approval,
        &candidates,
        executed_at_ms,
    ))?;

    let mut removed_entries = Vec::new();
    let mut stop_reason = None;
    for candidate in candidates {
        let fresh = match audit_git_worktrees(
            repository_root,
            retention_references,
            audit_options,
            executed_at_ms,
        ) {
            Ok(audit) => audit,
            Err(error) => {
                stop_reason = Some(format!("fresh-audit-failed:{error}"));
                break;
            }
        };
        let observed_remaining = candidate_fingerprints(&fresh);
        if observed_remaining != expected_remaining {
            stop_reason = Some("remaining-candidate-set-changed-before-remove".into());
            break;
        }
        let Some(current) = candidate_entries(&fresh)
            .into_iter()
            .find(|entry| entry.entry_fingerprint == candidate.entry_fingerprint)
        else {
            stop_reason = Some("candidate-missing-before-remove".into());
            break;
        };
        if current.path_fingerprint != candidate.path_fingerprint
            || current.head != candidate.head
            || current.size.allocated_bytes != candidate.size.allocated_bytes
        {
            stop_reason = Some("candidate-changed-before-remove".into());
            break;
        }
        if let Err(error) = journal.append(&json!({
            "event": "candidate-preflight",
            "path": current.path,
            "path_fingerprint": current.path_fingerprint,
            "entry_fingerprint": current.entry_fingerprint,
            "head": current.head,
            "remaining_candidate_count": expected_remaining.len(),
            "remaining_candidate_fingerprints": expected_remaining,
        })) {
            stop_reason = Some(error);
            break;
        }

        let command_result = git_remove_worktree(
            Path::new(&fresh.repository_root),
            Path::new(&current.path),
            audit_options.command_timeout_ms,
        );
        if let Err(error) = command_result {
            let _ = journal.append(&json!({
                "event": "candidate-remove-failed",
                "path": current.path,
                "path_fingerprint": current.path_fingerprint,
                "entry_fingerprint": current.entry_fingerprint,
                "reason": error,
            }));
            stop_reason = Some(error);
            break;
        }

        let removed = GitWorktreeRemovedEntry {
            path_fingerprint: current.path_fingerprint.clone(),
            entry_fingerprint: current.entry_fingerprint.clone(),
            head: current.head.clone(),
            observed_allocated_bytes: current.size.allocated_bytes,
            removal_command_succeeded: true,
            branch_delete_executed: false,
            worktree_prune_executed: false,
        };
        removed_entries.push(removed.clone());
        expected_remaining.remove(&current.entry_fingerprint);
        if let Err(error) = journal.append(&json!({
            "event": "candidate-removed",
            "path": current.path,
            "removed": removed,
            "remaining_candidate_count": expected_remaining.len(),
            "remaining_candidate_fingerprints": expected_remaining,
        })) {
            stop_reason = Some(error);
            break;
        }

        let after = match audit_git_worktrees(
            repository_root,
            retention_references,
            audit_options,
            executed_at_ms,
        ) {
            Ok(audit) => audit,
            Err(error) => {
                stop_reason = Some(format!("post-remove-audit-failed:{error}"));
                break;
            }
        };
        if candidate_fingerprints(&after) != expected_remaining {
            stop_reason = Some("remaining-candidate-set-changed-after-remove".into());
            break;
        }
    }

    let mut result = report(
        approval,
        executed_at_ms,
        removed_entries,
        expected_remaining.len(),
        stop_reason,
        false,
    );
    let summary_event = json!({
        "event": "execution-summary",
        "report": result,
    });
    if journal.append(&summary_event).is_err() && result.stop_reason.is_none() {
        result.stop_reason = Some("git-worktree-removal-journal-final-write-failed".into());
        result.complete = false;
    }
    result.write_ahead_journal_sealed = journal.seal();
    if !result.write_ahead_journal_sealed {
        result.complete = false;
        if result.stop_reason.is_none() {
            result.stop_reason = Some("git-worktree-removal-journal-seal-failed".into());
        }
    }
    Ok(result)
}

pub fn public_summary(report: &GitWorktreeRemovalReport) -> GitWorktreeRemovalPublicSummary {
    GitWorktreeRemovalPublicSummary {
        schema_kind: report.schema_kind.clone(),
        version: report.version,
        approval_id: report.approval_id.clone(),
        executed_at_ms: report.executed_at_ms,
        removal_plan_fingerprint: report.removal_plan_fingerprint.clone(),
        retention_reference_set_fingerprint: report.retention_reference_set_fingerprint.clone(),
        approved_candidate_count: report.approved_candidate_count,
        approved_allocated_bytes: report.approved_allocated_bytes,
        removed_candidate_count: report.removed_candidate_count,
        removed_observed_allocated_bytes: report.removed_observed_allocated_bytes,
        remaining_approved_candidate_count: report.remaining_approved_candidate_count,
        stop_reason: report.stop_reason.clone(),
        complete: report.complete,
        write_ahead_journal_created: report.write_ahead_journal_created,
        write_ahead_journal_sealed: report.write_ahead_journal_sealed,
        filesystem_mutation_executed: report.filesystem_mutation_executed,
        branch_delete_executed: report.branch_delete_executed,
        worktree_prune_executed: report.worktree_prune_executed,
        force_remove_used: report.force_remove_used,
        reclaimed_bytes_verified: report.reclaimed_bytes_verified,
        local_paths_redacted: true,
        branch_names_redacted: true,
        commit_heads_redacted: true,
        approval_identity_and_rationale_redacted: true,
        notices: vec![
            "exact-human-approval-bound".into(),
            "fresh-re-audit-before-every-remove".into(),
            "exact-remaining-candidate-set-required".into(),
            "write-ahead-journal-create-new-and-fsynced".into(),
            "git-worktree-remove-without-force".into(),
            "no-branch-delete".into(),
            "no-git-worktree-prune".into(),
            "observed-allocated-bytes-is-not-verified-reclamation".into(),
            "partial-success-is-reported-and-stops-execution".into(),
            "no-user-file-production-time-inference".into(),
            "no-cloud-provider-mutation".into(),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git_worktree::audit_git_worktrees;

    fn git(root: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[cfg(unix)]
    fn fixture() -> (
        tempfile::TempDir,
        PathBuf,
        PathBuf,
        Vec<String>,
        GitWorktreeAuditReport,
    ) {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("repository");
        std::fs::create_dir(&root).unwrap();
        git(&root, &["init", "--initial-branch=main"]);
        git(&root, &["config", "user.name", "DiskSage Test"]);
        git(&root, &["config", "user.email", "disksage@example.invalid"]);
        std::fs::write(root.join("tracked.txt"), b"base").unwrap();
        git(&root, &["add", "tracked.txt"]);
        git(&root, &["commit", "-m", "base"]);
        git(&root, &["branch", "old"]);
        std::fs::write(root.join("tracked.txt"), b"main advanced").unwrap();
        git(&root, &["commit", "-am", "advance main"]);
        let old = temp.path().join("old-worktree");
        git(&root, &["worktree", "add", old.to_str().unwrap(), "old"]);
        let references = vec!["main".to_string()];
        let audit =
            audit_git_worktrees(&root, &references, GitWorktreeAuditOptions::default(), 100)
                .unwrap();
        assert_eq!(audit.removal_candidate_count, 1);
        (temp, root, old, references, audit)
    }

    #[test]
    fn approval_is_exact_attributed_and_integrity_bound() {
        let mut audit = GitWorktreeAuditReport {
            schema_kind: "disksage.git-worktree-audit/v2".into(),
            version: 2,
            repository_root: "/tmp/repository".into(),
            common_dir: "/tmp/repository/.git".into(),
            generated_at_ms: 1,
            retention_references: Vec::new(),
            retention_reference_set_fingerprint: "a".repeat(64),
            retention_reachable_commit_count: 1,
            worktree_count: 2,
            removal_candidate_count: 1,
            removal_candidate_allocated_bytes: 4096,
            preserved_count: 1,
            evidence_gap_count: 0,
            evidence_complete: true,
            removal_plan_fingerprint: "b".repeat(64),
            exact_approval_phrase: Some(format!(
                "DiskSage stale worktree 1 4096 승인 {}",
                "b".repeat(64)
            )),
            entries: Vec::new(),
            issues: Vec::new(),
            filesystem_mutation_executed: false,
        };
        let phrase = audit.exact_approval_phrase.clone().unwrap();
        let approval = create_git_worktree_removal_approval(
            &audit,
            &phrase,
            "human:test",
            "reviewed exact stale worktree plan",
            10,
        )
        .unwrap();
        assert!(git_worktree_removal_approval_integrity_valid(&approval));

        let mut changed = approval.clone();
        changed.approved_candidate_count += 1;
        assert!(!git_worktree_removal_approval_integrity_valid(&changed));
        let mut uppercase = approval.clone();
        uppercase.removal_plan_fingerprint =
            uppercase.removal_plan_fingerprint.to_ascii_uppercase();
        uppercase.approval_id = approval_id_for(&uppercase).unwrap();
        assert!(!git_worktree_removal_approval_integrity_valid(&uppercase));
        assert!(create_git_worktree_removal_approval(
            &audit,
            "wrong phrase",
            "human:test",
            "reviewed exact stale worktree plan",
            10,
        )
        .is_err());
        audit.evidence_complete = false;
        assert!(create_git_worktree_removal_approval(
            &audit,
            &phrase,
            "human:test",
            "reviewed exact stale worktree plan",
            10,
        )
        .is_err());
    }

    #[cfg(unix)]
    #[test]
    fn execute_removes_only_approved_clean_worktree_and_preserves_branch() {
        let (temp, root, old, references, audit) = fixture();
        let phrase = audit.exact_approval_phrase.clone().unwrap();
        let approval = create_git_worktree_removal_approval(
            &audit,
            &phrase,
            "human:test",
            "approved exact clean merged fixture worktree",
            200,
        )
        .unwrap();
        let journal = temp.path().join("execution.jsonl");
        let result = execute_git_worktree_removal(
            &root,
            &references,
            GitWorktreeAuditOptions::default(),
            &approval,
            &audit.removal_plan_fingerprint,
            &journal,
            300,
        )
        .unwrap();

        assert!(result.complete);
        assert_eq!(result.removed_candidate_count, 1);
        assert_eq!(result.remaining_approved_candidate_count, 0);
        assert!(result.filesystem_mutation_executed);
        assert!(!result.branch_delete_executed);
        assert!(!result.worktree_prune_executed);
        assert!(!result.force_remove_used);
        assert!(!old.exists());
        git(&root, &["show-ref", "--verify", "refs/heads/old"]);
        let journal_text = std::fs::read_to_string(&journal).unwrap();
        assert!(journal_text.contains("\"event\":\"execution-start\""));
        assert!(journal_text.contains("\"event\":\"candidate-removed\""));
        assert!(journal_text.contains("\"event\":\"execution-summary\""));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(journal).unwrap().permissions().mode() & 0o777,
                0o400
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn changed_candidate_set_rejects_before_journal_or_mutation() {
        let (temp, root, old, references, audit) = fixture();
        let phrase = audit.exact_approval_phrase.clone().unwrap();
        let approval = create_git_worktree_removal_approval(
            &audit,
            &phrase,
            "human:test",
            "approved exact clean merged fixture worktree",
            200,
        )
        .unwrap();
        std::fs::write(old.join("untracked.txt"), b"changed").unwrap();
        let journal = temp.path().join("execution.jsonl");
        assert!(execute_git_worktree_removal(
            &root,
            &references,
            GitWorktreeAuditOptions::default(),
            &approval,
            &audit.removal_plan_fingerprint,
            &journal,
            300,
        )
        .is_err());
        assert!(old.exists());
        assert!(!journal.exists());
    }

    #[test]
    fn public_summary_redacts_local_identity_and_denies_extra_mutations() {
        let report = GitWorktreeRemovalReport {
            schema_kind: GIT_WORKTREE_REMOVAL_SCHEMA_KIND.into(),
            version: 1,
            approval_id: "a".repeat(64),
            executed_at_ms: 100,
            removal_plan_fingerprint: "b".repeat(64),
            retention_reference_set_fingerprint: "c".repeat(64),
            approved_candidate_count: 1,
            approved_allocated_bytes: 4096,
            removed_candidate_count: 1,
            removed_observed_allocated_bytes: 4096,
            remaining_approved_candidate_count: 0,
            removed_entries: vec![GitWorktreeRemovedEntry {
                path_fingerprint: "d".repeat(64),
                entry_fingerprint: "e".repeat(64),
                head: "f".repeat(40),
                observed_allocated_bytes: 4096,
                removal_command_succeeded: true,
                branch_delete_executed: false,
                worktree_prune_executed: false,
            }],
            stop_reason: None,
            complete: true,
            write_ahead_journal_created: true,
            write_ahead_journal_sealed: true,
            filesystem_mutation_executed: true,
            branch_delete_executed: false,
            worktree_prune_executed: false,
            force_remove_used: false,
            reclaimed_bytes_verified: false,
        };
        let value = serde_json::to_value(public_summary(&report)).unwrap();
        let encoded = serde_json::to_string(&value).unwrap();
        assert!(!encoded.contains("/Users/"));
        assert!(!encoded.contains("refs/heads/"));
        assert!(!encoded.contains(&"f".repeat(40)));
        assert!(value["local_paths_redacted"].as_bool().unwrap());
        assert!(!value["branch_delete_executed"].as_bool().unwrap());
        assert!(!value["worktree_prune_executed"].as_bool().unwrap());
        assert!(!value["force_remove_used"].as_bool().unwrap());
        assert!(!value["reclaimed_bytes_verified"].as_bool().unwrap());
    }

    #[test]
    fn git_command_is_remove_only_without_force_prune_or_branch_delete() {
        let args = git_remove_args(Path::new("/tmp/approved-worktree"));
        let rendered: Vec<_> = args
            .iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            rendered,
            vec!["worktree", "remove", "--", "/tmp/approved-worktree"]
        );
        assert!(!rendered.iter().any(|value| value == "--force"));
        assert!(!rendered.iter().any(|value| value == "prune"));
        assert!(!rendered.iter().any(|value| value == "branch"));
    }

    #[cfg(unix)]
    #[test]
    fn existing_journal_is_not_overwritten_and_prevents_mutation() {
        let (temp, root, old, references, audit) = fixture();
        let phrase = audit.exact_approval_phrase.clone().unwrap();
        let approval = create_git_worktree_removal_approval(
            &audit,
            &phrase,
            "human:test",
            "approved exact clean merged fixture worktree",
            200,
        )
        .unwrap();
        let journal = temp.path().join("execution.jsonl");
        std::fs::write(&journal, b"existing").unwrap();

        assert!(execute_git_worktree_removal(
            &root,
            &references,
            GitWorktreeAuditOptions::default(),
            &approval,
            &audit.removal_plan_fingerprint,
            &journal,
            300,
        )
        .is_err());
        assert!(old.exists());
        assert_eq!(std::fs::read(journal).unwrap(), b"existing");
    }
}
