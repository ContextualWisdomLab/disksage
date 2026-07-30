//! Fail-closed, fingerprint-bound Git worktree auditing.
//!
//! The audit is read-only. A worktree is a removal candidate only when its HEAD is already
//! contained in an explicitly selected retention-reference set without itself being an exact
//! retained tip, its tracked and untracked state is clean, its path and size evidence are complete,
//! it is neither locked nor prunable, and no active CWD or open-file consumer is observed. The
//! resulting approval phrase is evidence, not execution.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

pub const GIT_WORKTREE_AUDIT_SCHEMA_KIND: &str = "disksage.git-worktree-audit/v2";
const MAX_COMMAND_OUTPUT_BYTES: usize = 4 * 1024 * 1024;
const MAX_REFERENCE_BYTES: usize = 1_024;
const MAX_REACHABLE_COMMITS: usize = 100_000;
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
            max_worktrees: 512,
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
    pub retention_references: Vec<GitWorktreeReferenceBinding>,
    pub retention_reference_set_fingerprint: String,
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
    pub retention_reference_count: usize,
    pub retention_reference_set_fingerprint: String,
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

fn validate_reference(reference: &str) -> Result<(), String> {
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
fn active_use_evidence(
    path: &Path,
    timeout_ms: u64,
    max_pids: usize,
) -> GitWorktreeActiveUseEvidence {
    // Running lsof with its own CWD inside the audited tree would make the probe observe itself.
    // A canonical worktree has an existing parent, which is outside the candidate directory.
    let command_cwd = path.parent().unwrap_or(path);
    let result = match run_bounded_command(
        "lsof",
        &[
            OsString::from("-F0p"),
            OsString::from("+D"),
            path.as_os_str().to_os_string(),
        ],
        command_cwd,
        timeout_ms,
    ) {
        Ok(result) => result,
        Err(error) => {
            return GitWorktreeActiveUseEvidence {
                method: "lsof-recursive-pid".into(),
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
            method: "lsof-recursive-pid".into(),
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
            method: "lsof-recursive-pid".into(),
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
            method: "lsof-recursive-pid".into(),
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
        let Some(raw_pid) = field.strip_prefix(b"p") else {
            continue;
        };
        let Ok(raw_pid) = std::str::from_utf8(raw_pid) else {
            return GitWorktreeActiveUseEvidence {
                method: "lsof-recursive-pid".into(),
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
                method: "lsof-recursive-pid".into(),
                assessed: true,
                evidence_complete: false,
                active: false,
                observed_pids: Vec::new(),
                results_truncated: false,
                error: Some("active-use-pid-invalid".into()),
            };
        };
        if pid == result.child_pid {
            continue;
        }
        pids.insert(pid);
    }
    let results_truncated = pids.len() > max_pids;
    let observed_pids: Vec<_> = pids.into_iter().take(max_pids).collect();
    GitWorktreeActiveUseEvidence {
        method: "lsof-recursive-pid".into(),
        assessed: true,
        evidence_complete: !results_truncated,
        active: !observed_pids.is_empty(),
        observed_pids,
        results_truncated,
        error: results_truncated.then(|| "active-use-pid-limit".into()),
    }
}

#[cfg(not(unix))]
fn active_use_evidence(
    _path: &Path,
    _timeout_ms: u64,
    _max_pids: usize,
) -> GitWorktreeActiveUseEvidence {
    GitWorktreeActiveUseEvidence {
        method: "platform-active-use-probe-unavailable".into(),
        assessed: true,
        evidence_complete: false,
        active: false,
        observed_pids: Vec::new(),
        results_truncated: false,
        error: Some("active-use-platform-unsupported".into()),
    }
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
    match input.contained_in_reference {
        Some(true) => {}
        Some(false) => blockers.push("reference-does-not-contain-head".into()),
        None => blockers.push("reference-containment-evidence-incomplete".into()),
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
    validate_options(options)?;
    if !repository_root.is_absolute() {
        return Err("git-worktree-repository-root-not-absolute".into());
    }
    let repository_root = canonical_real_directory(repository_root)?;
    let common_dir = resolve_common_dir(&repository_root, options.command_timeout_ms)?;
    let retention_references = resolve_references(
        &repository_root,
        retention_references,
        options.command_timeout_ms,
    )?;
    let reference_set_fingerprint = retention_reference_set_fingerprint(&retention_references);
    let retained_tip_oids: BTreeSet<_> = retention_references
        .iter()
        .map(|binding| binding.reference_oid.as_str())
        .collect();
    let reachable_commits = reachable_commit_set(
        &repository_root,
        &retention_references,
        options.command_timeout_ms,
    )?;
    let raw_worktrees = list_worktrees(&repository_root, options)?;
    let actor_cwd = canonical_actor_cwd();
    let common_dir_string = common_dir.to_string_lossy().into_owned();
    let audit_origin = repository_root.clone();
    let mut entries = Vec::with_capacity(raw_worktrees.len());
    let mut issues = Vec::new();

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
        let blockers = candidate_blockers(&classification);
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
            head_is_retained_tip,
            actor_cwd_inside,
            size,
            active_use,
            disposition,
            blockers,
            entry_fingerprint: String::new(),
        };
        entry.entry_fingerprint =
            entry_fingerprint(&common_dir_string, &reference_set_fingerprint, &entry);
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
        removal_plan_fingerprint(&common_dir_string, &reference_set_fingerprint, &entries);
    let exact_approval_phrase = (removal_candidate_count > 0).then(|| {
        format!(
            "DiskSage stale worktree {removal_candidate_count} {removal_candidate_allocated_bytes} 승인 {removal_plan_fingerprint}"
        )
    });

    Ok(GitWorktreeAuditReport {
        schema_kind: GIT_WORKTREE_AUDIT_SCHEMA_KIND.into(),
        version: 2,
        repository_root: repository_root.to_string_lossy().into_owned(),
        common_dir: common_dir_string,
        generated_at_ms,
        retention_references,
        retention_reference_set_fingerprint: reference_set_fingerprint,
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
        retention_reference_count: report.retention_references.len(),
        retention_reference_set_fingerprint: report.retention_reference_set_fingerprint.clone(),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn oid(character: char) -> String {
        std::iter::repeat_n(character, 40).collect()
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

    #[test]
    fn retention_reachability_membership_is_exact_and_fail_closed() {
        let reachable = BTreeSet::from([oid('a'), oid('b')]);
        assert_eq!(containment_observation(&oid('a'), &reachable), Some(true));
        assert_eq!(containment_observation(&oid('c'), &reachable), Some(false));
        assert_eq!(containment_observation("not-an-oid", &reachable), None);
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
            head_is_retained_tip: false,
            actor_cwd_inside: Some(false),
            size_complete: true,
            active_use_assessed: true,
            active_use_complete: true,
            active_use_active: false,
        };
        assert!(candidate_blockers(&safe).is_empty());

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
            version: 2,
            repository_root: "/private/repo".into(),
            common_dir: "/private/repo/.git".into(),
            generated_at_ms: 1,
            retention_references: vec![GitWorktreeReferenceBinding {
                reference_ref: "origin/develop".into(),
                reference_oid: oid('a'),
            }],
            retention_reference_set_fingerprint: "r".repeat(64),
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
}
