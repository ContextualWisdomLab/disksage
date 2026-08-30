//! Fail-closed, fingerprint-bound Git worktree auditing.
//!
//! The audit is read-only. A worktree is a removal candidate only when its HEAD is already
//! contained in an explicitly selected retention-reference set without itself being an exact
//! retained tip, its tracked and untracked state is clean, its path and size evidence are complete,
//! it is neither locked nor prunable, and no active CWD or open-file consumer is observed. The
//! resulting approval phrase is evidence, not execution.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::fd::{AsRawFd, FromRawFd};
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

pub const GIT_WORKTREE_AUDIT_SCHEMA_KIND: &str = "disksage.git-worktree-audit/v4";
const MAX_COMMAND_OUTPUT_BYTES: usize = 4 * 1024 * 1024;
/// Maximum UTF-8 byte length accepted for a Git reference at the audit boundary.
pub const MAX_REFERENCE_BYTES: usize = 1_024;
const MAX_REACHABLE_COMMITS: usize = 100_000;
const GIT_WORKTREE_REMOVAL_VERSION: u32 = 2;
const MAX_RATIONALE_BYTES: usize = 1_000;
const MAX_ADMIN_FALLBACK_ENTRIES: usize = 512;
const MAX_ADMIN_FALLBACK_FILE_BYTES: u64 = 16 * 1024;
const POLL_INTERVAL_MS: u64 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitWorktreeAuditOptions {
    pub command_timeout_ms: u64,
    pub size_scan_timeout_ms: u64,
    pub max_worktrees: usize,
    pub max_entries_per_worktree: u64,
    pub max_active_pids: usize,
}

impl Default for GitWorktreeAuditOptions {
    fn default() -> Self {
        Self {
            command_timeout_ms: 10_000,
            size_scan_timeout_ms: 60_000,
            max_worktrees: 2_048,
            max_entries_per_worktree: 2_000_000,
            max_active_pids: 64,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GitWorktreeDisposition {
    RemovalCandidate,
    Preserve,
    EvidenceGap,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitWorktreeSizeEvidence {
    pub method: String,
    pub evidence_complete: bool,
    pub allocated_bytes: u64,
    pub logical_bytes: u64,
    pub visited_entries: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitWorktreeActiveUseEvidence {
    pub method: String,
    pub assessed: bool,
    pub evidence_complete: bool,
    pub active: bool,
    pub observed_pids: Vec<u32>,
    pub results_truncated: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitWorktreeAuditEntry {
    pub path: String,
    pub path_fingerprint: String,
    pub head: String,
    pub branch: Option<String>,
    pub detached: bool,
    pub bare: bool,
    pub primary: bool,
    pub audit_origin: bool,
    pub locked: bool,
    pub lock_reason: Option<String>,
    pub prunable: bool,
    pub prunable_reason: Option<String>,
    pub status_clean: Option<bool>,
    pub status_entry_count: Option<u64>,
    pub contained_in_reference: Option<bool>,
    pub closed_pull_request_head: bool,
    /// The exact worktree commit occurs in a completed same-repository pull request.
    #[serde(default)]
    pub completed_pull_request_commit: bool,
    /// The exact worktree commit occurs in an open same-repository pull request and must be kept.
    #[serde(default)]
    pub open_pull_request_commit: bool,
    #[serde(default)]
    pub stale_open_pull_request_head: bool,
    pub head_is_retained_tip: bool,
    pub actor_cwd_inside: Option<bool>,
    pub size: GitWorktreeSizeEvidence,
    pub active_use: GitWorktreeActiveUseEvidence,
    pub disposition: GitWorktreeDisposition,
    pub blockers: Vec<String>,
    pub entry_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitWorktreeReferenceBinding {
    pub reference_ref: String,
    pub reference_oid: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitWorktreeAuditReport {
    pub schema_kind: String,
    pub version: u32,
    pub repository_root: String,
    pub common_dir: String,
    pub generated_at_ms: u64,
    #[serde(default)]
    pub stale_open_pull_request_cutoff_ms: Option<u64>,
    pub retention_references: Vec<GitWorktreeReferenceBinding>,
    pub retention_reference_set_fingerprint: String,
    pub removal_authority_fingerprint: String,
    pub retention_reachable_commit_count: usize,
    pub worktree_count: usize,
    pub removal_candidate_count: usize,
    pub removal_candidate_allocated_bytes: u64,
    pub preserved_count: usize,
    pub evidence_gap_count: usize,
    pub evidence_complete: bool,
    pub removal_plan_fingerprint: String,
    pub exact_approval_phrase: Option<String>,
    pub entries: Vec<GitWorktreeAuditEntry>,
    pub issues: Vec<String>,
    pub filesystem_mutation_executed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct GitWorktreeAuditPublicSummary {
    pub schema_kind: String,
    pub version: u32,
    pub generated_at_ms: u64,
    pub stale_open_pull_request_cutoff_ms: Option<u64>,
    pub retention_reference_count: usize,
    pub retention_reference_set_fingerprint: String,
    pub removal_authority_fingerprint: String,
    pub retention_reachable_commit_count: usize,
    pub worktree_count: usize,
    pub removal_candidate_count: usize,
    pub removal_candidate_allocated_bytes: u64,
    pub preserved_count: usize,
    pub evidence_gap_count: usize,
    pub evidence_complete: bool,
    pub removal_plan_fingerprint: String,
    pub exact_approval_phrase: Option<String>,
    pub filesystem_mutation_executed: bool,
    pub local_paths_redacted: bool,
    pub branch_names_redacted: bool,
    pub metadata_semantics: Vec<String>,
    pub notices: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitWorktreeRemovalApproval {
    pub version: u32,
    pub approval_id: String,
    pub removal_plan_fingerprint: String,
    pub retention_reference_set_fingerprint: String,
    pub removal_authority_fingerprint: String,
    pub removal_candidate_count: usize,
    pub removal_candidate_allocated_bytes: u64,
    pub exact_approval_phrase: String,
    pub approved_at_ms: u64,
    pub approved_by: String,
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitWorktreeRemovalItemResult {
    pub path: String,
    pub path_fingerprint: String,
    pub entry_fingerprint: String,
    pub head: String,
    pub branch: Option<String>,
    pub allocated_bytes_upper_bound: u64,
    pub removal_attempted: bool,
    pub removal_command_succeeded: bool,
    pub path_absence_verified: bool,
    pub registration_absence_verified: bool,
    pub branch_retained: Option<bool>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitWorktreeRemovalResult {
    pub version: u32,
    pub result_id: String,
    pub approval_id: String,
    pub removal_plan_fingerprint: String,
    pub retention_reference_set_fingerprint: String,
    pub removal_authority_fingerprint: String,
    pub requested_at_ms: u64,
    pub completed_at_ms: u64,
    pub planned_candidate_count: usize,
    pub attempted_count: usize,
    pub removed_count: usize,
    pub planned_allocated_bytes_upper_bound: u64,
    pub removed_allocated_bytes_upper_bound: u64,
    pub items: Vec<GitWorktreeRemovalItemResult>,
    pub stopped_reason: Option<String>,
    pub branch_delete_executed: bool,
    pub git_prune_executed: bool,
    pub filesystem_mutation_executed: bool,
    pub verification_complete: bool,
    pub notices: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RawWorktree {
    path: PathBuf,
    head: String,
    branch: Option<String>,
    detached: bool,
    bare: bool,
    locked: bool,
    lock_reason: Option<String>,
    prunable: bool,
    prunable_reason: Option<String>,
    fallback_evidence_incomplete: bool,
}

#[derive(Default)]
struct RawWorktreeBuilder {
    path: Option<PathBuf>,
    head: Option<String>,
    branch: Option<String>,
    detached: bool,
    bare: bool,
    locked: bool,
    lock_reason: Option<String>,
    prunable: bool,
    prunable_reason: Option<String>,
}

impl RawWorktreeBuilder {
    fn finish(self) -> Result<RawWorktree, String> {
        Ok(RawWorktree {
            path: self
                .path
                .ok_or_else(|| "git-worktree-porcelain-path-missing".to_string())?,
            head: self
                .head
                .ok_or_else(|| "git-worktree-porcelain-head-missing".to_string())?,
            branch: self.branch,
            detached: self.detached,
            bare: self.bare,
            locked: self.locked,
            lock_reason: self.lock_reason,
            prunable: self.prunable,
            prunable_reason: self.prunable_reason,
            fallback_evidence_incomplete: false,
        })
    }

    fn is_empty(&self) -> bool {
        self.path.is_none()
            && self.head.is_none()
            && self.branch.is_none()
            && !self.detached
            && !self.bare
            && !self.locked
            && !self.prunable
    }
}

struct CommandResult {
    child_pid: u32,
    status_code: Option<i32>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    timed_out: bool,
    stdout_truncated: bool,
    stderr_truncated: bool,
}

#[derive(Debug)]
struct ClassificationInput {
    primary: bool,
    audit_origin: bool,
    bare: bool,
    locked: bool,
    prunable: bool,
    path_valid: bool,
    status_clean: Option<bool>,
    contained_in_reference: Option<bool>,
    closed_pull_request_head: bool,
    completed_pull_request_commit: bool,
    open_pull_request_commit: bool,
    stale_open_pull_request_head: bool,
    head_is_retained_tip: bool,
    actor_cwd_inside: Option<bool>,
    size_complete: bool,
    active_use_assessed: bool,
    active_use_complete: bool,
    active_use_active: bool,
}

fn validate_options(options: GitWorktreeAuditOptions) -> Result<(), String> {
    if options.command_timeout_ms == 0 || options.command_timeout_ms > 300_000 {
        return Err("git-worktree-command-timeout-out-of-bounds".into());
    }
    if options.size_scan_timeout_ms == 0 || options.size_scan_timeout_ms > 600_000 {
        return Err("git-worktree-size-timeout-out-of-bounds".into());
    }
    if options.max_worktrees == 0 || options.max_worktrees > 10_000 {
        return Err("git-worktree-count-limit-out-of-bounds".into());
    }
    if options.max_entries_per_worktree == 0 || options.max_entries_per_worktree > 20_000_000 {
        return Err("git-worktree-entry-limit-out-of-bounds".into());
    }
    if options.max_active_pids == 0 || options.max_active_pids > 4_096 {
        return Err("git-worktree-active-pid-limit-out-of-bounds".into());
    }
    Ok(())
}

pub fn validate_reference(reference: &str) -> Result<(), String> {
    if reference.is_empty()
        || reference.len() > MAX_REFERENCE_BYTES
        || reference.starts_with('-')
        || reference
            .chars()
            .any(|character| character.is_control() || character == '\0')
    {
        return Err("git-worktree-reference-invalid".into());
    }
    Ok(())
}

fn is_oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
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
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if program == "git" {
        command.env("GIT_OPTIONAL_LOCKS", "0");
    } else if program == "gh" {
        command.env_remove("GH_REPO");
    }
    #[cfg(unix)]
    // Keep descendants in a private process group so a timeout cannot leave a Git helper holding
    // stdout/stderr pipes open and make the bounded reader join hang indefinitely.
    unsafe {
        command.pre_exec(|| {
            if libc::setpgid(0, 0) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = command
        .spawn()
        .map_err(|_| format!("{program}-command-spawn-failed"))?;
    let child_pid = child.id();
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| format!("{program}-stdout-capture-failed"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| format!("{program}-stderr-capture-failed"))?;
    let stdout_thread = drain_bounded(stdout);
    let stderr_thread = drain_bounded(stderr);
    let started = Instant::now();
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if started.elapsed() >= Duration::from_millis(timeout_ms) => {
                timed_out = true;
                #[cfg(unix)]
                unsafe {
                    let _ = libc::kill(-(child_pid as libc::pid_t), libc::SIGKILL);
                }
                let _ = child.kill();
                break child.wait().ok();
            }
            Ok(None) => thread::sleep(Duration::from_millis(POLL_INTERVAL_MS)),
            Err(_) => {
                #[cfg(unix)]
                unsafe {
                    let _ = libc::kill(-(child_pid as libc::pid_t), libc::SIGKILL);
                }
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let (stdout, stdout_truncated) = stdout_thread
        .join()
        .map_err(|_| format!("{program}-stdout-reader-failed"))?;
    let (stderr, stderr_truncated) = stderr_thread
        .join()
        .map_err(|_| format!("{program}-stderr-reader-failed"))?;
    Ok(CommandResult {
        child_pid,
        status_code: status.and_then(|value| value.code()),
        stdout,
        stderr,
        timed_out,
        stdout_truncated,
        stderr_truncated,
    })
}

fn command_text<'a>(bytes: &'a [u8], reason: &str) -> Result<&'a str, String> {
    std::str::from_utf8(bytes).map_err(|_| reason.to_string())
}

fn run_git(
    cwd: &Path,
    args: &[OsString],
    timeout_ms: u64,
    reason: &str,
) -> Result<CommandResult, String> {
    let result = run_bounded_command("git", args, cwd, timeout_ms)?;
    if result.timed_out {
        return Err(format!("{reason}-timeout"));
    }
    if result.stdout_truncated || result.stderr_truncated {
        return Err(format!("{reason}-output-truncated"));
    }
    Ok(result)
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct GitHubPullRequestHead {
    #[serde(default)]
    number: Option<u64>,
    #[serde(rename = "headRefName")]
    head_ref_name: String,
    #[serde(rename = "headRefOid")]
    head_ref_oid: String,
    #[serde(rename = "isCrossRepository")]
    is_cross_repository: bool,
    #[serde(rename = "createdAt")]
    created_at: Option<String>,
    state: String,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct GitHubSearchRepository {
    #[serde(rename = "name")]
    _name: String,
    #[serde(rename = "nameWithOwner")]
    name_with_owner: String,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct GitHubSearchPullRequest {
    number: u64,
    state: String,
    repository: GitHubSearchRepository,
}

pub type ClosedPullRequestHeads = BTreeSet<(String, String)>;
pub type StaleOpenPullRequestHeads = BTreeMap<(String, String), BTreeSet<u64>>;
pub type PullRequestCommits = BTreeSet<String>;

/// Verify that a locally available reference is still bound to an exact provider-observed OID
/// and that `ancestor_oid` is reachable from it. The reference/OID equality makes stale remote
/// tracking refs fail closed instead of silently authorizing cleanup.
pub(crate) fn exact_reference_contains_commit(
    repository_root: &Path,
    reference: &str,
    reference_oid: &str,
    ancestor_oid: &str,
    timeout_ms: u64,
) -> Result<bool, String> {
    validate_reference(reference)?;
    for oid in [reference_oid, ancestor_oid] {
        if oid.len() != 40 || !oid.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("git-default-branch-oid-invalid".into());
        }
    }
    let resolved = run_git(
        repository_root,
        &[
            OsString::from("rev-parse"),
            OsString::from("--verify"),
            OsString::from(format!("{reference}^{{commit}}")),
        ],
        timeout_ms,
        "git-default-branch-reference",
    )?;
    let expected_reference_oid = reference_oid.to_ascii_lowercase();
    if resolved.status_code != Some(0)
        || String::from_utf8(resolved.stdout)
            .ok()
            .map(|value| value.trim().to_ascii_lowercase())
            .as_deref()
            != Some(expected_reference_oid.as_str())
    {
        return Ok(false);
    }
    let ancestry = run_git(
        repository_root,
        &[
            OsString::from("merge-base"),
            OsString::from("--is-ancestor"),
            OsString::from(ancestor_oid),
            OsString::from(reference_oid),
        ],
        timeout_ms,
        "git-default-branch-ancestry",
    )?;
    Ok(ancestry.status_code == Some(0))
}

pub(crate) fn is_standalone_repository_root(
    repository_root: &Path,
    timeout_ms: u64,
) -> Result<bool, String> {
    let common_dir = match resolve_common_dir(repository_root, timeout_ms) {
        Ok(path) => path,
        Err(error) if error == "git-common-dir-resolve-failed" => return Ok(false),
        Err(error) => return Err(error),
    };
    let git_entry = repository_root.join(".git");
    let Ok(metadata) = fs::symlink_metadata(&git_entry) else {
        return Ok(false);
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Ok(false);
    }
    Ok(fs::canonicalize(&git_entry).ok().as_deref() == Some(common_dir.as_path()))
}

/// Resolve the provider's current default branch and exact OID through the authenticated GitHub
/// CLI. The returned local reference must still be checked against this OID before use.
pub(crate) fn github_default_branch_reference_oid(
    repository_root: &Path,
    timeout_ms: u64,
) -> Result<(String, String), String> {
    let started = Instant::now();
    let identity = run_bounded_command(
        "gh",
        &[
            OsString::from("repo"),
            OsString::from("view"),
            OsString::from("--json"),
            OsString::from("nameWithOwner,defaultBranchRef"),
            OsString::from("--jq"),
            OsString::from("[.nameWithOwner,.defaultBranchRef.name]|@tsv"),
        ],
        repository_root,
        timeout_ms,
    )?;
    if identity.timed_out {
        return Err("github-default-branch-timeout".into());
    }
    if identity.stdout_truncated || identity.stderr_truncated {
        return Err("github-default-branch-output-truncated".into());
    }
    if identity.status_code != Some(0) {
        return Err("github-default-branch-query-failed".into());
    }
    let text = command_text(&identity.stdout, "github-default-branch-output-not-utf8")?;
    let mut fields = text.trim().split('\t');
    let repository = fields
        .next()
        .filter(|value| {
            value.split('/').count() == 2
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"-_./".contains(&byte))
        })
        .ok_or_else(|| "github-default-branch-output-invalid".to_string())?;
    let branch = fields
        .next()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "github-default-branch-output-invalid".to_string())?;
    if fields.next().is_some() {
        return Err("github-default-branch-output-invalid".into());
    }
    validate_reference(&format!("refs/heads/{branch}"))?;
    let remaining_ms = timeout_ms.saturating_sub(started.elapsed().as_millis() as u64);
    if remaining_ms == 0 {
        return Err("github-default-branch-timeout".into());
    }
    let remote_name = matching_github_remote_name(repository_root, repository, remaining_ms)?;
    let remaining_ms = timeout_ms.saturating_sub(started.elapsed().as_millis() as u64);
    if remaining_ms == 0 {
        return Err("github-default-branch-timeout".into());
    }
    let commit = run_bounded_command(
        "gh",
        &[
            OsString::from("api"),
            OsString::from(format!("repos/{repository}/commits/{branch}")),
            OsString::from("--jq"),
            OsString::from(".sha"),
        ],
        repository_root,
        remaining_ms,
    )?;
    if commit.timed_out {
        return Err("github-default-branch-timeout".into());
    }
    if commit.stdout_truncated || commit.stderr_truncated {
        return Err("github-default-branch-output-truncated".into());
    }
    if commit.status_code != Some(0) {
        return Err("github-default-branch-query-failed".into());
    }
    let oid = command_text(&commit.stdout, "github-default-branch-output-not-utf8")?
        .trim()
        .to_ascii_lowercase();
    if !is_oid(&oid) {
        return Err("github-default-branch-output-invalid".into());
    }
    Ok((format!("refs/remotes/{remote_name}/{branch}"), oid))
}

fn github_slug_from_remote_url(value: &str) -> Option<String> {
    let trimmed = value.trim().trim_end_matches('/').trim_end_matches(".git");
    let path = if let Some(value) = trimmed.strip_prefix("git@github.com:") {
        value
    } else if let Some(value) = trimmed.strip_prefix("ssh://git@github.com/") {
        value
    } else if let Some(value) = trimmed.strip_prefix("https://github.com/") {
        value
    } else if let Some(value) = trimmed.strip_prefix("http://github.com/") {
        value
    } else {
        return None;
    };
    let mut parts = path.split('/');
    let owner = parts.next().filter(|part| !part.is_empty())?;
    let repository = parts.next().filter(|part| !part.is_empty())?;
    if parts.next().is_some() {
        return None;
    }
    Some(format!("{owner}/{repository}").to_ascii_lowercase())
}

fn matching_github_remote_name(
    repository_root: &Path,
    repository: &str,
    timeout_ms: u64,
) -> Result<String, String> {
    let output = run_git(
        repository_root,
        &[
            OsString::from("config"),
            OsString::from("--get-regexp"),
            OsString::from(r"^remote\..*\.url$"),
        ],
        timeout_ms,
        "git-remote-config",
    )?;
    if output.status_code != Some(0) {
        return Err("github-default-branch-remote-unavailable".into());
    }
    let expected = repository.to_ascii_lowercase();
    let mut matches = BTreeSet::new();
    for line in command_text(&output.stdout, "git-remote-config-not-utf8")?.lines() {
        let Some((key, url)) = line.split_once(char::is_whitespace) else {
            continue;
        };
        let Some(remote_name) = key
            .strip_prefix("remote.")
            .and_then(|value| value.strip_suffix(".url"))
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        validate_reference(&format!("refs/remotes/{remote_name}/placeholder"))?;
        if github_slug_from_remote_url(url).as_deref() == Some(expected.as_str()) {
            matches.insert(remote_name.to_string());
        }
    }
    if matches.len() != 1 {
        return Err("github-default-branch-remote-not-unique".into());
    }
    Ok(matches.into_iter().next().expect("one matching remote"))
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PullRequestCommitMembership {
    pub completed: PullRequestCommits,
    pub open: BTreeMap<String, BTreeSet<u64>>,
}

fn parse_closed_pull_request_heads(bytes: &[u8]) -> Result<ClosedPullRequestHeads, String> {
    let records: Vec<GitHubPullRequestHead> =
        serde_json::from_slice(bytes).map_err(|_| "github-closed-pr-json-invalid".to_string())?;
    let closed_heads: ClosedPullRequestHeads = records
        .into_iter()
        .filter(|record| {
            matches!(record.state.as_str(), "CLOSED" | "MERGED") && !record.is_cross_repository
        })
        .map(|record| {
            let oid = record.head_ref_oid.to_ascii_lowercase();
            let branch_ref = format!("refs/heads/{}", record.head_ref_name);
            validate_reference(&branch_ref)?;
            if !is_oid(&oid) {
                return Err("github-closed-pr-head-invalid".to_string());
            }
            Ok((branch_ref, oid))
        })
        .collect::<Result<_, _>>()?;
    if closed_heads.len() > 10_000 {
        return Err("github-closed-pr-count-exceeds-limit".into());
    }
    Ok(closed_heads)
}

fn parse_open_pull_request_heads(bytes: &[u8]) -> Result<ClosedPullRequestHeads, String> {
    let records: Vec<GitHubPullRequestHead> =
        serde_json::from_slice(bytes).map_err(|_| "github-open-pr-json-invalid".to_string())?;
    records
        .into_iter()
        .filter(|record| record.state == "OPEN" && !record.is_cross_repository)
        .map(|record| {
            let oid = record.head_ref_oid.to_ascii_lowercase();
            let branch_ref = format!("refs/heads/{}", record.head_ref_name);
            validate_reference(&branch_ref)?;
            if !is_oid(&oid) {
                return Err("github-open-pr-head-invalid".to_string());
            }
            Ok((branch_ref, oid))
        })
        .collect()
}

fn parse_pull_request_search(bytes: &[u8], repository: &str) -> Result<Vec<(u64, bool)>, String> {
    let records: Vec<GitHubSearchPullRequest> = serde_json::from_slice(bytes)
        .map_err(|_| "github-pr-commit-search-json-invalid".to_string())?;
    if records.len() > 100 {
        return Err("github-pr-commit-search-incomplete".into());
    }
    records
        .into_iter()
        .map(|record| {
            if record.repository.name_with_owner != repository || record.number == 0 {
                return Err("github-pr-commit-search-repository-mismatch".into());
            }
            match record.state.as_str() {
                "open" => Ok((record.number, true)),
                "closed" | "merged" => Ok((record.number, false)),
                _ => Err("github-pr-commit-search-state-invalid".into()),
            }
        })
        .collect()
}

fn pull_request_contains_commit(bytes: &[u8], head: &str) -> Result<bool, String> {
    let text = command_text(bytes, "github-pr-commits-not-utf8")?;
    let mut count = 0usize;
    let mut found = false;
    for line in text.lines() {
        let oid = line.trim().to_ascii_lowercase();
        if !is_oid(&oid) {
            return Err("github-pr-commit-invalid".into());
        }
        count = count.saturating_add(1);
        found |= oid == head;
    }
    if count >= 250 {
        return Err("github-pr-commit-count-exceeds-limit".into());
    }
    Ok(found)
}

fn parse_github_timestamp_ms(value: &str) -> Option<u64> {
    let bytes = value.as_bytes();
    if bytes.len() != 20
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
        || bytes[19] != b'Z'
    {
        return None;
    }
    let number = |start: usize, end: usize| value.get(start..end)?.parse::<u64>().ok();
    let year = number(0, 4)?;
    let month = number(5, 7)?;
    let day = number(8, 10)?;
    let hour = number(11, 13)?;
    let minute = number(14, 16)?;
    let second = number(17, 19)?;
    if year == 0
        || !(1..=12).contains(&month)
        || day == 0
        || day > days_in_month(year, month)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }
    let days = days_before_year(year)?
        .checked_add(days_before_month(year, month))?
        .checked_add(day - 1)?
        .checked_sub(days_before_year(1970)?)?;
    days.checked_mul(86_400_000)?
        .checked_add(hour * 3_600_000)?
        .checked_add(minute * 60_000)?
        .checked_add(second * 1_000)
}

fn is_leap_year(year: u64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn days_in_month(year: u64, month: u64) -> u64 {
    match month {
        2 if is_leap_year(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn days_before_year(year: u64) -> Option<u64> {
    let prior = year.checked_sub(1)?;
    year.checked_mul(365)?
        .checked_add(prior / 4)?
        .checked_sub(prior / 100)?
        .checked_add(prior / 400)
}

fn days_before_month(year: u64, month: u64) -> u64 {
    (1..month)
        .map(|candidate| days_in_month(year, candidate))
        .sum()
}

fn parse_stale_open_pull_request_heads(
    bytes: &[u8],
    cutoff_ms: u64,
) -> Result<StaleOpenPullRequestHeads, String> {
    let records: Vec<GitHubPullRequestHead> =
        serde_json::from_slice(bytes).map_err(|_| "github-open-pr-json-invalid".to_string())?;
    let mut stale_heads = StaleOpenPullRequestHeads::new();
    for record in records {
        if record.state != "OPEN" || record.is_cross_repository {
            continue;
        }
        let created_at = record
            .created_at
            .as_deref()
            .ok_or_else(|| "github-open-pr-created-at-missing".to_string())?;
        let created_ms = parse_github_timestamp_ms(created_at)
            .ok_or_else(|| "github-open-pr-created-at-invalid".to_string())?;
        if created_ms >= cutoff_ms {
            continue;
        }
        let head_ref_name = record.head_ref_name;
        let head_ref_oid = record.head_ref_oid;
        let binding = {
            let oid = head_ref_oid.to_ascii_lowercase();
            let branch_ref = format!("refs/heads/{head_ref_name}");
            validate_reference(&branch_ref)?;
            if !is_oid(&oid) {
                return Err("github-open-pr-head-invalid".to_string());
            }
            (branch_ref, oid)
        };
        let number = record
            .number
            .filter(|number| *number > 0)
            .ok_or_else(|| "github-open-pr-number-missing".to_string())?;
        stale_heads.entry(binding).or_default().insert(number);
    }
    if stale_heads.len() > 10_000 {
        return Err("github-open-pr-count-exceeds-limit".into());
    }
    Ok(stale_heads)
}

/// Resolve exact head OIDs for same-repository GitHub pull requests that are closed or merged.
///
/// The authenticated `gh` client resolves repository identity from the selected repository and
/// returns only bounded JSON. Runtime diagnostics are never reflected to the caller.
pub fn github_closed_pull_request_heads(
    repository_root: &Path,
    timeout_ms: u64,
) -> Result<ClosedPullRequestHeads, String> {
    github_closed_pull_request_heads_with_options(
        repository_root,
        GitWorktreeAuditOptions {
            command_timeout_ms: timeout_ms,
            ..GitWorktreeAuditOptions::default()
        },
    )
}

/// Resolve closed or merged pull-request heads within the caller's worktree bounds.
pub fn github_closed_pull_request_heads_with_options(
    repository_root: &Path,
    options: GitWorktreeAuditOptions,
) -> Result<ClosedPullRequestHeads, String> {
    validate_options(options)?;
    let timeout_ms = options.command_timeout_ms;
    let started = Instant::now();
    let mut heads = ClosedPullRequestHeads::new();

    let mut accept_result = |result: CommandResult| -> Result<(), String> {
        if result.timed_out {
            return Err("github-closed-pr-list-timeout".into());
        }
        if result.stdout_truncated || result.stderr_truncated {
            return Err("github-closed-pr-list-output-truncated".into());
        }
        if result.status_code != Some(0) {
            return Err("github-closed-pr-list-failed".into());
        }
        let stderr = String::from_utf8_lossy(&result.stderr).to_ascii_lowercase();
        if stderr.contains("search")
            && stderr.contains("1000")
            && (stderr.contains("cap") || stderr.contains("limit"))
        {
            return Err("github-closed-pr-list-incomplete".into());
        }
        heads.extend(parse_closed_pull_request_heads(&result.stdout)?);
        if heads.len() > 10_000 {
            return Err("github-closed-pr-count-exceeds-limit".into());
        }
        Ok(())
    };

    let closed_args = vec![
        OsString::from("pr"),
        OsString::from("list"),
        OsString::from("--state"),
        OsString::from("closed"),
        OsString::from("--search"),
        OsString::from("is:unmerged"),
        OsString::from("--limit"),
        OsString::from("10001"),
        OsString::from("--json"),
        OsString::from("headRefName,headRefOid,isCrossRepository,state"),
    ];
    let remaining_ms = timeout_ms.saturating_sub(started.elapsed().as_millis() as u64);
    if remaining_ms == 0 {
        return Err("github-closed-pr-list-timeout".into());
    }
    accept_result(run_bounded_command(
        "gh",
        &closed_args,
        repository_root,
        remaining_ms,
    )?)?;

    let branches = list_worktrees(repository_root, options)?
        .into_iter()
        .filter_map(|worktree| worktree.branch)
        .collect::<BTreeSet<_>>();
    let merged_queries = branches
        .iter()
        .map(|branch| {
            let head = branch
                .strip_prefix("refs/heads/")
                .ok_or_else(|| "git-worktree-porcelain-branch-invalid".to_string())?;
            Ok::<_, String>(vec![
                OsString::from("pr"),
                OsString::from("list"),
                OsString::from("--state"),
                OsString::from("merged"),
                OsString::from("--head"),
                OsString::from(head),
                OsString::from("--limit"),
                OsString::from("10001"),
                OsString::from("--json"),
                OsString::from("headRefName,headRefOid,isCrossRepository,state"),
            ])
        })
        .collect::<Result<Vec<_>, String>>()?;

    const PR_QUERY_CONCURRENCY: usize = 8;
    for query_chunk in merged_queries.chunks(PR_QUERY_CONCURRENCY) {
        let remaining_ms = timeout_ms.saturating_sub(started.elapsed().as_millis() as u64);
        if remaining_ms == 0 {
            return Err("github-closed-pr-list-timeout".into());
        }
        let results = thread::scope(|scope| {
            let workers = query_chunk
                .iter()
                .map(|args| {
                    scope.spawn(move || {
                        run_bounded_command("gh", args, repository_root, remaining_ms)
                    })
                })
                .collect::<Vec<_>>();
            workers
                .into_iter()
                .map(|worker| {
                    worker
                        .join()
                        .map_err(|_| "github-closed-pr-list-worker-failed".to_string())
                        .and_then(|result| result)
                })
                .collect::<Result<Vec<_>, String>>()
        })?;
        for result in results {
            accept_result(result)?;
        }
    }
    let mut open_vetoes = ClosedPullRequestHeads::new();
    let open_queries = branches
        .iter()
        .map(|branch| {
            let head = branch
                .strip_prefix("refs/heads/")
                .ok_or_else(|| "git-worktree-porcelain-branch-invalid".to_string())?;
            Ok::<_, String>(vec![
                OsString::from("pr"),
                OsString::from("list"),
                OsString::from("--state"),
                OsString::from("open"),
                OsString::from("--head"),
                OsString::from(head),
                OsString::from("--limit"),
                OsString::from("10001"),
                OsString::from("--json"),
                OsString::from("headRefName,headRefOid,isCrossRepository,state"),
            ])
        })
        .collect::<Result<Vec<_>, String>>()?;
    for query_chunk in open_queries.chunks(PR_QUERY_CONCURRENCY) {
        let remaining_ms = timeout_ms.saturating_sub(started.elapsed().as_millis() as u64);
        if remaining_ms == 0 {
            return Err("github-closed-pr-list-timeout".into());
        }
        let results = thread::scope(|scope| {
            let workers = query_chunk
                .iter()
                .map(|args| {
                    scope.spawn(move || {
                        run_bounded_command("gh", args, repository_root, remaining_ms)
                    })
                })
                .collect::<Vec<_>>();
            workers
                .into_iter()
                .map(|worker| {
                    worker
                        .join()
                        .map_err(|_| "github-closed-pr-list-worker-failed".to_string())
                        .and_then(|result| result)
                })
                .collect::<Result<Vec<_>, String>>()
        })?;
        for result in results {
            if result.timed_out || result.status_code != Some(0) {
                return Err("github-closed-pr-list-failed".into());
            }
            if result.stdout_truncated || result.stderr_truncated {
                return Err("github-closed-pr-list-output-truncated".into());
            }
            open_vetoes.extend(parse_open_pull_request_heads(&result.stdout)?);
        }
    }
    heads.retain(|binding| !open_vetoes.contains(binding));
    Ok(heads)
}

/// Resolve exact commit membership for the repository's registered worktrees.
///
/// Search results are only discovery hints: every hit is rebound to the exact repository and then
/// verified against the pull request's authoritative commit list. Open membership is retained
/// separately so it can veto every removal authority, including a second completed PR containing
/// the same commit.
pub fn github_pull_request_commit_membership(
    repository_root: &Path,
    options: GitWorktreeAuditOptions,
) -> Result<PullRequestCommitMembership, String> {
    validate_options(options)?;
    let started = Instant::now();
    let remaining = || {
        options
            .command_timeout_ms
            .saturating_sub(started.elapsed().as_millis() as u64)
    };
    let run = |args: &[OsString], reason: &str| -> Result<CommandResult, String> {
        let timeout_ms = remaining();
        if timeout_ms == 0 {
            return Err(format!("{reason}-timeout"));
        }
        let result = run_bounded_command("gh", args, repository_root, timeout_ms)?;
        if result.timed_out {
            return Err(format!("{reason}-timeout"));
        }
        if result.stdout_truncated || result.stderr_truncated {
            return Err(format!("{reason}-output-truncated"));
        }
        if result.status_code != Some(0) {
            return Err(format!("{reason}-failed"));
        }
        Ok(result)
    };

    let repository_result = run(
        &[
            OsString::from("repo"),
            OsString::from("view"),
            OsString::from("--json"),
            OsString::from("nameWithOwner"),
            OsString::from("--jq"),
            OsString::from(".nameWithOwner"),
        ],
        "github-repository-identity",
    )?;
    let repository = command_text(
        &repository_result.stdout,
        "github-repository-identity-not-utf8",
    )?
    .trim();
    let repository_parts = repository.split('/').collect::<Vec<_>>();
    if repository_parts.len() != 2
        || repository_parts.iter().any(|part| part.is_empty())
        || !repository
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_./".contains(&byte))
    {
        return Err("github-repository-identity-invalid".into());
    }

    let heads = list_worktrees(repository_root, options)?
        .into_iter()
        .map(|worktree| worktree.head)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let mut membership = PullRequestCommitMembership::default();
    const SEARCH_CONCURRENCY: usize = 8;
    let mut discovered = Vec::new();
    for chunk in heads.chunks(SEARCH_CONCURRENCY) {
        let timeout_ms = remaining();
        if timeout_ms == 0 {
            return Err("github-pr-commit-search-timeout".into());
        }
        let results = thread::scope(|scope| {
            let workers = chunk
                .iter()
                .map(|head| {
                    let args = vec![
                        OsString::from("search"),
                        OsString::from("prs"),
                        OsString::from(head),
                        OsString::from("--repo"),
                        OsString::from(repository),
                        OsString::from("--limit"),
                        OsString::from("101"),
                        OsString::from("--json"),
                        OsString::from("number,state,repository"),
                    ];
                    scope.spawn(move || {
                        run_bounded_command("gh", &args, repository_root, timeout_ms)
                            .map(|result| (head.clone(), result))
                    })
                })
                .collect::<Vec<_>>();
            workers
                .into_iter()
                .map(|worker| {
                    worker
                        .join()
                        .map_err(|_| "github-pr-commit-search-worker-failed".to_string())
                        .and_then(|result| result)
                })
                .collect::<Result<Vec<_>, String>>()
        })?;
        for (head, result) in results {
            if result.timed_out {
                return Err("github-pr-commit-search-timeout".into());
            }
            if result.stdout_truncated || result.stderr_truncated {
                return Err("github-pr-commit-search-output-truncated".into());
            }
            if result.status_code != Some(0) {
                return Err("github-pr-commit-search-failed".into());
            }
            for candidate in parse_pull_request_search(&result.stdout, repository)? {
                discovered.push((head.clone(), candidate));
            }
        }
    }
    let mut pull_requests = BTreeMap::<(u64, bool), BTreeSet<String>>::new();
    for (head, pull_request) in discovered {
        pull_requests.entry(pull_request).or_default().insert(head);
    }
    for ((number, open), heads) in pull_requests {
        let commits = run(
            &[
                OsString::from("api"),
                OsString::from("--paginate"),
                OsString::from(format!(
                    "repos/{repository}/pulls/{number}/commits?per_page=100"
                )),
                OsString::from("--jq"),
                OsString::from(".[].sha"),
            ],
            "github-pr-commits",
        )?;
        for head in heads {
            if pull_request_contains_commit(&commits.stdout, &head)? {
                if open {
                    membership.open.entry(head).or_default().insert(number);
                } else {
                    membership.completed.insert(head);
                }
            }
        }
    }
    Ok(membership)
}

/// Resolve exact head OIDs for same-repository open pull requests created before an explicit cutoff.
///
/// The cutoff is supplied by the operator; DiskSage never chooses an age threshold implicitly.
/// GitHub state, repository identity, branch name, head OID, and creation timestamp are all
/// refreshed before a plan and before each removal.
pub fn github_stale_open_pull_request_heads(
    repository_root: &Path,
    cutoff_ms: u64,
    timeout_ms: u64,
) -> Result<StaleOpenPullRequestHeads, String> {
    let result = run_bounded_command(
        "gh",
        &[
            OsString::from("pr"),
            OsString::from("list"),
            OsString::from("--state"),
            OsString::from("open"),
            OsString::from("--limit"),
            OsString::from("10001"),
            OsString::from("--json"),
            OsString::from("number,headRefName,headRefOid,isCrossRepository,state,createdAt"),
        ],
        repository_root,
        timeout_ms,
    )?;
    if result.timed_out {
        return Err("github-open-pr-list-timeout".into());
    }
    if result.stdout_truncated || result.stderr_truncated {
        return Err("github-open-pr-list-output-truncated".into());
    }
    if result.status_code != Some(0) {
        return Err("github-open-pr-list-failed".into());
    }
    parse_stale_open_pull_request_heads(&result.stdout, cutoff_ms)
}

#[cfg(test)]
fn git_admin_metadata_blocker(
    status: &crate::provider_sync::FileProviderItemStatus,
) -> Option<&'static str> {
    (!status.is_local_current()).then_some("git-worktree-admin-metadata-not-local-current")
}

#[cfg(all(target_os = "macos", not(coverage)))]
fn check_file_provider_git_metadata(path: &Path) -> Result<Option<&'static str>, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| "git-worktree-admin-metadata-stat-failed".to_string())?;
    let output = match crate::provider_sync::file_providerctl_status(path) {
        Ok(output) => output,
        Err(error) if error == "file-provider-status-command-failed" => return Ok(None),
        Err(error) => return Err(format!("git-worktree-admin-metadata-{error}")),
    };
    let local_current =
        crate::provider_sync::parse_file_providerctl_local_current(&output, metadata.len())
            .map_err(|error| format!("git-worktree-admin-metadata-{error}"))?;
    Ok((!local_current).then_some("git-worktree-admin-metadata-not-local-current"))
}

#[cfg(all(target_os = "macos", not(coverage)))]
fn ensure_git_admin_metadata_local(repository_root: &Path) -> Result<(), String> {
    let git_entry = repository_root.join(".git");
    let mut paths = Vec::new();
    match fs::symlink_metadata(&git_entry) {
        Ok(metadata) if metadata.is_dir() => {
            paths.push(git_entry.join("HEAD"));
            paths.push(git_entry.join("config"));
        }
        Ok(_) => paths.push(git_entry),
        Err(_) => {
            let head = repository_root.join("HEAD");
            if fs::symlink_metadata(&head).is_ok() {
                paths.push(head);
                paths.push(repository_root.join("config"));
            }
        }
    }
    for path in paths {
        if fs::symlink_metadata(&path).is_err() {
            continue;
        }
        if let Some(blocker) = check_file_provider_git_metadata(&path)? {
            return Err(blocker.into());
        }
    }
    Ok(())
}

#[cfg(any(not(target_os = "macos"), coverage))]
fn ensure_git_admin_metadata_local(_repository_root: &Path) -> Result<(), String> {
    Ok(())
}

fn parse_worktree_porcelain(bytes: &[u8]) -> Result<Vec<RawWorktree>, String> {
    let mut entries = Vec::new();
    let mut current = RawWorktreeBuilder::default();
    for field in bytes.split(|byte| *byte == 0) {
        if field.is_empty() {
            if !current.is_empty() {
                entries.push(current.finish()?);
                current = RawWorktreeBuilder::default();
            }
            continue;
        }
        let field = command_text(field, "git-worktree-porcelain-not-utf8")?;
        if let Some(path) = field.strip_prefix("worktree ") {
            if current.path.is_some() {
                return Err("git-worktree-porcelain-duplicate-path".into());
            }
            current.path = Some(PathBuf::from(path));
        } else if let Some(head) = field.strip_prefix("HEAD ") {
            if current.head.is_some() || !is_oid(head) {
                return Err("git-worktree-porcelain-head-invalid".into());
            }
            current.head = Some(head.to_ascii_lowercase());
        } else if let Some(branch) = field.strip_prefix("branch ") {
            if current.branch.is_some() || !branch.starts_with("refs/heads/") {
                return Err("git-worktree-porcelain-branch-invalid".into());
            }
            current.branch = Some(branch.to_string());
        } else if field == "detached" {
            current.detached = true;
        } else if field == "bare" {
            current.bare = true;
        } else if field == "locked" {
            current.locked = true;
        } else if let Some(reason) = field.strip_prefix("locked ") {
            current.locked = true;
            current.lock_reason = Some(reason.to_string());
        } else if field == "prunable" {
            current.prunable = true;
        } else if let Some(reason) = field.strip_prefix("prunable ") {
            current.prunable = true;
            current.prunable_reason = Some(reason.to_string());
        } else {
            return Err("git-worktree-porcelain-field-unknown".into());
        }
    }
    if !current.is_empty() {
        entries.push(current.finish()?);
    }
    if entries.is_empty() {
        return Err("git-worktree-list-empty".into());
    }
    Ok(entries)
}

#[cfg(unix)]
fn allocated_bytes(metadata: &fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    metadata.blocks().saturating_mul(512)
}

#[cfg(not(unix))]
fn allocated_bytes(metadata: &fs::Metadata) -> u64 {
    metadata.len()
}

fn size_evidence(path: &Path, max_entries: u64, timeout_ms: u64) -> GitWorktreeSizeEvidence {
    let started = Instant::now();
    let mut stack = vec![path.to_path_buf()];
    let mut visited_entries = 0u64;
    let mut allocated = 0u64;
    let mut logical = 0u64;
    while let Some(entry) = stack.pop() {
        if started.elapsed() >= Duration::from_millis(timeout_ms) {
            return GitWorktreeSizeEvidence {
                method: "bounded-filesystem-st-blocks-sum".into(),
                evidence_complete: false,
                allocated_bytes: allocated,
                logical_bytes: logical,
                visited_entries,
                error: Some("size-scan-timeout".into()),
            };
        }
        if visited_entries >= max_entries {
            return GitWorktreeSizeEvidence {
                method: "bounded-filesystem-st-blocks-sum".into(),
                evidence_complete: false,
                allocated_bytes: allocated,
                logical_bytes: logical,
                visited_entries,
                error: Some("size-scan-entry-limit".into()),
            };
        }
        visited_entries = visited_entries.saturating_add(1);
        let metadata = match fs::symlink_metadata(&entry) {
            Ok(metadata) => metadata,
            Err(_) => {
                return GitWorktreeSizeEvidence {
                    method: "bounded-filesystem-st-blocks-sum".into(),
                    evidence_complete: false,
                    allocated_bytes: allocated,
                    logical_bytes: logical,
                    visited_entries,
                    error: Some("size-scan-metadata-unavailable".into()),
                };
            }
        };
        allocated = allocated.saturating_add(allocated_bytes(&metadata));
        logical = logical.saturating_add(metadata.len());
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            continue;
        }
        let mut children: Vec<_> = match fs::read_dir(&entry) {
            Ok(children) => match children.collect::<Result<Vec<_>, _>>() {
                Ok(children) => children,
                Err(_) => {
                    return GitWorktreeSizeEvidence {
                        method: "bounded-filesystem-st-blocks-sum".into(),
                        evidence_complete: false,
                        allocated_bytes: allocated,
                        logical_bytes: logical,
                        visited_entries,
                        error: Some("size-scan-entry-unavailable".into()),
                    };
                }
            },
            Err(_) => {
                return GitWorktreeSizeEvidence {
                    method: "bounded-filesystem-st-blocks-sum".into(),
                    evidence_complete: false,
                    allocated_bytes: allocated,
                    logical_bytes: logical,
                    visited_entries,
                    error: Some("size-scan-directory-unreadable".into()),
                };
            }
        };
        children.sort_by_key(|child| child.file_name());
        stack.extend(children.into_iter().rev().map(|child| child.path()));
    }
    GitWorktreeSizeEvidence {
        method: "bounded-filesystem-st-blocks-sum".into(),
        evidence_complete: true,
        allocated_bytes: allocated,
        logical_bytes: logical,
        visited_entries,
        error: None,
    }
}

fn skipped_active_use(reason: &str) -> GitWorktreeActiveUseEvidence {
    GitWorktreeActiveUseEvidence {
        method: "lsof-recursive-pid".into(),
        assessed: false,
        evidence_complete: true,
        active: false,
        observed_pids: Vec::new(),
        results_truncated: false,
        error: Some(reason.into()),
    }
}

#[cfg(unix)]
fn command_contains_path(command: &[u8], path: &[u8], recursive: bool) -> bool {
    if path.is_empty() || command.len() < path.len() {
        return false;
    }
    (0..=command.len() - path.len()).any(|start| {
        let end = start + path.len();
        command[start..end] == *path
            && (start == 0
                || command[start - 1].is_ascii_whitespace()
                || command[start - 1] == b'=')
            && (end == command.len()
                || command[end].is_ascii_whitespace()
                || (recursive && command[end] == b'/'))
    })
}

#[cfg(unix)]
fn ps_command_args() -> Vec<OsString> {
    let mut args = Vec::with_capacity(3);
    #[cfg(target_os = "macos")]
    args.push(OsString::from("-ww"));
    args.extend([OsString::from("-axo"), OsString::from("pid=,command=")]);
    args
}

#[cfg(unix)]
fn is_external_process(pid: u32, probe_pid: u32, disksage_pid: u32, parent_pid: u32) -> bool {
    // The invoking shell necessarily contains the reviewed path in DiskSage's CLI arguments.
    // It is control-plane context, not a consumer of the tree; lsof evidence still detects any
    // descriptor it actually holds. Excluding only the direct parent keeps unrelated processes
    // fail-closed while avoiding a self-created command-line false positive.
    pid != probe_pid && pid != disksage_pid && pid != parent_pid
}

#[cfg(unix)]
pub fn active_use_evidence(
    path: &Path,
    timeout_ms: u64,
    max_pids: usize,
    recursive: bool,
) -> GitWorktreeActiveUseEvidence {
    active_use_evidence_with_command_path(path, path, timeout_ms, max_pids, recursive)
}

/// Probe handles below `object_path` while matching process arguments against `command_path`.
///
/// Atomic staging changes an object's pathname but not an already-started process command. Keeping
/// these identities separate closes that rename boundary without weakening the bounded probes.
#[cfg(unix)]
pub(crate) fn active_use_evidence_with_command_path(
    object_path: &Path,
    command_path: &Path,
    timeout_ms: u64,
    max_pids: usize,
    recursive: bool,
) -> GitWorktreeActiveUseEvidence {
    let command_cwd = object_path.parent().unwrap_or(object_path);
    let method = if recursive {
        "lsof-recursive-pid"
    } else {
        "lsof-file-pid"
    };
    let mut lsof_args = vec![OsString::from("-F0p")];
    if recursive {
        lsof_args.push(OsString::from("+D"));
    }
    lsof_args.push(object_path.as_os_str().to_os_string());
    let result = match run_bounded_command("lsof", &lsof_args, command_cwd, timeout_ms) {
        Ok(result) => result,
        Err(error) => {
            return GitWorktreeActiveUseEvidence {
                method: method.into(),
                assessed: true,
                evidence_complete: false,
                active: false,
                observed_pids: Vec::new(),
                results_truncated: false,
                error: Some(error),
            };
        }
    };
    if result.timed_out {
        return GitWorktreeActiveUseEvidence {
            method: method.into(),
            assessed: true,
            evidence_complete: false,
            active: false,
            observed_pids: Vec::new(),
            results_truncated: false,
            error: Some("active-use-timeout".into()),
        };
    }
    if result.stdout_truncated || result.stderr_truncated {
        return GitWorktreeActiveUseEvidence {
            method: method.into(),
            assessed: true,
            evidence_complete: false,
            active: false,
            observed_pids: Vec::new(),
            results_truncated: true,
            error: Some("active-use-output-truncated".into()),
        };
    }
    let stderr = String::from_utf8_lossy(&result.stderr);
    if !matches!(result.status_code, Some(0) | Some(1))
        || (result.status_code == Some(1) && !stderr.trim().is_empty())
    {
        return GitWorktreeActiveUseEvidence {
            method: method.into(),
            assessed: true,
            evidence_complete: false,
            active: false,
            observed_pids: Vec::new(),
            results_truncated: false,
            error: Some("active-use-command-failed".into()),
        };
    }
    let mut pids = BTreeSet::new();
    for field in result.stdout.split(|byte| *byte == 0) {
        let field = field.strip_prefix(b"\n").unwrap_or(field);
        let Some(raw_pid) = field.strip_prefix(b"p") else {
            continue;
        };
        let Ok(raw_pid) = std::str::from_utf8(raw_pid) else {
            return GitWorktreeActiveUseEvidence {
                method: method.into(),
                assessed: true,
                evidence_complete: false,
                active: false,
                observed_pids: Vec::new(),
                results_truncated: false,
                error: Some("active-use-pid-not-utf8".into()),
            };
        };
        let Ok(pid) = raw_pid.parse::<u32>() else {
            return GitWorktreeActiveUseEvidence {
                method: method.into(),
                assessed: true,
                evidence_complete: false,
                active: false,
                observed_pids: Vec::new(),
                results_truncated: false,
                error: Some("active-use-pid-invalid".into()),
            };
        };
        // The lsof probe itself is noise, but another task in this DiskSage
        // process may legitimately hold the reviewed tree open.
        if pid == result.child_pid {
            continue;
        }
        pids.insert(pid);
    }
    let ps = match run_bounded_command("ps", &ps_command_args(), command_cwd, timeout_ms) {
        Ok(result)
            if !result.timed_out
                && !result.stdout_truncated
                && !result.stderr_truncated
                && result.status_code == Some(0) =>
        {
            result
        }
        _ => {
            return GitWorktreeActiveUseEvidence {
                method: format!("{method}+ps-command"),
                assessed: true,
                evidence_complete: false,
                active: false,
                observed_pids: Vec::new(),
                results_truncated: false,
                error: Some("active-use-process-command-unavailable".into()),
            };
        }
    };
    let path_bytes = command_path.as_os_str().as_encoded_bytes();
    let parent_pid = unsafe { libc::getppid() as u32 };
    for line in ps.stdout.split(|byte| *byte == b'\n') {
        let line = &line[line
            .iter()
            .position(|byte| !byte.is_ascii_whitespace())
            .unwrap_or(line.len())..];
        let split = line.iter().position(|byte| byte.is_ascii_whitespace());
        let Some(split) = split else { continue };
        let Ok(pid) = std::str::from_utf8(&line[..split])
            .ok()
            .and_then(|value| value.parse::<u32>().ok())
            .ok_or(())
        else {
            continue;
        };
        if is_external_process(pid, ps.child_pid, std::process::id(), parent_pid)
            && command_contains_path(&line[split..], path_bytes, recursive)
        {
            pids.insert(pid);
        }
    }
    let results_truncated = pids.len() > max_pids;
    let observed_pids: Vec<_> = pids.into_iter().take(max_pids).collect();
    GitWorktreeActiveUseEvidence {
        method: format!("{method}+ps-command"),
        assessed: true,
        evidence_complete: !results_truncated,
        active: !observed_pids.is_empty(),
        observed_pids,
        results_truncated,
        error: results_truncated.then(|| "active-use-pid-limit".into()),
    }
}

#[cfg(not(unix))]
pub(crate) fn active_use_evidence(
    _path: &Path,
    _timeout_ms: u64,
    _max_pids: usize,
    recursive: bool,
) -> GitWorktreeActiveUseEvidence {
    GitWorktreeActiveUseEvidence {
        method: if recursive {
            "lsof-recursive-pid"
        } else {
            "lsof-file-pid"
        }
        .into(),
        assessed: true,
        evidence_complete: false,
        active: false,
        observed_pids: Vec::new(),
        results_truncated: false,
        error: Some("active-use-platform-unsupported".into()),
    }
}

#[cfg(not(unix))]
pub(crate) fn active_use_evidence_with_command_path(
    object_path: &Path,
    _command_path: &Path,
    timeout_ms: u64,
    max_pids: usize,
    recursive: bool,
) -> GitWorktreeActiveUseEvidence {
    active_use_evidence(object_path, timeout_ms, max_pids, recursive)
}

fn candidate_blockers(input: &ClassificationInput) -> Vec<String> {
    let mut blockers = Vec::new();
    if input.primary {
        blockers.push("primary-worktree".into());
    }
    if input.audit_origin {
        blockers.push("audit-origin-worktree".into());
    }
    if input.bare {
        blockers.push("bare-worktree".into());
    }
    if input.locked {
        blockers.push("worktree-locked".into());
    }
    if input.prunable {
        blockers.push("worktree-prunable-metadata".into());
    }
    if !input.path_valid {
        blockers.push("worktree-path-evidence-incomplete".into());
    }
    match input.status_clean {
        Some(true) => {}
        Some(false) => blockers.push("worktree-dirty".into()),
        None => blockers.push("git-status-evidence-incomplete".into()),
    }
    if input.open_pull_request_commit {
        blockers.push("open-pull-request-commit".into());
    }
    match (
        input.contained_in_reference,
        input.closed_pull_request_head || input.completed_pull_request_commit,
        input.stale_open_pull_request_head,
    ) {
        (Some(true), _, _) | (_, true, _) | (_, _, true) => {}
        (Some(false), false, false) => blockers.push("reference-does-not-contain-head".into()),
        (None, false, false) => blockers.push("reference-containment-evidence-incomplete".into()),
    }
    if input.head_is_retained_tip {
        blockers.push("head-is-retained-tip".into());
    }
    match input.actor_cwd_inside {
        Some(true) => blockers.push("actor-cwd-inside-worktree".into()),
        Some(false) => {}
        None => blockers.push("actor-cwd-evidence-incomplete".into()),
    }
    if !input.size_complete {
        blockers.push("size-evidence-incomplete".into());
    }
    if input.active_use_assessed {
        if !input.active_use_complete {
            blockers.push("active-use-evidence-incomplete".into());
        } else if input.active_use_active {
            blockers.push("active-use-detected".into());
        }
    } else if blockers.is_empty() {
        blockers.push("active-use-evidence-incomplete".into());
    }
    blockers
}

fn evidence_gap_blocker(blocker: &str) -> bool {
    blocker.ends_with("evidence-incomplete") || blocker == "worktree-path-evidence-incomplete"
}

fn disposition(blockers: &[String]) -> GitWorktreeDisposition {
    if blockers.is_empty() {
        GitWorktreeDisposition::RemovalCandidate
    } else if blockers.iter().any(|blocker| evidence_gap_blocker(blocker)) {
        GitWorktreeDisposition::EvidenceGap
    } else {
        GitWorktreeDisposition::Preserve
    }
}

fn hash_field(hasher: &mut blake3::Hasher, value: &str) {
    hasher.update(&(value.len() as u64).to_le_bytes());
    hasher.update(value.as_bytes());
}

fn path_fingerprint(common_dir: &str, path: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage.git-worktree-path\0v1\0");
    hash_field(&mut hasher, common_dir);
    hash_field(&mut hasher, path);
    hasher.finalize().to_hex().to_string()
}

fn retention_reference_set_fingerprint(references: &[GitWorktreeReferenceBinding]) -> String {
    let mut bindings: Vec<_> = references.iter().collect();
    bindings.sort_by(|left, right| {
        left.reference_oid
            .cmp(&right.reference_oid)
            .then_with(|| left.reference_ref.cmp(&right.reference_ref))
    });
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage.git-worktree-retention-references\0v1\0");
    for binding in bindings {
        hash_field(&mut hasher, &binding.reference_ref);
        hash_field(&mut hasher, &binding.reference_oid);
    }
    hasher.finalize().to_hex().to_string()
}

fn removal_authority_fingerprint(
    retention_fingerprint: &str,
    closed_pull_request_heads: &ClosedPullRequestHeads,
) -> String {
    removal_authority_fingerprint_with_open(
        retention_fingerprint,
        closed_pull_request_heads,
        &BTreeMap::new(),
        &BTreeSet::new(),
        &BTreeMap::new(),
        None,
    )
}

fn removal_authority_fingerprint_with_open(
    retention_fingerprint: &str,
    closed_pull_request_heads: &ClosedPullRequestHeads,
    stale_open_pull_request_heads: &StaleOpenPullRequestHeads,
    completed_pull_request_commits: &PullRequestCommits,
    open_pull_request_commits: &BTreeMap<String, BTreeSet<u64>>,
    stale_open_pull_request_cutoff_ms: Option<u64>,
) -> String {
    let mut hasher = blake3::Hasher::new();
    if stale_open_pull_request_heads.is_empty()
        && completed_pull_request_commits.is_empty()
        && open_pull_request_commits.is_empty()
        && stale_open_pull_request_cutoff_ms.is_none()
    {
        hasher.update(b"disksage.git-worktree-removal-authority\0v1\0");
        hash_field(&mut hasher, retention_fingerprint);
        for (branch_ref, oid) in closed_pull_request_heads {
            hash_field(&mut hasher, branch_ref);
            hash_field(&mut hasher, oid);
        }
        return hasher.finalize().to_hex().to_string();
    }
    hasher.update(b"disksage.git-worktree-removal-authority\0v2\0");
    hash_field(&mut hasher, retention_fingerprint);
    hash_field(
        &mut hasher,
        &stale_open_pull_request_cutoff_ms
            .map(|value| value.to_string())
            .unwrap_or_default(),
    );
    for (branch_ref, oid) in closed_pull_request_heads {
        hash_field(&mut hasher, "closed");
        hash_field(&mut hasher, branch_ref);
        hash_field(&mut hasher, oid);
    }
    for ((branch_ref, oid), pull_request_numbers) in stale_open_pull_request_heads {
        hash_field(&mut hasher, "stale-open");
        hash_field(&mut hasher, branch_ref);
        hash_field(&mut hasher, oid);
        for pull_request_number in pull_request_numbers {
            hash_field(&mut hasher, &pull_request_number.to_string());
        }
    }
    for oid in completed_pull_request_commits {
        hash_field(&mut hasher, "completed-commit");
        hash_field(&mut hasher, oid);
    }
    for (oid, pull_request_numbers) in open_pull_request_commits {
        hash_field(&mut hasher, "open-commit-veto");
        hash_field(&mut hasher, oid);
        for pull_request_number in pull_request_numbers {
            hash_field(&mut hasher, &pull_request_number.to_string());
        }
    }
    hasher.finalize().to_hex().to_string()
}

fn entry_fingerprint(
    common_dir: &str,
    reference_set_fingerprint: &str,
    entry: &GitWorktreeAuditEntry,
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage.git-worktree-entry\0v1\0");
    hash_field(&mut hasher, common_dir);
    hash_field(&mut hasher, &entry.path);
    hash_field(&mut hasher, &entry.head);
    hash_field(&mut hasher, entry.branch.as_deref().unwrap_or(""));
    hash_field(&mut hasher, reference_set_fingerprint);
    hasher.update(&entry.size.allocated_bytes.to_le_bytes());
    hasher.update(&entry.size.logical_bytes.to_le_bytes());
    hasher.update(&entry.size.visited_entries.to_le_bytes());
    hasher.update(&entry.status_entry_count.unwrap_or(u64::MAX).to_le_bytes());
    hasher.update(&[
        u8::from(entry.status_clean == Some(true)),
        u8::from(entry.contained_in_reference == Some(true)),
        u8::from(entry.closed_pull_request_head),
        u8::from(entry.completed_pull_request_commit),
        u8::from(entry.open_pull_request_commit),
        u8::from(entry.stale_open_pull_request_head),
        u8::from(entry.head_is_retained_tip),
        u8::from(entry.actor_cwd_inside == Some(true)),
        u8::from(entry.locked),
        u8::from(entry.prunable),
        u8::from(entry.active_use.active),
        u8::from(entry.active_use.evidence_complete),
    ]);
    for pid in &entry.active_use.observed_pids {
        hasher.update(&pid.to_le_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

fn removal_plan_fingerprint(
    common_dir: &str,
    reference_set_fingerprint: &str,
    entries: &[GitWorktreeAuditEntry],
) -> String {
    let mut candidates: Vec<_> = entries
        .iter()
        .filter(|entry| entry.disposition == GitWorktreeDisposition::RemovalCandidate)
        .collect();
    candidates.sort_by(|left, right| left.path.cmp(&right.path));
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage.git-worktree-removal-plan\0v1\0");
    hash_field(&mut hasher, common_dir);
    hash_field(&mut hasher, reference_set_fingerprint);
    hasher.update(&(candidates.len() as u64).to_le_bytes());
    for candidate in candidates {
        hash_field(&mut hasher, &candidate.entry_fingerprint);
        hasher.update(&candidate.size.allocated_bytes.to_le_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

fn canonical_real_directory(path: &Path) -> Result<PathBuf, String> {
    let metadata =
        fs::symlink_metadata(path).map_err(|_| "worktree-path-metadata-unavailable".to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("worktree-path-not-real-directory".into());
    }
    fs::canonicalize(path).map_err(|_| "worktree-path-canonicalize-failed".to_string())
}

fn resolve_reference(
    repository_root: &Path,
    reference: &str,
    timeout_ms: u64,
) -> Result<String, String> {
    let expression = format!("{reference}^{{commit}}");
    let result = run_git(
        repository_root,
        &[
            OsString::from("rev-parse"),
            OsString::from("--verify"),
            OsString::from("--end-of-options"),
            OsString::from(expression),
        ],
        timeout_ms,
        "git-reference-resolve",
    )?;
    if result.status_code != Some(0) {
        return Err("git-reference-resolve-failed".into());
    }
    let oid = command_text(&result.stdout, "git-reference-oid-not-utf8")?
        .trim()
        .to_ascii_lowercase();
    if !is_oid(&oid) {
        return Err("git-reference-oid-invalid".into());
    }
    Ok(oid)
}

fn resolve_references(
    repository_root: &Path,
    references: &[String],
    timeout_ms: u64,
) -> Result<Vec<GitWorktreeReferenceBinding>, String> {
    if references.is_empty() || references.len() > 10_000 {
        return Err("git-worktree-retention-reference-count-invalid".into());
    }
    let mut unique = BTreeSet::new();
    let mut bindings = Vec::new();
    for reference in references {
        validate_reference(reference)?;
        if !unique.insert(reference.clone()) {
            continue;
        }
        bindings.push(GitWorktreeReferenceBinding {
            reference_ref: reference.clone(),
            reference_oid: if is_oid(reference) {
                reference.to_ascii_lowercase()
            } else {
                resolve_reference(repository_root, reference, timeout_ms)?
            },
        });
    }
    bindings.sort_by(|left, right| {
        left.reference_oid
            .cmp(&right.reference_oid)
            .then_with(|| left.reference_ref.cmp(&right.reference_ref))
    });
    Ok(bindings)
}

fn resolve_common_dir(repository_root: &Path, timeout_ms: u64) -> Result<PathBuf, String> {
    let result = run_git(
        repository_root,
        &[
            OsString::from("rev-parse"),
            OsString::from("--path-format=absolute"),
            OsString::from("--git-common-dir"),
        ],
        timeout_ms,
        "git-common-dir-resolve",
    )?;
    if result.status_code != Some(0) {
        return Err("git-common-dir-resolve-failed".into());
    }
    let path = PathBuf::from(command_text(&result.stdout, "git-common-dir-not-utf8")?.trim());
    fs::canonicalize(path).map_err(|_| "git-common-dir-canonicalize-failed".into())
}

fn list_worktrees(
    repository_root: &Path,
    options: GitWorktreeAuditOptions,
) -> Result<Vec<RawWorktree>, String> {
    let result = run_git(
        repository_root,
        &[
            OsString::from("worktree"),
            OsString::from("list"),
            OsString::from("--porcelain"),
            OsString::from("-z"),
        ],
        options.command_timeout_ms,
        "git-worktree-list",
    )?;
    if result.status_code != Some(0) {
        return Err("git-worktree-list-failed".into());
    }
    let entries = parse_worktree_porcelain(&result.stdout)?;
    if entries.len() > options.max_worktrees {
        return Err("git-worktree-list-exceeds-limit".into());
    }
    Ok(entries)
}

const GIT_WORKTREE_LIST_TIMEOUT: &str = "git-worktree-list-timeout";

#[cfg(unix)]
fn open_admin_fallback_file(path: &Path) -> std::io::Result<fs::File> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
    let name = path
        .file_name()
        .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
    let mut directory_options = fs::OpenOptions::new();
    directory_options
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC);
    let directory = directory_options.open(parent)?;
    let name = std::ffi::CString::new(name.as_bytes())
        .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
    let fd = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(unsafe { fs::File::from_raw_fd(fd) })
}

#[cfg(not(unix))]
fn open_admin_fallback_file(path: &Path) -> std::io::Result<fs::File> {
    fs::OpenOptions::new().read(true).open(path)
}

fn read_admin_fallback_file(path: &Path) -> Result<String, String> {
    let file = open_admin_fallback_file(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            "git-worktree-admin-fallback-file-missing".to_string()
        } else {
            "git-worktree-admin-fallback-file-open-failed".to_string()
        }
    })?;
    let metadata = file
        .metadata()
        .map_err(|_| "git-worktree-admin-fallback-file-metadata-failed".to_string())?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("git-worktree-admin-fallback-file-unsafe".into());
    }
    #[cfg(target_os = "macos")]
    {
        use std::os::darwin::fs::MetadataExt;
        const SF_DATALESS: u32 = 0x4000_0000;
        if metadata.st_flags() & SF_DATALESS != 0 {
            return Err("git-worktree-admin-fallback-file-dataless".into());
        }
    }
    if metadata.len() > MAX_ADMIN_FALLBACK_FILE_BYTES {
        return Err("git-worktree-admin-fallback-file-too-large".into());
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_ADMIN_FALLBACK_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "git-worktree-admin-fallback-file-read-failed".to_string())?;
    if bytes.len() as u64 > MAX_ADMIN_FALLBACK_FILE_BYTES {
        return Err("git-worktree-admin-fallback-file-too-large".into());
    }
    String::from_utf8(bytes)
        .map(|value| value.trim().to_string())
        .map_err(|_| "git-worktree-admin-fallback-file-not-utf8".into())
}

/// Recover bounded registration facts when Git's porcelain listing hangs on a malformed entry.
/// The returned records intentionally retain evidence gaps, so no removal operation can use them.
fn admin_fallback_worktrees(
    common_dir: &Path,
    options: GitWorktreeAuditOptions,
) -> (Vec<RawWorktree>, Vec<String>) {
    let admin_dir = common_dir.join("worktrees");
    let mut issues = vec![
        "read-only-git-admin-fallback".into(),
        "git-worktree-remove-not-invoked".into(),
        "git-worktree-prune-not-invoked".into(),
    ];
    let mut entries: Vec<_> = match fs::read_dir(&admin_dir) {
        Ok(read_dir) => read_dir.filter_map(Result::ok).collect(),
        Err(_) => {
            issues.push("git-worktree-admin-fallback-directory-unavailable".into());
            return (Vec::new(), issues);
        }
    };
    entries.sort_by_key(|entry| entry.file_name());
    if entries.len() > options.max_worktrees.min(MAX_ADMIN_FALLBACK_ENTRIES) {
        issues.push("git-worktree-admin-fallback-entry-limit".into());
    }
    entries.truncate(options.max_worktrees.min(MAX_ADMIN_FALLBACK_ENTRIES));
    let mut worktrees = Vec::with_capacity(entries.len());
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        let admin_entry = entry.path();
        let safe_dir = fs::symlink_metadata(&admin_entry)
            .is_ok_and(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink());
        if !safe_dir {
            issues.push(format!("git-worktree-admin-fallback-entry-unsafe:{name}"));
            continue;
        }
        let gitdir = read_admin_fallback_file(&admin_entry.join("gitdir"));
        let path = gitdir
            .as_ref()
            .ok()
            .and_then(|value| Path::new(value).parent())
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(format!("<worktree-admin:{name}>")));
        let head = read_admin_fallback_file(&admin_entry.join("HEAD"))
            .ok()
            .filter(|value| is_oid(value))
            .unwrap_or_else(|| "admin-unknown-head".into());
        let locked = fs::symlink_metadata(admin_entry.join("locked")).is_ok();
        let lock_reason = locked
            .then(|| read_admin_fallback_file(&admin_entry.join("locked")).unwrap_or_default())
            .filter(|value| !value.is_empty());
        let prunable = fs::symlink_metadata(admin_entry.join("prunable")).is_ok();
        let prunable_reason = prunable
            .then(|| read_admin_fallback_file(&admin_entry.join("prunable")).unwrap_or_default())
            .filter(|value| !value.is_empty());
        if gitdir.is_err() {
            issues.push(format!("git-worktree-admin-gitdir-unavailable:{name}"));
        }
        if head == "admin-unknown-head" {
            issues.push(format!("git-worktree-admin-head-unavailable:{name}"));
        }
        worktrees.push(RawWorktree {
            path,
            head,
            branch: None,
            detached: true,
            bare: false,
            locked,
            lock_reason,
            prunable,
            prunable_reason,
            fallback_evidence_incomplete: true,
        });
    }
    (worktrees, issues)
}

fn status_observation(path: &Path, timeout_ms: u64) -> (Option<bool>, Option<u64>) {
    let result = match run_git(
        path,
        &[
            OsString::from("status"),
            OsString::from("--porcelain=v1"),
            OsString::from("-z"),
            OsString::from("--untracked-files=all"),
            OsString::from("--ignore-submodules=none"),
        ],
        timeout_ms,
        "git-status",
    ) {
        Ok(result) => result,
        Err(_) => return (None, None),
    };
    if result.status_code != Some(0) {
        return (None, None);
    }
    let count = result
        .stdout
        .split(|byte| *byte == 0)
        .filter(|field| !field.is_empty())
        .count();
    let Ok(count) = u64::try_from(count) else {
        return (None, None);
    };
    (Some(count == 0), Some(count))
}

fn reachable_commit_set(
    repository_root: &Path,
    references: &[GitWorktreeReferenceBinding],
    timeout_ms: u64,
) -> Result<BTreeSet<String>, String> {
    let mut args = Vec::with_capacity(references.len() + 2);
    args.push(OsString::from("rev-list"));
    for reference in references {
        args.push(OsString::from(&reference.reference_oid));
    }
    args.push(OsString::from("--"));
    let result = run_git(
        repository_root,
        &args,
        timeout_ms,
        "git-retention-reachable-commits",
    )?;
    if result.status_code != Some(0) {
        return Err("git-retention-reachable-commits-failed".into());
    }
    let output = command_text(&result.stdout, "git-retention-reachable-commits-not-utf8")?;
    let mut reachable = BTreeSet::new();
    for line in output.lines() {
        let oid = line.trim().to_ascii_lowercase();
        if !is_oid(&oid) {
            return Err("git-retention-reachable-commit-invalid".into());
        }
        reachable.insert(oid);
        if reachable.len() > MAX_REACHABLE_COMMITS {
            return Err("git-retention-reachable-commit-limit".into());
        }
    }
    if reachable.is_empty() {
        return Err("git-retention-reachable-commit-set-empty".into());
    }
    Ok(reachable)
}

fn containment_observation(head: &str, reachable_commits: &BTreeSet<String>) -> Option<bool> {
    is_oid(head).then(|| reachable_commits.contains(head))
}

fn canonical_actor_cwd() -> Option<PathBuf> {
    std::env::current_dir()
        .ok()
        .and_then(|path| fs::canonicalize(path).ok())
}

/// Audit every linked worktree registered in one Git common directory.
///
/// The selected references are resolved once, then every worktree HEAD is checked against that
/// exact OID set. Exact retained tips are preserved. No fetch, prune, remove, branch deletion, file
/// deletion, or provider operation is performed.
pub fn audit_git_worktrees(
    repository_root: &Path,
    retention_references: &[String],
    options: GitWorktreeAuditOptions,
    generated_at_ms: u64,
) -> Result<GitWorktreeAuditReport, String> {
    audit_git_worktrees_with_closed_pull_request_heads(
        repository_root,
        retention_references,
        &BTreeSet::new(),
        options,
        generated_at_ms,
    )
}

/// Audit worktrees while accepting exact head OIDs from authoritatively closed pull requests as
/// removal authority. Callers must refresh this evidence immediately before execution.
pub fn audit_git_worktrees_with_closed_pull_request_heads(
    repository_root: &Path,
    retention_references: &[String],
    closed_pull_request_heads: &ClosedPullRequestHeads,
    options: GitWorktreeAuditOptions,
    generated_at_ms: u64,
) -> Result<GitWorktreeAuditReport, String> {
    audit_git_worktrees_with_pull_request_heads(
        repository_root,
        retention_references,
        closed_pull_request_heads,
        &BTreeMap::new(),
        None,
        options,
        generated_at_ms,
    )
}

/// Audit worktrees with exact same-repository closed and explicitly stale-open PR head evidence.
pub fn audit_git_worktrees_with_pull_request_heads(
    repository_root: &Path,
    retention_references: &[String],
    closed_pull_request_heads: &ClosedPullRequestHeads,
    stale_open_pull_request_heads: &StaleOpenPullRequestHeads,
    stale_open_pull_request_cutoff_ms: Option<u64>,
    options: GitWorktreeAuditOptions,
    generated_at_ms: u64,
) -> Result<GitWorktreeAuditReport, String> {
    audit_git_worktrees_with_pull_request_membership(
        repository_root,
        retention_references,
        closed_pull_request_heads,
        stale_open_pull_request_heads,
        &PullRequestCommitMembership::default(),
        stale_open_pull_request_cutoff_ms,
        options,
        generated_at_ms,
    )
}

/// Audit worktrees with exact PR-head evidence plus exact commit membership.
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
    validate_options(options)?;
    if closed_pull_request_heads.len() > 10_000
        || closed_pull_request_heads
            .iter()
            .any(|(branch_ref, oid)| validate_reference(branch_ref).is_err() || !is_oid(oid))
    {
        return Err("git-worktree-closed-pull-request-heads-invalid".into());
    }
    if stale_open_pull_request_heads.len() > 10_000
        || stale_open_pull_request_heads
            .iter()
            .any(|((branch_ref, oid), pull_request_numbers)| {
                validate_reference(branch_ref).is_err()
                    || !is_oid(oid)
                    || pull_request_numbers.is_empty()
                    || pull_request_numbers.contains(&0)
            })
        || (!stale_open_pull_request_heads.is_empty()
            && stale_open_pull_request_cutoff_ms.is_none())
    {
        return Err("git-worktree-stale-open-pull-request-heads-invalid".into());
    }
    if pull_request_commits.completed.len() > options.max_worktrees
        || pull_request_commits.open.len() > options.max_worktrees
        || pull_request_commits
            .completed
            .iter()
            .chain(pull_request_commits.open.keys())
            .any(|oid| !is_oid(oid))
    {
        return Err("git-worktree-pull-request-commits-invalid".into());
    }
    if !repository_root.is_absolute() {
        return Err("git-worktree-repository-root-not-absolute".into());
    }
    let repository_root = canonical_real_directory(repository_root)?;
    ensure_git_admin_metadata_local(&repository_root)?;
    let common_dir = resolve_common_dir(&repository_root, options.command_timeout_ms)?;
    let retention_references = resolve_references(
        &repository_root,
        retention_references,
        options.command_timeout_ms,
    )?;
    let retention_fingerprint = retention_reference_set_fingerprint(&retention_references);
    let authority_fingerprint = removal_authority_fingerprint_with_open(
        &retention_fingerprint,
        closed_pull_request_heads,
        stale_open_pull_request_heads,
        &pull_request_commits.completed,
        &pull_request_commits.open,
        stale_open_pull_request_cutoff_ms,
    );
    let retained_tip_oids: BTreeSet<_> = retention_references
        .iter()
        .map(|binding| binding.reference_oid.as_str())
        .collect();
    let reachable_commits = reachable_commit_set(
        &repository_root,
        &retention_references,
        options.command_timeout_ms,
    )?;
    let (raw_worktrees, fallback_issues) = match list_worktrees(&repository_root, options) {
        Ok(raw_worktrees) => (raw_worktrees, Vec::new()),
        Err(error) if error == GIT_WORKTREE_LIST_TIMEOUT => {
            admin_fallback_worktrees(&common_dir, options)
        }
        Err(error) => return Err(error),
    };
    let actor_cwd = canonical_actor_cwd();
    let common_dir_string = common_dir.to_string_lossy().into_owned();
    let audit_origin = repository_root.clone();
    let mut entries = Vec::with_capacity(raw_worktrees.len());
    let mut issues = fallback_issues;

    for (index, raw) in raw_worktrees.into_iter().enumerate() {
        let path_result = canonical_real_directory(&raw.path);
        let path_valid = path_result.is_ok();
        let canonical_path = path_result.as_ref().unwrap_or(&raw.path);
        let path_string = canonical_path.to_string_lossy().into_owned();
        let audit_origin_entry = path_result.as_ref().is_ok_and(|path| path == &audit_origin);
        let actor_cwd_inside = actor_cwd
            .as_ref()
            .map(|cwd| path_result.as_ref().is_ok_and(|path| cwd.starts_with(path)));
        let (status_clean, status_entry_count) = if path_valid && !raw.bare {
            status_observation(canonical_path, options.command_timeout_ms)
        } else {
            (None, None)
        };
        let contained_in_reference = containment_observation(&raw.head, &reachable_commits);
        let closed_pull_request_head = raw.branch.as_ref().is_some_and(|branch_ref| {
            closed_pull_request_heads.contains(&(branch_ref.clone(), raw.head.clone()))
        });
        let stale_open_pull_request_numbers = raw.branch.as_ref().and_then(|branch_ref| {
            stale_open_pull_request_heads.get(&(branch_ref.clone(), raw.head.clone()))
        });
        let stale_open_pull_request_head = stale_open_pull_request_numbers.is_some();
        let completed_pull_request_commit = pull_request_commits.completed.contains(&raw.head);
        let open_pull_request_commit =
            pull_request_commits
                .open
                .get(&raw.head)
                .is_some_and(|pull_request_numbers| {
                    stale_open_pull_request_numbers
                        .is_none_or(|stale_numbers| !pull_request_numbers.is_subset(stale_numbers))
                });
        let head_is_retained_tip = retained_tip_oids.contains(raw.head.as_str());
        let size = if path_valid {
            size_evidence(
                canonical_path,
                options.max_entries_per_worktree,
                options.size_scan_timeout_ms,
            )
        } else {
            GitWorktreeSizeEvidence {
                method: "bounded-filesystem-st-blocks-sum".into(),
                evidence_complete: false,
                allocated_bytes: 0,
                logical_bytes: 0,
                visited_entries: 0,
                error: Some("worktree-path-evidence-incomplete".into()),
            }
        };

        let preliminary = ClassificationInput {
            primary: index == 0,
            audit_origin: audit_origin_entry,
            bare: raw.bare,
            locked: raw.locked,
            prunable: raw.prunable,
            path_valid,
            status_clean,
            contained_in_reference,
            closed_pull_request_head,
            completed_pull_request_commit,
            open_pull_request_commit,
            stale_open_pull_request_head,
            head_is_retained_tip,
            actor_cwd_inside,
            size_complete: size.evidence_complete,
            active_use_assessed: false,
            active_use_complete: false,
            active_use_active: false,
        };
        let preliminary_blockers = candidate_blockers(&preliminary);
        let active_use = if preliminary_blockers
            .iter()
            .all(|blocker| blocker == "active-use-evidence-incomplete")
        {
            active_use_evidence(
                canonical_path,
                options.command_timeout_ms,
                options.max_active_pids,
                true,
            )
        } else {
            skipped_active_use("active-use-not-needed-for-preserved-worktree")
        };
        let classification = ClassificationInput {
            active_use_assessed: active_use.assessed,
            active_use_complete: active_use.evidence_complete,
            active_use_active: active_use.active,
            ..preliminary
        };
        let mut blockers = candidate_blockers(&classification);
        if raw.fallback_evidence_incomplete {
            blockers.push("git-worktree-admin-fallback-evidence-incomplete".into());
        }
        let disposition = disposition(&blockers);
        let mut entry = GitWorktreeAuditEntry {
            path: path_string.clone(),
            path_fingerprint: path_fingerprint(&common_dir_string, &path_string),
            head: raw.head,
            branch: raw.branch,
            detached: raw.detached,
            bare: raw.bare,
            primary: index == 0,
            audit_origin: audit_origin_entry,
            locked: raw.locked,
            lock_reason: raw.lock_reason,
            prunable: raw.prunable,
            prunable_reason: raw.prunable_reason,
            status_clean,
            status_entry_count,
            contained_in_reference,
            closed_pull_request_head,
            completed_pull_request_commit,
            open_pull_request_commit,
            stale_open_pull_request_head,
            head_is_retained_tip,
            actor_cwd_inside,
            size,
            active_use,
            disposition,
            blockers,
            entry_fingerprint: String::new(),
        };
        entry.entry_fingerprint =
            entry_fingerprint(&common_dir_string, &authority_fingerprint, &entry);
        if entry.disposition == GitWorktreeDisposition::EvidenceGap {
            issues.extend(
                entry
                    .blockers
                    .iter()
                    .filter(|blocker| evidence_gap_blocker(blocker))
                    .map(|blocker| format!("{}:{blocker}", entry.path_fingerprint)),
            );
        }
        entries.push(entry);
    }

    let removal_candidate_count = entries
        .iter()
        .filter(|entry| entry.disposition == GitWorktreeDisposition::RemovalCandidate)
        .count();
    let removal_candidate_allocated_bytes = entries
        .iter()
        .filter(|entry| entry.disposition == GitWorktreeDisposition::RemovalCandidate)
        .fold(0u64, |total, entry| {
            total.saturating_add(entry.size.allocated_bytes)
        });
    let preserved_count = entries
        .iter()
        .filter(|entry| entry.disposition == GitWorktreeDisposition::Preserve)
        .count();
    let evidence_gap_count = entries
        .iter()
        .filter(|entry| entry.disposition == GitWorktreeDisposition::EvidenceGap)
        .count();
    let removal_plan_fingerprint =
        removal_plan_fingerprint(&common_dir_string, &authority_fingerprint, &entries);
    let exact_approval_phrase = (removal_candidate_count > 0).then(|| {
        format!(
            "DiskSage stale worktree {removal_candidate_count} {removal_candidate_allocated_bytes} 승인 {removal_plan_fingerprint}"
        )
    });

    Ok(GitWorktreeAuditReport {
        schema_kind: GIT_WORKTREE_AUDIT_SCHEMA_KIND.into(),
        version: 4,
        repository_root: repository_root.to_string_lossy().into_owned(),
        common_dir: common_dir_string,
        generated_at_ms,
        stale_open_pull_request_cutoff_ms,
        retention_references,
        retention_reference_set_fingerprint: retention_fingerprint,
        removal_authority_fingerprint: authority_fingerprint,
        retention_reachable_commit_count: reachable_commits.len(),
        worktree_count: entries.len(),
        removal_candidate_count,
        removal_candidate_allocated_bytes,
        preserved_count,
        evidence_gap_count,
        evidence_complete: issues.is_empty(),
        removal_plan_fingerprint,
        exact_approval_phrase,
        entries,
        issues,
        filesystem_mutation_executed: false,
    })
}

pub fn public_summary(report: &GitWorktreeAuditReport) -> GitWorktreeAuditPublicSummary {
    GitWorktreeAuditPublicSummary {
        schema_kind: report.schema_kind.clone(),
        version: report.version,
        generated_at_ms: report.generated_at_ms,
        stale_open_pull_request_cutoff_ms: report.stale_open_pull_request_cutoff_ms,
        retention_reference_count: report.retention_references.len(),
        retention_reference_set_fingerprint: report.retention_reference_set_fingerprint.clone(),
        removal_authority_fingerprint: report.removal_authority_fingerprint.clone(),
        retention_reachable_commit_count: report.retention_reachable_commit_count,
        worktree_count: report.worktree_count,
        removal_candidate_count: report.removal_candidate_count,
        removal_candidate_allocated_bytes: report.removal_candidate_allocated_bytes,
        preserved_count: report.preserved_count,
        evidence_gap_count: report.evidence_gap_count,
        evidence_complete: report.evidence_complete,
        removal_plan_fingerprint: report.removal_plan_fingerprint.clone(),
        exact_approval_phrase: report.exact_approval_phrase.clone(),
        filesystem_mutation_executed: report.filesystem_mutation_executed,
        local_paths_redacted: true,
        branch_names_redacted: true,
        metadata_semantics: vec![
            "git-worktree-operational-artifact-audit".into(),
            "user-file-production-time-not-inferred".into(),
            "filename-date-not-used".into(),
            "filesystem-created-or-modified-time-not-used-for-removal".into(),
        ],
        notices: vec![
            "read-only-audit".into(),
            "no-fetch-performed".into(),
            "retention-references-bound-to-resolved-oids".into(),
            "retention-reachable-commit-set-bounded".into(),
            "exact-retained-tips-preserved".into(),
            "only-strict-retained-tip-ancestors-can-be-candidates".into(),
            "allocated-bytes-is-filesystem-block-sum-upper-bound".into(),
            "approval-phrase-is-not-execution".into(),
            "no-worktree-prune-remove-or-branch-delete".into(),
            "no-user-file-or-cloud-provider-mutation".into(),
        ],
    }
}

fn valid_hex64(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn exact_removal_approval_phrase(
    candidate_count: usize,
    allocated_bytes: u64,
    plan_fingerprint: &str,
) -> String {
    format!("DiskSage stale worktree {candidate_count} {allocated_bytes} 승인 {plan_fingerprint}")
}

fn validate_audit_for_removal(report: &GitWorktreeAuditReport) -> Result<(), String> {
    if report.schema_kind != GIT_WORKTREE_AUDIT_SCHEMA_KIND
        || report.version != 4
        || report.filesystem_mutation_executed
        || !Path::new(&report.repository_root).is_absolute()
        || !Path::new(&report.common_dir).is_absolute()
        || !valid_hex64(&report.retention_reference_set_fingerprint)
        || !valid_hex64(&report.removal_authority_fingerprint)
        || !valid_hex64(&report.removal_plan_fingerprint)
    {
        return Err("git-worktree-removal-audit-integrity-invalid".into());
    }
    if retention_reference_set_fingerprint(&report.retention_references)
        != report.retention_reference_set_fingerprint
    {
        return Err("git-worktree-removal-reference-binding-mismatch".into());
    }

    let mut candidates = 0usize;
    let mut preserved = 0usize;
    let mut evidence_gaps = 0usize;
    let mut allocated_bytes = 0u64;
    for entry in &report.entries {
        if entry.path_fingerprint != path_fingerprint(&report.common_dir, &entry.path)
            || entry.entry_fingerprint
                != entry_fingerprint(
                    &report.common_dir,
                    &report.removal_authority_fingerprint,
                    entry,
                )
        {
            return Err("git-worktree-removal-entry-integrity-mismatch".into());
        }
        match entry.disposition {
            GitWorktreeDisposition::RemovalCandidate => {
                candidates = candidates.saturating_add(1);
                allocated_bytes = allocated_bytes.saturating_add(entry.size.allocated_bytes);
                if !entry.blockers.is_empty()
                    || entry.primary
                    || entry.audit_origin
                    || entry.bare
                    || entry.locked
                    || entry.prunable
                    || entry.status_clean != Some(true)
                    || entry.status_entry_count != Some(0)
                    || (entry.contained_in_reference != Some(true)
                        && !entry.closed_pull_request_head
                        && !entry.completed_pull_request_commit
                        && !entry.stale_open_pull_request_head)
                    || entry.open_pull_request_commit
                    || entry.head_is_retained_tip
                    || entry.actor_cwd_inside != Some(false)
                    || !entry.size.evidence_complete
                    || !entry.active_use.assessed
                    || !entry.active_use.evidence_complete
                    || entry.active_use.active
                {
                    return Err("git-worktree-removal-candidate-integrity-invalid".into());
                }
            }
            GitWorktreeDisposition::Preserve => preserved = preserved.saturating_add(1),
            GitWorktreeDisposition::EvidenceGap => evidence_gaps = evidence_gaps.saturating_add(1),
        }
    }
    if report.worktree_count != report.entries.len()
        || report.removal_candidate_count != candidates
        || report.removal_candidate_allocated_bytes != allocated_bytes
        || report.preserved_count != preserved
        || report.evidence_gap_count != evidence_gaps
        || report.evidence_complete != (report.issues.is_empty() && evidence_gaps == 0)
        || removal_plan_fingerprint(
            &report.common_dir,
            &report.removal_authority_fingerprint,
            &report.entries,
        ) != report.removal_plan_fingerprint
    {
        return Err("git-worktree-removal-audit-summary-mismatch".into());
    }
    if candidates == 0 || !report.evidence_complete {
        return Err("git-worktree-removal-audit-not-executable".into());
    }
    let expected = exact_removal_approval_phrase(
        candidates,
        allocated_bytes,
        &report.removal_plan_fingerprint,
    );
    if report.exact_approval_phrase.as_deref() != Some(expected.as_str()) {
        return Err("git-worktree-removal-approval-phrase-mismatch".into());
    }
    Ok(())
}

fn removal_approval_id_for(
    report: &GitWorktreeAuditReport,
    approved_at_ms: u64,
    approved_by: &str,
    rationale: &str,
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage.git-worktree-removal-approval\0v1\0");
    for value in [
        report.removal_plan_fingerprint.as_str(),
        report.retention_reference_set_fingerprint.as_str(),
        report.removal_authority_fingerprint.as_str(),
        report.exact_approval_phrase.as_deref().unwrap_or_default(),
        approved_by,
        rationale,
    ] {
        hash_field(&mut hasher, value);
    }
    hasher.update(&(report.removal_candidate_count as u64).to_le_bytes());
    hasher.update(&report.removal_candidate_allocated_bytes.to_le_bytes());
    hasher.update(&approved_at_ms.to_le_bytes());
    hasher.finalize().to_hex().to_string()
}

/// Bind an attributed human decision to one exact, complete audit. This performs no mutation.
pub fn approve_stale_worktree_removal(
    report: &GitWorktreeAuditReport,
    confirmation_exact_approval_phrase: &str,
    approved_at_ms: u64,
    approved_by: &str,
    rationale: &str,
) -> Result<GitWorktreeRemovalApproval, String> {
    validate_audit_for_removal(report)?;
    let expected = report
        .exact_approval_phrase
        .as_deref()
        .ok_or_else(|| "git-worktree-removal-approval-phrase-missing".to_string())?;
    if confirmation_exact_approval_phrase != expected {
        return Err("git-worktree-removal-exact-approval-required".into());
    }
    if approved_at_ms < report.generated_at_ms {
        return Err("git-worktree-removal-approval-predates-audit".into());
    }
    if rationale.trim().len() > MAX_RATIONALE_BYTES {
        return Err("git-worktree-removal-rationale-too-long".into());
    }
    crate::cloud_review::validate_review_attribution(approved_by, rationale)
        .map_err(|_| "git-worktree-removal-human-attribution-invalid".to_string())?;
    let approved_by = approved_by.trim();
    let rationale = rationale.trim();
    Ok(GitWorktreeRemovalApproval {
        version: GIT_WORKTREE_REMOVAL_VERSION,
        approval_id: removal_approval_id_for(report, approved_at_ms, approved_by, rationale),
        removal_plan_fingerprint: report.removal_plan_fingerprint.clone(),
        retention_reference_set_fingerprint: report.retention_reference_set_fingerprint.clone(),
        removal_authority_fingerprint: report.removal_authority_fingerprint.clone(),
        removal_candidate_count: report.removal_candidate_count,
        removal_candidate_allocated_bytes: report.removal_candidate_allocated_bytes,
        exact_approval_phrase: expected.into(),
        approved_at_ms,
        approved_by: approved_by.into(),
        rationale: rationale.into(),
    })
}

fn validate_removal_approval(
    report: &GitWorktreeAuditReport,
    approval: &GitWorktreeRemovalApproval,
    confirmation_exact_approval_phrase: &str,
) -> Result<(), String> {
    validate_audit_for_removal(report)?;
    if approval.version != GIT_WORKTREE_REMOVAL_VERSION
        || !valid_hex64(&approval.approval_id)
        || approval.removal_plan_fingerprint != report.removal_plan_fingerprint
        || approval.retention_reference_set_fingerprint
            != report.retention_reference_set_fingerprint
        || approval.removal_authority_fingerprint != report.removal_authority_fingerprint
        || approval.removal_candidate_count != report.removal_candidate_count
        || approval.removal_candidate_allocated_bytes != report.removal_candidate_allocated_bytes
        || approval.exact_approval_phrase
            != report.exact_approval_phrase.as_deref().unwrap_or_default()
        || approval.exact_approval_phrase != confirmation_exact_approval_phrase
        || approval.approved_at_ms < report.generated_at_ms
        || approval.approval_id
            != removal_approval_id_for(
                report,
                approval.approved_at_ms,
                &approval.approved_by,
                &approval.rationale,
            )
    {
        return Err("git-worktree-removal-approval-integrity-mismatch".into());
    }
    crate::cloud_review::validate_review_attribution(&approval.approved_by, &approval.rationale)
        .map_err(|_| "git-worktree-removal-human-attribution-invalid".to_string())
}

fn live_audit_matches_approved(
    approved: &GitWorktreeAuditReport,
    live: &GitWorktreeAuditReport,
) -> Result<(), String> {
    validate_audit_for_removal(live)?;
    if live.common_dir != approved.common_dir
        || live.repository_root != approved.repository_root
        || live.retention_reference_set_fingerprint != approved.retention_reference_set_fingerprint
        || live.removal_authority_fingerprint != approved.removal_authority_fingerprint
        || live.removal_plan_fingerprint != approved.removal_plan_fingerprint
        || live.removal_candidate_count != approved.removal_candidate_count
        || live.removal_candidate_allocated_bytes != approved.removal_candidate_allocated_bytes
    {
        return Err("git-worktree-removal-live-plan-drift".into());
    }
    Ok(())
}

fn branch_retained(repository_root: &Path, branch: &str, timeout_ms: u64) -> Result<bool, String> {
    validate_reference(branch)?;
    let result = run_git(
        repository_root,
        &[
            OsString::from("show-ref"),
            OsString::from("--verify"),
            OsString::from("--quiet"),
            OsString::from("--"),
            OsString::from(branch),
        ],
        timeout_ms,
        "git-worktree-branch-retention",
    )?;
    Ok(result.status_code == Some(0))
}

fn registration_absent(
    repository_root: &Path,
    removed_path: &Path,
    options: GitWorktreeAuditOptions,
) -> Result<bool, String> {
    let worktrees = list_worktrees(repository_root, options)?;
    Ok(!worktrees.iter().any(|entry| {
        entry.path == removed_path
            || canonical_real_directory(&entry.path)
                .ok()
                .as_deref()
                .is_some_and(|path| path == removed_path)
    }))
}

fn current_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

fn removal_result_id_for(result: &GitWorktreeRemovalResult) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage.git-worktree-removal-result\0v1\0");
    for value in [
        result.approval_id.as_str(),
        result.removal_plan_fingerprint.as_str(),
        result.retention_reference_set_fingerprint.as_str(),
        result.stopped_reason.as_deref().unwrap_or_default(),
    ] {
        hash_field(&mut hasher, value);
    }
    hasher.update(&result.requested_at_ms.to_le_bytes());
    hasher.update(&result.completed_at_ms.to_le_bytes());
    for item in &result.items {
        hash_field(&mut hasher, &item.entry_fingerprint);
        hasher.update(&[
            u8::from(item.removal_attempted),
            u8::from(item.removal_command_succeeded),
            u8::from(item.path_absence_verified),
            u8::from(item.registration_absence_verified),
            u8::from(item.branch_retained == Some(true)),
        ]);
        hash_field(&mut hasher, item.error.as_deref().unwrap_or_default());
    }
    hasher.finalize().to_hex().to_string()
}

fn pending_item(candidate: &GitWorktreeAuditEntry) -> GitWorktreeRemovalItemResult {
    GitWorktreeRemovalItemResult {
        path: candidate.path.clone(),
        path_fingerprint: candidate.path_fingerprint.clone(),
        entry_fingerprint: candidate.entry_fingerprint.clone(),
        head: candidate.head.clone(),
        branch: candidate.branch.clone(),
        allocated_bytes_upper_bound: candidate.size.allocated_bytes,
        removal_attempted: false,
        removal_command_succeeded: false,
        path_absence_verified: false,
        registration_absence_verified: false,
        branch_retained: None,
        error: None,
    }
}

/// Re-audit the full plan and each individual candidate before invoking non-force Git worktree
/// removal. No prune or branch-deletion command is reachable from this function.
pub fn execute_stale_worktree_removal(
    approved_report: &GitWorktreeAuditReport,
    approval: &GitWorktreeRemovalApproval,
    confirmation_exact_approval_phrase: &str,
    options: GitWorktreeAuditOptions,
    requested_at_ms: u64,
) -> Result<GitWorktreeRemovalResult, String> {
    execute_stale_worktree_removal_with_github_closed_pull_requests(
        approved_report,
        approval,
        confirmation_exact_approval_phrase,
        false,
        options,
        requested_at_ms,
    )
}

/// Execute with freshly queried GitHub closed-PR evidence before the plan and every candidate.
pub fn execute_stale_worktree_removal_with_github_closed_pull_requests(
    approved_report: &GitWorktreeAuditReport,
    approval: &GitWorktreeRemovalApproval,
    confirmation_exact_approval_phrase: &str,
    include_closed_pull_requests: bool,
    options: GitWorktreeAuditOptions,
    requested_at_ms: u64,
) -> Result<GitWorktreeRemovalResult, String> {
    execute_stale_worktree_removal_with_github_pull_requests(
        approved_report,
        approval,
        confirmation_exact_approval_phrase,
        include_closed_pull_requests,
        None,
        options,
        requested_at_ms,
    )
}

/// Execute with freshly queried same-repository closed and explicitly stale-open PR evidence.
pub fn execute_stale_worktree_removal_with_github_pull_requests(
    approved_report: &GitWorktreeAuditReport,
    approval: &GitWorktreeRemovalApproval,
    confirmation_exact_approval_phrase: &str,
    include_closed_pull_requests: bool,
    stale_open_pull_request_cutoff_ms: Option<u64>,
    options: GitWorktreeAuditOptions,
    requested_at_ms: u64,
) -> Result<GitWorktreeRemovalResult, String> {
    validate_options(options)?;
    validate_removal_approval(
        approved_report,
        approval,
        confirmation_exact_approval_phrase,
    )?;
    if requested_at_ms < approval.approved_at_ms {
        return Err("git-worktree-removal-request-predates-approval".into());
    }
    let repository_root = PathBuf::from(&approved_report.repository_root);
    let reference_names: Vec<_> = approved_report
        .retention_references
        .iter()
        .map(|binding| binding.reference_ref.clone())
        .collect();
    let audit_live = |observed_at_ms| {
        let evidence = crate::git_worktree_github_evidence::collect(
            &repository_root,
            include_closed_pull_requests,
            stale_open_pull_request_cutoff_ms,
            options,
        )?;
        audit_git_worktrees_with_pull_request_membership(
            &repository_root,
            &reference_names,
            &evidence.closed_heads,
            &evidence.stale_open_heads,
            &evidence.pull_request_commits,
            stale_open_pull_request_cutoff_ms,
            options,
            observed_at_ms,
        )
    };
    let initial_live = audit_live(requested_at_ms)?;
    live_audit_matches_approved(approved_report, &initial_live)?;

    let mut candidates: Vec<_> = initial_live
        .entries
        .iter()
        .filter(|entry| entry.disposition == GitWorktreeDisposition::RemovalCandidate)
        .cloned()
        .collect();
    candidates.sort_by(|left, right| left.path.cmp(&right.path));
    let mut items = Vec::with_capacity(candidates.len());
    let mut stopped_reason = None;

    for (index, candidate) in candidates.iter().enumerate() {
        let live = if index == 0 {
            initial_live.clone()
        } else {
            match audit_live(current_unix_ms()) {
                Ok(report) => report,
                Err(error) => {
                    let mut item = pending_item(candidate);
                    item.error = Some(error);
                    items.push(item);
                    stopped_reason = Some("git-worktree-removal-live-reaudit-failed".into());
                    break;
                }
            }
        };
        if live.retention_reference_set_fingerprint
            != approved_report.retention_reference_set_fingerprint
        {
            let mut item = pending_item(candidate);
            item.error = Some("git-worktree-removal-reference-drift".into());
            items.push(item);
            stopped_reason = Some("git-worktree-removal-reference-drift".into());
            break;
        }
        let live_candidate = live.entries.iter().find(|entry| {
            entry.path_fingerprint == candidate.path_fingerprint
                && entry.disposition == GitWorktreeDisposition::RemovalCandidate
        });
        if !live_candidate.is_some_and(|entry| {
            entry.path == candidate.path
                && entry.head == candidate.head
                && entry.entry_fingerprint == candidate.entry_fingerprint
        }) {
            let mut item = pending_item(candidate);
            item.error = Some("git-worktree-removal-candidate-drift".into());
            items.push(item);
            stopped_reason = Some("git-worktree-removal-candidate-drift".into());
            break;
        }

        let mut item = pending_item(candidate);
        item.removal_attempted = true;
        let removal = run_git(
            &repository_root,
            &[
                OsString::from("worktree"),
                OsString::from("remove"),
                OsString::from("--"),
                OsString::from(&candidate.path),
            ],
            options.command_timeout_ms,
            "git-worktree-remove",
        );
        let command_succeeded = removal
            .as_ref()
            .is_ok_and(|result| result.status_code == Some(0));
        item.removal_command_succeeded = command_succeeded;
        if !command_succeeded {
            item.error = Some(match removal {
                Ok(_) => "git-worktree-remove-command-failed".into(),
                Err(error) => error,
            });
            items.push(item);
            stopped_reason = Some("git-worktree-removal-command-failed".into());
            break;
        }

        item.path_absence_verified = matches!(
            fs::symlink_metadata(&candidate.path),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound
        );
        item.registration_absence_verified =
            registration_absent(&repository_root, Path::new(&candidate.path), options)
                .unwrap_or(false);
        item.branch_retained = candidate.branch.as_deref().map(|branch| {
            branch_retained(&repository_root, branch, options.command_timeout_ms).unwrap_or(false)
        });
        let verified = item.path_absence_verified
            && item.registration_absence_verified
            && item.branch_retained != Some(false);
        if !verified {
            item.error = Some("git-worktree-removal-post-verification-failed".into());
            stopped_reason = Some("git-worktree-removal-post-verification-failed".into());
        }
        items.push(item);
        if stopped_reason.is_some() {
            break;
        }
    }

    let attempted_count = items.iter().filter(|item| item.removal_attempted).count();
    let removed_count = items
        .iter()
        .filter(|item| {
            item.removal_command_succeeded
                && item.path_absence_verified
                && item.registration_absence_verified
                && item.branch_retained != Some(false)
        })
        .count();
    let removed_allocated_bytes_upper_bound = items
        .iter()
        .filter(|item| {
            item.removal_command_succeeded
                && item.path_absence_verified
                && item.registration_absence_verified
                && item.branch_retained != Some(false)
        })
        .fold(0u64, |total, item| {
            total.saturating_add(item.allocated_bytes_upper_bound)
        });
    let verification_complete = stopped_reason.is_none()
        && removed_count == approved_report.removal_candidate_count
        && items.len() == approved_report.removal_candidate_count;
    let completed_at_ms = current_unix_ms().max(requested_at_ms);
    let mut result = GitWorktreeRemovalResult {
        version: GIT_WORKTREE_REMOVAL_VERSION,
        result_id: String::new(),
        approval_id: approval.approval_id.clone(),
        removal_plan_fingerprint: approved_report.removal_plan_fingerprint.clone(),
        retention_reference_set_fingerprint: approved_report
            .retention_reference_set_fingerprint
            .clone(),
        removal_authority_fingerprint: approved_report.removal_authority_fingerprint.clone(),
        requested_at_ms,
        completed_at_ms,
        planned_candidate_count: approved_report.removal_candidate_count,
        attempted_count,
        removed_count,
        planned_allocated_bytes_upper_bound: approved_report.removal_candidate_allocated_bytes,
        removed_allocated_bytes_upper_bound,
        items,
        stopped_reason,
        branch_delete_executed: false,
        git_prune_executed: false,
        filesystem_mutation_executed: attempted_count > 0,
        verification_complete,
        notices: vec![
            "non-force-git-worktree-remove-only".into(),
            "no-git-worktree-prune".into(),
            "no-branch-delete".into(),
            "allocated-bytes-is-pre-removal-upper-bound".into(),
            "partial-results-stop-fail-closed".into(),
            "no-user-file-or-cloud-provider-mutation".into(),
        ],
    };
    result.result_id = removal_result_id_for(&result);
    Ok(result)
}

fn path_overlaps(left: &Path, right: &Path) -> bool {
    left.starts_with(right) || right.starts_with(left)
}

fn absolute_without_parent(path: &Path) -> bool {
    path.is_absolute()
        && !path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
}

/// Prepare private immutable-record storage outside the repository, common Git directory, and all
/// audited worktrees, including when the app-data directory does not exist yet.
pub fn prepare_worktree_record_directory(
    app_data_dir: &Path,
    report: &GitWorktreeAuditReport,
    directory_name: &str,
) -> Result<PathBuf, String> {
    validate_audit_for_removal(report)?;
    let mut components = Path::new(directory_name).components();
    if !absolute_without_parent(app_data_dir)
        || !matches!(components.next(), Some(std::path::Component::Normal(_)))
        || components.next().is_some()
    {
        return Err("git-worktree-removal-record-path-invalid".into());
    }
    let mut forbidden = vec![
        PathBuf::from(&report.repository_root),
        PathBuf::from(&report.common_dir),
    ];
    forbidden.extend(
        report
            .entries
            .iter()
            .map(|entry| PathBuf::from(&entry.path)),
    );

    let existing_ancestor = app_data_dir
        .ancestors()
        .find(|candidate| candidate.exists())
        .ok_or_else(|| "git-worktree-removal-record-parent-unavailable".to_string())?;
    let canonical_existing_ancestor = fs::canonicalize(existing_ancestor)
        .map_err(|_| "git-worktree-removal-record-parent-unavailable".to_string())?;
    let missing_suffix = app_data_dir
        .strip_prefix(existing_ancestor)
        .map_err(|_| "git-worktree-removal-record-parent-unavailable".to_string())?;
    let prospective_record_dir = canonical_existing_ancestor
        .join(missing_suffix)
        .join(directory_name);
    if forbidden
        .iter()
        .any(|path| path_overlaps(&prospective_record_dir, path))
    {
        return Err("git-worktree-removal-record-dir-overlaps-repository".into());
    }

    fs::create_dir_all(app_data_dir)
        .map_err(|_| "git-worktree-removal-record-parent-create-failed".to_string())?;
    let canonical_app_data_dir = fs::canonicalize(app_data_dir)
        .map_err(|_| "git-worktree-removal-record-parent-unavailable".to_string())?;
    let record_dir = canonical_app_data_dir.join(directory_name);
    match fs::symlink_metadata(&record_dir) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err("git-worktree-removal-record-dir-not-real-directory".into());
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(&record_dir)
                .map_err(|_| "git-worktree-removal-record-dir-create-failed".to_string())?;
        }
        Err(_) => return Err("git-worktree-removal-record-dir-unavailable".into()),
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&record_dir, fs::Permissions::from_mode(0o700))
            .map_err(|_| "git-worktree-removal-record-dir-permissions-failed".to_string())?;
    }
    let canonical_record_dir = fs::canonicalize(&record_dir)
        .map_err(|_| "git-worktree-removal-record-dir-unavailable".to_string())?;
    if forbidden
        .iter()
        .any(|path| path_overlaps(&canonical_record_dir, path))
    {
        return Err("git-worktree-removal-record-dir-overlaps-repository".into());
    }
    Ok(canonical_record_dir)
}

/// Persist an approval or result using create-new semantics and make the completed file read-only.
pub fn write_immutable_worktree_record<T: serde::Serialize>(
    record_dir: &Path,
    filename: &str,
    value: &T,
) -> Result<PathBuf, String> {
    if !absolute_without_parent(record_dir)
        || filename.is_empty()
        || filename.contains('/')
        || filename.contains('\\')
        || !filename.ends_with(".json")
    {
        return Err("git-worktree-removal-record-path-invalid".into());
    }
    let directory = fs::symlink_metadata(record_dir)
        .map_err(|_| "git-worktree-removal-record-dir-unavailable".to_string())?;
    if directory.file_type().is_symlink() || !directory.is_dir() {
        return Err("git-worktree-removal-record-dir-not-real-directory".into());
    }
    let path = record_dir.join(filename);
    let encoded = serde_json::to_vec_pretty(value)
        .map_err(|_| "git-worktree-removal-record-serialization-failed".to_string())?;
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&path)
        .map_err(|_| "git-worktree-removal-record-create-failed".to_string())?;
    file.write_all(&encoded)
        .and_then(|_| file.write_all(b"\n"))
        .and_then(|_| file.sync_all())
        .map_err(|_| "git-worktree-removal-record-write-failed".to_string())?;
    let mut permissions = file
        .metadata()
        .map_err(|_| "git-worktree-removal-record-metadata-failed".to_string())?
        .permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&path, permissions)
        .map_err(|_| "git-worktree-removal-record-permissions-failed".to_string())?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn oid(character: char) -> String {
        std::iter::repeat_n(character, 40).collect()
    }

    #[test]
    fn github_remote_resolution_accepts_renamed_unique_remote_and_rejects_ambiguity() {
        let temp = tempfile::tempdir().unwrap();
        assert!(Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(temp.path())
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .args([
                "remote",
                "add",
                "upstream",
                "https://github.com/ContextualWisdomLab/example.git",
            ])
            .current_dir(temp.path())
            .status()
            .unwrap()
            .success());
        assert_eq!(
            matching_github_remote_name(temp.path(), "ContextualWisdomLab/example", 5_000).unwrap(),
            "upstream"
        );
        assert!(Command::new("git")
            .args([
                "remote",
                "add",
                "origin",
                "git@github.com:contextualwisdomlab/example.git",
            ])
            .current_dir(temp.path())
            .status()
            .unwrap()
            .success());
        assert_eq!(
            matching_github_remote_name(temp.path(), "ContextualWisdomLab/example", 5_000),
            Err("github-default-branch-remote-not-unique".into())
        );
    }

    #[cfg(unix)]
    #[test]
    fn active_use_includes_process_command_paths_not_held_open() {
        let temp = tempfile::tempdir().unwrap();
        let marker = temp.path().join("npx-environment");
        fs::create_dir(&marker).unwrap();
        let mut child = Command::new("sh")
            .args(["-c", "sleep 20 & wait", marker.to_str().unwrap()])
            .spawn()
            .unwrap();

        let evidence = active_use_evidence(&marker, 5_000, 64, true);
        let _ = child.kill();
        let _ = child.wait();

        assert!(evidence.evidence_complete, "{evidence:?}");
        assert!(evidence.active, "{evidence:?}");
        assert!(evidence.observed_pids.contains(&child.id()), "{evidence:?}");
        assert!(!command_contains_path(
            b"tool /cache/env-old",
            b"/cache/env",
            true
        ));
        assert!(command_contains_path(
            b"tool /cache/env with spaces/child",
            b"/cache/env with spaces",
            true,
        ));
    }

    #[cfg(unix)]
    #[test]
    fn active_use_pid_filter_excludes_probe_and_disksage_processes() {
        assert!(!is_external_process(41, 41, 42, 43));
        assert!(!is_external_process(42, 41, 42, 43));
        assert!(!is_external_process(43, 41, 42, 43));
        assert!(is_external_process(44, 41, 42, 43));
    }

    #[cfg(unix)]
    #[test]
    fn staged_probe_keeps_original_command_path_identity() {
        let temp = tempfile::tempdir().unwrap();
        let original = temp.path().join("approved-cache");
        let staged = temp.path().join(".disksage-stage").join("approved-cache");
        fs::create_dir_all(&staged).unwrap();
        let mut child = Command::new("sh")
            .args(["-c", "sleep 20 & wait", original.to_str().unwrap()])
            .spawn()
            .unwrap();

        let evidence = active_use_evidence_with_command_path(&staged, &original, 5_000, 64, true);
        let _ = child.kill();
        let _ = child.wait();

        assert!(evidence.evidence_complete, "{evidence:?}");
        assert!(evidence.active, "{evidence:?}");
        assert!(evidence.observed_pids.contains(&child.id()), "{evidence:?}");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn active_use_includes_path_beyond_default_macos_ps_width() {
        let temp = tempfile::tempdir().unwrap();
        let marker = temp.path().join("long-command-environment");
        fs::create_dir(&marker).unwrap();
        let padding = "x".repeat(512);
        let mut child = Command::new("sh")
            .args([
                "-c",
                "sleep 20 & wait",
                padding.as_str(),
                marker.to_str().unwrap(),
            ])
            .spawn()
            .unwrap();

        let evidence = active_use_evidence(&marker, 5_000, 64, true);
        let _ = child.kill();
        let _ = child.wait();

        assert!(evidence.evidence_complete, "{evidence:?}");
        assert!(evidence.active, "{evidence:?}");
        assert!(evidence.observed_pids.contains(&child.id()), "{evidence:?}");
    }

    #[test]
    fn closed_pull_request_evidence_binds_same_repository_branch_and_head() {
        let json = format!(
            r#"[
              {{"headRefName":"closed-local","headRefOid":"{}","isCrossRepository":false,"state":"CLOSED"}},
              {{"headRefName":"merged","headRefOid":"{}","isCrossRepository":false,"state":"MERGED"}},
              {{"headRefName":"forked","headRefOid":"{}","isCrossRepository":true,"state":"CLOSED"}}
            ]"#,
            oid('a'),
            oid('b'),
            oid('c'),
        );
        assert_eq!(
            parse_closed_pull_request_heads(json.as_bytes()).unwrap(),
            BTreeSet::from([
                ("refs/heads/closed-local".into(), oid('a')),
                ("refs/heads/merged".into(), oid('b')),
            ])
        );
    }

    #[test]
    fn pull_request_commit_discovery_is_repository_bound_and_exact() {
        let json = br#"[
          {"number":1370,"state":"merged","repository":{"name":"disksage","nameWithOwner":"ContextualWisdomLab/disksage"}},
          {"number":1454,"state":"closed","repository":{"name":"disksage","nameWithOwner":"ContextualWisdomLab/disksage"}}
        ]"#;
        assert_eq!(
            parse_pull_request_search(json, "ContextualWisdomLab/disksage").unwrap(),
            vec![(1370, false), (1454, false)]
        );
        assert_eq!(
            parse_pull_request_search(json, "ContextualWisdomLab/other").unwrap_err(),
            "github-pr-commit-search-repository-mismatch"
        );
        assert!(pull_request_contains_commit(
            format!("{}\n{}\n", oid('a'), oid('b')).as_bytes(),
            &oid('b')
        )
        .unwrap());
    }

    #[test]
    fn merged_pull_request_evidence_binds_exact_branch_and_head() {
        let json = format!(
            r#"[{{"headRefName":"merged-local","headRefOid":"{}","isCrossRepository":false,"state":"MERGED"}}]"#,
            oid('a')
        );
        assert_eq!(
            parse_closed_pull_request_heads(json.as_bytes()).unwrap(),
            BTreeSet::from([("refs/heads/merged-local".into(), oid('a'))])
        );
    }

    #[test]
    fn stale_open_pull_request_evidence_requires_valid_timestamp_and_filters_explicit_cutoff() {
        let json = format!(
            r#"[
              {{"number":1,"headRefName":"old-local","headRefOid":"{}","isCrossRepository":false,"state":"OPEN","createdAt":"2026-01-01T00:00:00Z"}},
              {{"number":2,"headRefName":"new-local","headRefOid":"{}","isCrossRepository":false,"state":"OPEN","createdAt":"2026-08-28T00:00:00Z"}},
              {{"number":3,"headRefName":"forked","headRefOid":"{}","isCrossRepository":true,"state":"OPEN","createdAt":"2020-01-01T00:00:00Z"}},
              {{"number":4,"headRefName":"closed","headRefOid":"{}","isCrossRepository":false,"state":"CLOSED","createdAt":"2020-01-01T00:00:00Z"}}
            ]"#,
            oid('a'),
            oid('b'),
            oid('c'),
            oid('d'),
        );
        let cutoff = parse_github_timestamp_ms("2026-08-01T00:00:00Z").unwrap();
        assert_eq!(
            parse_stale_open_pull_request_heads(json.as_bytes(), cutoff).unwrap(),
            BTreeMap::from([(
                ("refs/heads/old-local".into(), oid('a')),
                BTreeSet::from([1])
            )])
        );
        assert!(parse_github_timestamp_ms("2026-02-30T00:00:00Z").is_none());
        assert!(parse_github_timestamp_ms("2026-01-01T00:00:00+00:00").is_none());
    }

    #[test]
    fn stale_open_pull_request_evidence_fails_closed_when_timestamp_is_missing() {
        let json = format!(
            r#"[{{"headRefName":"old-local","headRefOid":"{}","isCrossRepository":false,"state":"OPEN"}}]"#,
            oid('a')
        );
        assert_eq!(
            parse_stale_open_pull_request_heads(json.as_bytes(), u64::MAX).unwrap_err(),
            "github-open-pr-created-at-missing"
        );
    }

    fn executable_report() -> GitWorktreeAuditReport {
        let common_dir = "/tmp/repository/.git".to_string();
        let references = vec![GitWorktreeReferenceBinding {
            reference_ref: "main".into(),
            reference_oid: oid('a'),
        }];
        let reference_fingerprint = retention_reference_set_fingerprint(&references);
        let authority_fingerprint =
            removal_authority_fingerprint(&reference_fingerprint, &BTreeSet::new());
        let mut entry = GitWorktreeAuditEntry {
            path: "/tmp/secondary".into(),
            path_fingerprint: path_fingerprint(&common_dir, "/tmp/secondary"),
            head: oid('b'),
            branch: Some("refs/heads/merged".into()),
            detached: false,
            bare: false,
            primary: false,
            audit_origin: false,
            locked: false,
            lock_reason: None,
            prunable: false,
            prunable_reason: None,
            status_clean: Some(true),
            status_entry_count: Some(0),
            contained_in_reference: Some(true),
            closed_pull_request_head: false,
            completed_pull_request_commit: false,
            open_pull_request_commit: false,
            stale_open_pull_request_head: false,
            head_is_retained_tip: false,
            actor_cwd_inside: Some(false),
            size: GitWorktreeSizeEvidence {
                method: "bounded-filesystem-st-blocks-sum".into(),
                evidence_complete: true,
                allocated_bytes: 4_096,
                logical_bytes: 100,
                visited_entries: 3,
                error: None,
            },
            active_use: GitWorktreeActiveUseEvidence {
                method: "lsof-recursive-pid".into(),
                assessed: true,
                evidence_complete: true,
                active: false,
                observed_pids: Vec::new(),
                results_truncated: false,
                error: None,
            },
            disposition: GitWorktreeDisposition::RemovalCandidate,
            blockers: Vec::new(),
            entry_fingerprint: String::new(),
        };
        entry.entry_fingerprint = entry_fingerprint(&common_dir, &authority_fingerprint, &entry);
        let entries = vec![entry];
        let plan_fingerprint =
            removal_plan_fingerprint(&common_dir, &authority_fingerprint, &entries);
        GitWorktreeAuditReport {
            schema_kind: GIT_WORKTREE_AUDIT_SCHEMA_KIND.into(),
            version: 4,
            repository_root: "/tmp/repository".into(),
            common_dir,
            generated_at_ms: 10,
            stale_open_pull_request_cutoff_ms: None,
            retention_references: references,
            retention_reference_set_fingerprint: reference_fingerprint,
            removal_authority_fingerprint: authority_fingerprint,
            retention_reachable_commit_count: 2,
            worktree_count: 1,
            removal_candidate_count: 1,
            removal_candidate_allocated_bytes: 4_096,
            preserved_count: 0,
            evidence_gap_count: 0,
            evidence_complete: true,
            removal_plan_fingerprint: plan_fingerprint.clone(),
            exact_approval_phrase: Some(exact_removal_approval_phrase(1, 4_096, &plan_fingerprint)),
            entries,
            issues: Vec::new(),
            filesystem_mutation_executed: false,
        }
    }

    #[cfg(all(unix, not(coverage)))]
    fn git(cwd: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .env("GIT_AUTHOR_NAME", "DiskSage Test")
            .env("GIT_AUTHOR_EMAIL", "disksage@example.invalid")
            .env("GIT_COMMITTER_NAME", "DiskSage Test")
            .env("GIT_COMMITTER_EMAIL", "disksage@example.invalid")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[cfg(all(unix, not(coverage)))]
    fn temporary_repository() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let repository = temp.path().join("repository");
        let secondary = temp.path().join("secondary");
        fs::create_dir(&repository).unwrap();
        git(&repository, &["init", "-b", "main"]);
        fs::write(repository.join("evidence.txt"), b"first\n").unwrap();
        git(&repository, &["add", "evidence.txt"]);
        git(&repository, &["commit", "-m", "first"]);
        fs::write(repository.join("evidence.txt"), b"second\n").unwrap();
        git(&repository, &["commit", "-am", "second"]);
        git(&repository, &["branch", "merged", "HEAD~1"]);
        git(
            &repository,
            &["worktree", "add", secondary.to_str().unwrap(), "merged"],
        );
        (temp, repository, secondary)
    }

    #[cfg(all(unix, not(coverage)))]
    #[test]
    fn exact_closed_pull_request_branch_and_head_authorize_clean_unmerged_worktree() {
        let temp = tempfile::tempdir().unwrap();
        let repository = temp.path().join("repository");
        let secondary = temp.path().join("closed-pr");
        fs::create_dir(&repository).unwrap();
        git(&repository, &["init", "-b", "main"]);
        fs::write(repository.join("evidence.txt"), b"main\n").unwrap();
        git(&repository, &["add", "evidence.txt"]);
        git(&repository, &["commit", "-m", "main"]);
        git(&repository, &["branch", "closed-pr"]);
        git(
            &repository,
            &["worktree", "add", secondary.to_str().unwrap(), "closed-pr"],
        );
        fs::write(secondary.join("closed.txt"), b"closed\n").unwrap();
        git(&secondary, &["add", "closed.txt"]);
        git(&secondary, &["commit", "-m", "closed-only"]);
        let head = command_text(
            &run_git(
                &secondary,
                &[OsString::from("rev-parse"), OsString::from("HEAD")],
                5_000,
                "test-rev-parse",
            )
            .unwrap()
            .stdout,
            "test-head-not-utf8",
        )
        .unwrap()
        .trim()
        .to_string();
        let closed = BTreeSet::from([("refs/heads/closed-pr".into(), head)]);

        let report = audit_git_worktrees_with_closed_pull_request_heads(
            &repository,
            &["refs/heads/main".into()],
            &closed,
            GitWorktreeAuditOptions::default(),
            42,
        )
        .unwrap();
        let entry = report
            .entries
            .iter()
            .find(|entry| entry.branch.as_deref() == Some("refs/heads/closed-pr"))
            .unwrap();
        assert_eq!(entry.contained_in_reference, Some(false));
        assert!(entry.closed_pull_request_head);
        assert_eq!(entry.disposition, GitWorktreeDisposition::RemovalCandidate);
    }
    #[cfg(all(unix, not(coverage)))]
    #[test]
    fn detached_intermediate_completed_commit_is_candidate_unless_an_open_pr_contains_it() {
        let temp = tempfile::tempdir().unwrap();
        let repository = temp.path().join("repository");
        let secondary = temp.path().join("detached-pr");
        fs::create_dir(&repository).unwrap();
        git(&repository, &["init", "-b", "main"]);
        fs::write(repository.join("main.txt"), b"main\n").unwrap();
        git(&repository, &["add", "main.txt"]);
        git(&repository, &["commit", "-m", "main"]);
        git(&repository, &["branch", "pull-request"]);
        git(
            &repository,
            &[
                "worktree",
                "add",
                secondary.to_str().unwrap(),
                "pull-request",
            ],
        );
        fs::write(secondary.join("first.txt"), b"first\n").unwrap();
        git(&secondary, &["add", "first.txt"]);
        git(&secondary, &["commit", "-m", "first"]);
        let intermediate = command_text(
            &run_git(
                &secondary,
                &[OsString::from("rev-parse"), OsString::from("HEAD")],
                5_000,
                "test-rev-parse",
            )
            .unwrap()
            .stdout,
            "test-head-not-utf8",
        )
        .unwrap()
        .trim()
        .to_string();
        fs::write(secondary.join("second.txt"), b"second\n").unwrap();
        git(&secondary, &["add", "second.txt"]);
        git(&secondary, &["commit", "-m", "second"]);
        git(&secondary, &["checkout", "--detach", &intermediate]);

        let completed = PullRequestCommitMembership {
            completed: BTreeSet::from([intermediate.clone()]),
            open: BTreeMap::new(),
            ..PullRequestCommitMembership::default()
        };
        let report = audit_git_worktrees_with_pull_request_membership(
            &repository,
            &["refs/heads/main".into()],
            &BTreeSet::new(),
            &BTreeMap::new(),
            &completed,
            None,
            GitWorktreeAuditOptions::default(),
            42,
        )
        .unwrap();
        let entry = report.entries.iter().find(|entry| entry.detached).unwrap();
        assert_eq!(entry.contained_in_reference, Some(false));
        assert!(entry.completed_pull_request_commit);
        assert_eq!(entry.disposition, GitWorktreeDisposition::RemovalCandidate);

        let open_veto = PullRequestCommitMembership {
            completed: BTreeSet::from([intermediate.clone()]),
            open: BTreeMap::from([(intermediate, BTreeSet::from([1]))]),
            ..PullRequestCommitMembership::default()
        };
        let report = audit_git_worktrees_with_pull_request_membership(
            &repository,
            &["refs/heads/main".into()],
            &BTreeSet::new(),
            &BTreeMap::new(),
            &open_veto,
            None,
            GitWorktreeAuditOptions::default(),
            43,
        )
        .unwrap();
        let entry = report.entries.iter().find(|entry| entry.detached).unwrap();
        assert!(entry.open_pull_request_commit);
        assert!(entry
            .blockers
            .iter()
            .any(|value| value == "open-pull-request-commit"));
        assert_eq!(entry.disposition, GitWorktreeDisposition::Preserve);
    }

    #[test]
    fn parses_nul_porcelain_and_preserves_lock_and_prunable_reasons() {
        let encoded = format!(
            "worktree /tmp/main\0HEAD {}\0branch refs/heads/develop\0\0worktree /tmp/locked\0HEAD {}\0detached\0locked agent\0\0worktree /tmp/gone\0HEAD {}\0prunable missing\0\0",
            oid('a'),
            oid('b'),
            oid('c')
        );
        let entries = parse_worktree_porcelain(encoded.as_bytes()).unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].branch.as_deref(), Some("refs/heads/develop"));
        assert!(entries[1].detached);
        assert!(entries[1].locked);
        assert_eq!(entries[1].lock_reason.as_deref(), Some("agent"));
        assert!(entries[2].prunable);
        assert_eq!(entries[2].prunable_reason.as_deref(), Some("missing"));
    }

    #[test]
    fn rejects_unknown_or_malformed_porcelain_fields() {
        let unknown = format!("worktree /tmp/main\0HEAD {}\0future field\0\0", oid('a'));
        assert!(parse_worktree_porcelain(unknown.as_bytes()).is_err());
        let missing_head = b"worktree /tmp/main\0\0";
        assert!(parse_worktree_porcelain(missing_head).is_err());
        let invalid_head = b"worktree /tmp/main\0HEAD nope\0\0";
        assert!(parse_worktree_porcelain(invalid_head).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn admin_fallback_surfaces_stale_registration_without_creating_removal_candidates() {
        let temp = tempfile::tempdir().unwrap();
        let common_dir = temp.path().join(".git");
        let admin = common_dir.join("worktrees").join("stale");
        fs::create_dir_all(&admin).unwrap();
        fs::write(admin.join("gitdir"), "/missing-worktree/.git\n").unwrap();
        fs::write(admin.join("HEAD"), "not-a-head\n").unwrap();
        let (entries, issues) =
            admin_fallback_worktrees(&common_dir, GitWorktreeAuditOptions::default());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, PathBuf::from("/missing-worktree"));
        assert_eq!(entries[0].head, "admin-unknown-head");
        assert!(entries[0].fallback_evidence_incomplete);
        assert_eq!(
            disposition(&["git-worktree-admin-fallback-evidence-incomplete".into()]),
            GitWorktreeDisposition::EvidenceGap
        );
        assert!(issues
            .iter()
            .any(|issue| issue == "read-only-git-admin-fallback"));
        assert!(issues
            .iter()
            .any(|issue| issue == "git-worktree-admin-head-unavailable:stale"));
    }

    #[cfg(unix)]
    #[test]
    fn admin_fallback_with_valid_oid_and_clean_path_stays_evidence_gap() {
        let temp = tempfile::tempdir().unwrap();
        let common_dir = temp.path().join(".git");
        let worktree = temp.path().join("linked");
        fs::create_dir_all(&worktree).unwrap();
        let admin = common_dir.join("worktrees").join("linked");
        fs::create_dir_all(&admin).unwrap();
        fs::write(
            admin.join("gitdir"),
            format!("{}/.git\n", worktree.display()),
        )
        .unwrap();
        fs::write(admin.join("HEAD"), format!("{}\n", oid('a'))).unwrap();

        let (entries, _) =
            admin_fallback_worktrees(&common_dir, GitWorktreeAuditOptions::default());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, worktree);
        assert!(is_oid(&entries[0].head));
        assert!(entries[0].path.is_dir());
        assert!(entries[0].fallback_evidence_incomplete);
        assert_eq!(
            disposition(&["git-worktree-admin-fallback-evidence-incomplete".into()]),
            GitWorktreeDisposition::EvidenceGap
        );
    }

    #[cfg(unix)]
    #[test]
    fn admin_fallback_file_rejects_symlinks_and_bounds_reads() {
        let temp = tempfile::tempdir().unwrap();
        let symlink = temp.path().join("symlink");
        fs::write(temp.path().join("target"), b"safe").unwrap();
        std::os::unix::fs::symlink(temp.path().join("target"), &symlink).unwrap();
        assert_eq!(
            read_admin_fallback_file(&symlink).unwrap_err(),
            "git-worktree-admin-fallback-file-open-failed"
        );

        let oversized = temp.path().join("oversized");
        fs::write(
            &oversized,
            vec![b'x'; (MAX_ADMIN_FALLBACK_FILE_BYTES + 1) as usize],
        )
        .unwrap();
        assert_eq!(
            read_admin_fallback_file(&oversized).unwrap_err(),
            "git-worktree-admin-fallback-file-too-large"
        );
    }

    #[test]
    fn retention_reachability_membership_is_exact_and_fail_closed() {
        let reachable = BTreeSet::from([oid('a'), oid('b')]);
        assert_eq!(containment_observation(&oid('a'), &reachable), Some(true));
        assert_eq!(containment_observation(&oid('c'), &reachable), Some(false));
        assert_eq!(containment_observation("not-an-oid", &reachable), None);
    }

    #[test]
    fn offloaded_git_metadata_is_a_hard_blocker() {
        let status = crate::provider_sync::FileProviderItemStatus {
            is_downloaded: false,
            is_downloading: false,
            is_most_recent_version_downloaded: false,
            is_uploaded: true,
            is_uploading: false,
            has_unresolved_conflicts: false,
            is_excluded_from_sync: false,
            is_sync_paused: false,
            is_trashed: false,
            is_keep_downloaded: false,
            capabilities: 0,
            allows_eviction: false,
            observed_bytes: 30,
            item_identifier_fingerprint: "f".repeat(64),
        };
        assert_eq!(
            git_admin_metadata_blocker(&status),
            Some("git-worktree-admin-metadata-not-local-current")
        );
    }

    #[test]
    fn only_complete_clean_merged_idle_secondary_is_candidate() {
        let safe = ClassificationInput {
            primary: false,
            audit_origin: false,
            bare: false,
            locked: false,
            prunable: false,
            path_valid: true,
            status_clean: Some(true),
            contained_in_reference: Some(true),
            closed_pull_request_head: false,
            completed_pull_request_commit: false,
            open_pull_request_commit: false,
            stale_open_pull_request_head: false,
            head_is_retained_tip: false,
            actor_cwd_inside: Some(false),
            size_complete: true,
            active_use_assessed: true,
            active_use_complete: true,
            active_use_active: false,
        };
        assert!(candidate_blockers(&safe).is_empty());

        let closed_unmerged = ClassificationInput {
            contained_in_reference: Some(false),
            closed_pull_request_head: true,
            ..safe
        };
        assert!(candidate_blockers(&closed_unmerged).is_empty());

        let stale_open = ClassificationInput {
            contained_in_reference: Some(false),
            stale_open_pull_request_head: true,
            ..safe
        };
        assert!(candidate_blockers(&stale_open).is_empty());

        let dirty = ClassificationInput {
            status_clean: Some(false),
            ..safe
        };
        assert_eq!(candidate_blockers(&dirty), vec!["worktree-dirty"]);
        assert_eq!(
            disposition(&candidate_blockers(&dirty)),
            GitWorktreeDisposition::Preserve
        );

        let incomplete = ClassificationInput {
            status_clean: None,
            ..safe
        };
        assert_eq!(
            disposition(&candidate_blockers(&incomplete)),
            GitWorktreeDisposition::EvidenceGap
        );

        let active = ClassificationInput {
            active_use_active: true,
            ..safe
        };
        assert_eq!(candidate_blockers(&active), vec!["active-use-detected"]);

        let retained_tip = ClassificationInput {
            head_is_retained_tip: true,
            ..safe
        };
        assert_eq!(
            candidate_blockers(&retained_tip),
            vec!["head-is-retained-tip"]
        );
    }

    #[test]
    fn fingerprint_is_stable_and_changes_with_candidate_identity() {
        let size = GitWorktreeSizeEvidence {
            method: "bounded-filesystem-st-blocks-sum".into(),
            evidence_complete: true,
            allocated_bytes: 10,
            logical_bytes: 9,
            visited_entries: 2,
            error: None,
        };
        let active_use = GitWorktreeActiveUseEvidence {
            method: "lsof-recursive-pid".into(),
            assessed: true,
            evidence_complete: true,
            active: false,
            observed_pids: Vec::new(),
            results_truncated: false,
            error: None,
        };
        let mut entry = GitWorktreeAuditEntry {
            path: "/tmp/secondary".into(),
            path_fingerprint: "p".repeat(64),
            head: oid('a'),
            branch: Some("refs/heads/merged".into()),
            detached: false,
            bare: false,
            primary: false,
            audit_origin: false,
            locked: false,
            lock_reason: None,
            prunable: false,
            prunable_reason: None,
            status_clean: Some(true),
            status_entry_count: Some(0),
            contained_in_reference: Some(true),
            closed_pull_request_head: false,
            completed_pull_request_commit: false,
            open_pull_request_commit: false,
            stale_open_pull_request_head: false,
            head_is_retained_tip: false,
            actor_cwd_inside: Some(false),
            size,
            active_use,
            disposition: GitWorktreeDisposition::RemovalCandidate,
            blockers: Vec::new(),
            entry_fingerprint: String::new(),
        };
        let reference_set = "r".repeat(64);
        entry.entry_fingerprint = entry_fingerprint("/tmp/common", &reference_set, &entry);
        let first = removal_plan_fingerprint("/tmp/common", &reference_set, &[entry.clone()]);
        let second = removal_plan_fingerprint("/tmp/common", &reference_set, &[entry.clone()]);
        assert_eq!(first, second);
        entry.head = oid('c');
        entry.entry_fingerprint = entry_fingerprint("/tmp/common", &reference_set, &entry);
        let changed = removal_plan_fingerprint("/tmp/common", &reference_set, &[entry]);
        assert_ne!(first, changed);
    }

    #[test]
    fn public_summary_redacts_local_identity_and_denies_execution_claims() {
        let report = GitWorktreeAuditReport {
            schema_kind: GIT_WORKTREE_AUDIT_SCHEMA_KIND.into(),
            version: 4,
            repository_root: "/private/repo".into(),
            common_dir: "/private/repo/.git".into(),
            generated_at_ms: 1,
            stale_open_pull_request_cutoff_ms: None,
            retention_references: vec![GitWorktreeReferenceBinding {
                reference_ref: "origin/develop".into(),
                reference_oid: oid('a'),
            }],
            retention_reference_set_fingerprint: "r".repeat(64),
            removal_authority_fingerprint: "a".repeat(64),
            retention_reachable_commit_count: 1,
            worktree_count: 1,
            removal_candidate_count: 0,
            removal_candidate_allocated_bytes: 0,
            preserved_count: 1,
            evidence_gap_count: 0,
            evidence_complete: true,
            removal_plan_fingerprint: "f".repeat(64),
            exact_approval_phrase: None,
            entries: Vec::new(),
            issues: Vec::new(),
            filesystem_mutation_executed: false,
        };
        let encoded = serde_json::to_string(&public_summary(&report)).unwrap();
        assert!(!encoded.contains("/private/repo"));
        assert!(encoded.contains("\"local_paths_redacted\":true"));
        assert!(encoded.contains("\"filesystem_mutation_executed\":false"));
        assert!(encoded.contains("filename-date-not-used"));
    }

    #[test]
    fn options_and_reference_are_bounded() {
        validate_options(GitWorktreeAuditOptions::default()).unwrap();
        assert!(validate_options(GitWorktreeAuditOptions {
            command_timeout_ms: 0,
            ..GitWorktreeAuditOptions::default()
        })
        .is_err());
        validate_reference("origin/develop").unwrap();
        assert!(validate_reference("--all").is_err());
        assert!(validate_reference("bad\nref").is_err());
    }

    #[test]
    fn approval_requires_exact_phrase_human_attribution_and_intact_audit() {
        let report = executable_report();
        let phrase = report.exact_approval_phrase.clone().unwrap();
        let approval = approve_stale_worktree_removal(
            &report,
            &phrase,
            11,
            "human:local:test",
            "merged worktree reviewed for removal",
        )
        .unwrap();
        assert!(valid_hex64(&approval.approval_id));
        assert!(approve_stale_worktree_removal(
            &report,
            &format!("{phrase} "),
            11,
            "human:local:test",
            "merged worktree reviewed for removal",
        )
        .is_err());
        assert!(approve_stale_worktree_removal(
            &report,
            &phrase,
            11,
            "agent:test",
            "merged worktree reviewed for removal",
        )
        .is_err());

        let mut tampered = report;
        tampered.removal_candidate_allocated_bytes += 1;
        assert!(approve_stale_worktree_removal(
            &tampered,
            &phrase,
            11,
            "human:local:test",
            "merged worktree reviewed for removal",
        )
        .is_err());
    }

    #[cfg(all(unix, not(coverage)))]
    #[test]
    fn execution_removes_only_clean_merged_worktree_and_retains_branch() {
        let (_temp, repository, secondary) = temporary_repository();
        let generated_at = current_unix_ms();
        let report = audit_git_worktrees(
            &repository,
            &["main".into()],
            GitWorktreeAuditOptions::default(),
            generated_at,
        )
        .unwrap();
        assert_eq!(report.removal_candidate_count, 1, "{report:#?}");
        assert_eq!(
            report
                .entries
                .iter()
                .filter(|entry| Path::new(&entry.path) == fs::canonicalize(&secondary).unwrap())
                .count(),
            1
        );
        let phrase = report.exact_approval_phrase.clone().unwrap();
        let approval = approve_stale_worktree_removal(
            &report,
            &phrase,
            generated_at + 1,
            "human:local:test",
            "temporary merged worktree reviewed for removal",
        )
        .unwrap();
        let result = execute_stale_worktree_removal(
            &report,
            &approval,
            &phrase,
            GitWorktreeAuditOptions::default(),
            generated_at + 2,
        )
        .unwrap();
        assert!(result.verification_complete);
        assert_eq!(result.removed_count, 1);
        assert!(!result.branch_delete_executed);
        assert!(!result.git_prune_executed);
        assert!(!secondary.exists());
        git(&repository, &["show-ref", "--verify", "refs/heads/merged"]);
    }

    #[cfg(all(unix, not(coverage)))]
    #[test]
    fn execution_fails_closed_when_candidate_becomes_dirty() {
        let (_temp, repository, secondary) = temporary_repository();
        let generated_at = current_unix_ms();
        let report = audit_git_worktrees(
            &repository,
            &["main".into()],
            GitWorktreeAuditOptions::default(),
            generated_at,
        )
        .unwrap();
        let phrase = report.exact_approval_phrase.clone().unwrap();
        let approval = approve_stale_worktree_removal(
            &report,
            &phrase,
            generated_at + 1,
            "human:local:test",
            "temporary merged worktree reviewed for removal",
        )
        .unwrap();
        fs::write(secondary.join("untracked.txt"), b"do not remove\n").unwrap();
        let error = execute_stale_worktree_removal(
            &report,
            &approval,
            &phrase,
            GitWorktreeAuditOptions::default(),
            generated_at + 2,
        )
        .unwrap_err();
        assert!(error.starts_with("git-worktree-removal-"));
        assert!(secondary.exists());
        git(&repository, &["show-ref", "--verify", "refs/heads/merged"]);
    }

    #[cfg(all(unix, not(coverage)))]
    #[test]
    fn immutable_records_are_create_new_and_cannot_be_redirected_into_repository() {
        use std::os::unix::fs::symlink;

        let (temp, repository, _secondary) = temporary_repository();
        let generated_at = current_unix_ms();
        let report = audit_git_worktrees(
            &repository,
            &["main".into()],
            GitWorktreeAuditOptions::default(),
            generated_at,
        )
        .unwrap();
        let phrase = report.exact_approval_phrase.clone().unwrap();
        let approval = approve_stale_worktree_removal(
            &report,
            &phrase,
            generated_at + 1,
            "human:local:test",
            "immutable approval record storage reviewed",
        )
        .unwrap();

        let app_data = temp.path().join("app-data");
        let record_dir =
            prepare_worktree_record_directory(&app_data, &report, "git-worktree-removals").unwrap();
        let filename = format!("{}.approval.json", approval.approval_id);
        let record = write_immutable_worktree_record(&record_dir, &filename, &approval).unwrap();
        assert!(record.metadata().unwrap().permissions().readonly());
        assert!(write_immutable_worktree_record(&record_dir, &filename, &approval).is_err());
        assert!(write_immutable_worktree_record(&record_dir, "../escape.json", &approval).is_err());

        let redirected_app_data = temp.path().join("redirected-app-data");
        symlink(&repository, &redirected_app_data).unwrap();
        let error = prepare_worktree_record_directory(
            &redirected_app_data,
            &report,
            "git-worktree-removals",
        )
        .unwrap_err();
        assert_eq!(error, "git-worktree-removal-record-dir-overlaps-repository");
    }
}
