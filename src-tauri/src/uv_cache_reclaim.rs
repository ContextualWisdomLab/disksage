//! Native uv cache pruning with explicit owner, process, service, and capacity evidence.
//!
//! uv owns its private cache format and locking protocol. DiskSage therefore plans and executes
//! only `uv cache prune`; it never deletes a private uv cache bucket directly. A plan is blocked
//! when cache traversal is incomplete, a live process uses the cache, a persistent user service
//! references cache-backed state, or an installed tool contains a symlink back into the cache.

#![deny(missing_docs)]

use crate::git_worktree::GitWorktreeActiveUseEvidence;
use crate::reclaim::{PlannedOperation, ReclaimPlanOptions};
use serde::Serialize;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Schema version for read-only native uv reclaim plans and approvals.
pub const SCHEMA_VERSION: u32 = 2;
/// Schema version for native uv reclaim execution receipts.
pub const RECEIPT_SCHEMA_VERSION: u32 = 4;
const COMMAND_TIMEOUT_MS: u64 = 120_000;
const MAX_OUTPUT_BYTES: usize = 32 * 1024;
const MAX_SERVICE_FILES: usize = 2_048;
const MAX_SERVICE_FILE_BYTES: u64 = 1024 * 1024;
const MAX_TOOL_TREE_ENTRIES: usize = 100_000;
const EXECUTE_ARGUMENTS: [&str; 8] = [
    "cache",
    "prune",
    "--no-config",
    "--offline",
    "--no-progress",
    "--color",
    "never",
    "--cache-dir",
];

/// Read-only evidence authorizing, or blocking, one uv-native cache prune attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UvCacheReclaimPlan {
    /// Contract version.
    pub schema_version: u32,
    /// Canonical uv executable used for discovery and later execution.
    pub executable_path: String,
    /// Stable local filesystem identity of the executable at planning time.
    pub executable_identity: String,
    /// Bounded `uv --version` output.
    pub uv_version: String,
    /// Canonical cache directory reported by `uv cache dir --no-config`.
    pub cache_path: String,
    /// Tool environment directory reported by `uv tool dir --no-config`.
    pub tools_path: String,
    /// Logical bytes observed by DiskSage's bounded reclaim inventory.
    pub cache_logical_bytes: u64,
    /// Allocated bytes when the platform exposes supported allocation evidence.
    pub cache_allocated_bytes: Option<u64>,
    /// Cache entries skipped by bounded inventory.
    pub cache_entries_skipped: u64,
    /// Bounded live-process and open-file evidence for the cache root.
    pub active_use: GitWorktreeActiveUseEvidence,
    /// Whether persistent-service discovery completed without an evidence gap.
    pub persistent_service_evidence_complete: bool,
    /// Persistent user-service references whose executable or argument resolves into the cache.
    pub persistent_service_cache_dependency_count: u64,
    /// Installed-tool symlinks whose resolved target is inside the cache.
    pub persistent_tool_cache_symlink_count: u64,
    /// Stable machine-readable reasons execution is not authorized.
    pub blockers: Vec<String>,
    /// Caller-supplied local observation timestamp in milliseconds since Unix epoch.
    pub observed_at_ms: u64,
    /// Content fingerprint over the mutation-relevant plan evidence.
    pub plan_fingerprint: String,
    /// Exact phrase a human approval must repeat for this plan.
    pub exact_approval_phrase: String,
}

impl UvCacheReclaimPlan {
    /// Returns true only when every mutation blocker is absent.
    pub fn eligible(&self) -> bool {
        self.blockers.is_empty()
    }
}

/// Human-attributed approval bound to one exact native uv reclaim plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UvCacheReclaimApproval {
    /// Contract version.
    pub schema_version: u32,
    /// Fingerprint of the exact fresh plan being approved.
    pub plan_fingerprint: String,
    /// Local approval timestamp in milliseconds since Unix epoch.
    pub approved_at_ms: u64,
    /// Human attribution in the shared `human:*` review namespace.
    pub approved_by: String,
    /// Human rationale retained with the immutable approval record.
    pub rationale: String,
    /// Exact plan-specific confirmation phrase.
    pub exact_approval_phrase: String,
}

/// Terminal evidence for one attempted uv-native prune.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UvCacheReclaimReceipt {
    /// Receipt contract version.
    pub schema_version: u32,
    /// Fresh plan re-created immediately before the mutation attempt.
    pub plan: UvCacheReclaimPlan,
    /// Human approval bound to the fresh plan.
    pub approval: UvCacheReclaimApproval,
    /// Exact executable and arguments invoked; no shell is involved.
    pub command: Vec<String>,
    /// Process exit code, or `-1` when execution failed before an exit code existed.
    pub status_code: i32,
    /// Bounded native command stdout.
    pub stdout: String,
    /// Bounded native command stderr.
    pub stderr: String,
    /// Stable execution error when the process could not complete normally.
    pub execution_error: Option<String>,
    /// Whether stdout or stderr exceeded the evidence bound.
    pub output_truncated: bool,
    /// Filesystem available bytes immediately before the native prune.
    pub filesystem_available_before_bytes: u64,
    /// Filesystem available bytes after the attempt, when postcheck succeeded.
    pub filesystem_available_after_bytes: Option<u64>,
    /// Positive filesystem-available delta when measurable without underflow.
    pub filesystem_available_delta_bytes: Option<u64>,
    /// Stable reason the post-mutation capacity check failed, if any.
    pub capacity_postcheck_error: Option<String>,
    /// Local execution timestamp in milliseconds since Unix epoch.
    pub executed_at_ms: u64,
    /// Local immutable result-record path.
    pub result_record_path: String,
    /// Publication error retained in-memory when the result record could not be persisted.
    pub result_record_error: Option<String>,
}

#[derive(Debug)]
struct CommandOutput {
    status_code: i32,
    stdout: String,
    stderr: String,
    truncated: bool,
}

#[derive(Debug, Default)]
struct PersistentDependencyEvidence {
    complete: bool,
    service_cache_dependencies: u64,
    tool_cache_symlinks: u64,
}

/// Resolve uv only from fixed system/user package-manager locations; PATH is never mutation authority.
pub fn fixed_uv_path() -> Result<PathBuf, String> {
    [
        Path::new("/opt/homebrew/bin/uv"),
        Path::new("/usr/local/bin/uv"),
        Path::new("/usr/bin/uv"),
    ]
    .into_iter()
    .find(|path| path.exists())
    .map(Path::to_path_buf)
    .ok_or_else(|| "uv-cache-reclaim-executable-not-found".into())
}

#[cfg(unix)]
fn executable(path: &Path) -> Result<(PathBuf, String), String> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let path = std::fs::canonicalize(path)
        .map_err(|_| "uv-cache-reclaim-executable-unavailable".to_string())?;
    let metadata = std::fs::symlink_metadata(&path)
        .map_err(|_| "uv-cache-reclaim-executable-unavailable".to_string())?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.permissions().mode() & 0o111 == 0
    {
        return Err("uv-cache-reclaim-executable-unsafe".into());
    }
    Ok((
        path,
        format!(
            "{}:{}:{}:{}:{}",
            metadata.dev(),
            metadata.ino(),
            metadata.len(),
            metadata.mtime(),
            metadata.mtime_nsec()
        ),
    ))
}

#[cfg(not(unix))]
fn executable(_path: &Path) -> Result<(PathBuf, String), String> {
    Err("uv-cache-reclaim-platform-active-use-evidence-unavailable".into())
}

fn bounded_text(file: &mut std::fs::File) -> Result<(String, bool), String> {
    file.seek(SeekFrom::Start(0))
        .map_err(|_| "uv-cache-reclaim-output-seek-failed".to_string())?;
    let mut bytes = Vec::new();
    file.take((MAX_OUTPUT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "uv-cache-reclaim-output-read-failed".to_string())?;
    let truncated = bytes.len() > MAX_OUTPUT_BYTES;
    bytes.truncate(MAX_OUTPUT_BYTES);
    Ok((String::from_utf8_lossy(&bytes).replace('\0', ""), truncated))
}

fn run_uv(path: &Path, args: &[&str]) -> Result<CommandOutput, String> {
    let mut stdout = tempfile::tempfile()
        .map_err(|_| "uv-cache-reclaim-output-file-create-failed".to_string())?;
    let mut stderr = tempfile::tempfile()
        .map_err(|_| "uv-cache-reclaim-output-file-create-failed".to_string())?;
    let mut command = Command::new(path);
    command
        .args(args)
        .env("UV_LOCK_TIMEOUT", "0")
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(
            stdout
                .try_clone()
                .map_err(|_| "uv-cache-reclaim-output-file-clone-failed".to_string())?,
        )
        .stderr(
            stderr
                .try_clone()
                .map_err(|_| "uv-cache-reclaim-output-file-clone-failed".to_string())?,
        );
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            command.pre_exec(|| {
                if libc::setpgid(0, 0) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
    let mut child = command
        .spawn()
        .map_err(|_| "uv-cache-reclaim-command-spawn-failed".to_string())?;
    let deadline = Instant::now() + Duration::from_millis(COMMAND_TIMEOUT_MS);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                #[cfg(unix)]
                unsafe {
                    let _ = libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL);
                }
                let _ = child.kill();
                let _ = child.wait();
                return Err("uv-cache-reclaim-command-timeout".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(_) => {
                #[cfg(unix)]
                unsafe {
                    let _ = libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL);
                }
                let _ = child.kill();
                let _ = child.wait();
                return Err("uv-cache-reclaim-command-wait-failed".into());
            }
        }
    };
    let (stdout, stdout_truncated) = bounded_text(&mut stdout)?;
    let (stderr, stderr_truncated) = bounded_text(&mut stderr)?;
    Ok(CommandOutput {
        status_code: status.code().unwrap_or(-1),
        stdout,
        stderr,
        truncated: stdout_truncated || stderr_truncated,
    })
}

fn command_path(path: &Path, args: &[&str], error: &str) -> Result<PathBuf, String> {
    let output = run_uv(path, args)?;
    if output.status_code != 0 || output.stdout.trim().is_empty() || output.truncated {
        return Err(error.into());
    }
    Ok(PathBuf::from(output.stdout.trim()))
}

fn path_refers_into_root(path: &Path, root: &Path) -> bool {
    if path.is_absolute() && path.starts_with(root) {
        return true;
    }
    std::fs::canonicalize(path)
        .ok()
        .is_some_and(|resolved| resolved.starts_with(root))
}

fn scan_tool_symlink_coupling(
    tools_path: &Path,
    cache_path: &Path,
) -> Result<(u64, bool), String> {
    if !tools_path.exists() {
        return Ok((0, true));
    }
    let mut pending = vec![tools_path.to_path_buf()];
    let mut entries = 0usize;
    let mut coupling = 0u64;
    while let Some(directory) = pending.pop() {
        let read_dir = match std::fs::read_dir(&directory) {
            Ok(value) => value,
            Err(_) => return Ok((coupling, false)),
        };
        for entry in read_dir {
            entries = entries.saturating_add(1);
            if entries > MAX_TOOL_TREE_ENTRIES {
                return Ok((coupling, false));
            }
            let entry = match entry {
                Ok(value) => value,
                Err(_) => return Ok((coupling, false)),
            };
            let path = entry.path();
            let metadata = match std::fs::symlink_metadata(&path) {
                Ok(value) => value,
                Err(_) => return Ok((coupling, false)),
            };
            if metadata.file_type().is_symlink() {
                if path_refers_into_root(&path, cache_path) {
                    coupling = coupling.saturating_add(1);
                }
            } else if metadata.is_dir() {
                pending.push(path);
            }
        }
    }
    Ok((coupling, true))
}

#[cfg(target_os = "macos")]
fn user_service_dependency_evidence(cache_path: &Path) -> PersistentDependencyEvidence {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return PersistentDependencyEvidence::default();
    };
    let root = home.join("Library").join("LaunchAgents");
    if !root.exists() {
        return PersistentDependencyEvidence {
            complete: true,
            ..PersistentDependencyEvidence::default()
        };
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return PersistentDependencyEvidence::default();
    };
    let mut evidence = PersistentDependencyEvidence {
        complete: true,
        ..PersistentDependencyEvidence::default()
    };
    let mut observed = 0usize;
    for entry in entries {
        observed = observed.saturating_add(1);
        if observed > MAX_SERVICE_FILES {
            evidence.complete = false;
            break;
        }
        let Ok(entry) = entry else {
            evidence.complete = false;
            continue;
        };
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("plist") {
            continue;
        }
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            evidence.complete = false;
            continue;
        };
        if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > MAX_SERVICE_FILE_BYTES {
            evidence.complete = false;
            continue;
        }
        let Ok(value) = plist::Value::from_file(&path) else {
            evidence.complete = false;
            continue;
        };
        let Some(dictionary) = value.as_dictionary() else {
            evidence.complete = false;
            continue;
        };
        let mut references = Vec::new();
        if let Some(program) = dictionary.get("Program").and_then(plist::Value::as_string) {
            references.push(PathBuf::from(program));
        }
        if let Some(arguments) = dictionary
            .get("ProgramArguments")
            .and_then(plist::Value::as_array)
        {
            references.extend(
                arguments
                    .iter()
                    .filter_map(plist::Value::as_string)
                    .map(PathBuf::from),
            );
        }
        if references
            .iter()
            .any(|reference| path_refers_into_root(reference, cache_path))
        {
            evidence.service_cache_dependencies =
                evidence.service_cache_dependencies.saturating_add(1);
        }
    }
    evidence
}

#[cfg(target_os = "linux")]
fn user_service_dependency_evidence(cache_path: &Path) -> PersistentDependencyEvidence {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return PersistentDependencyEvidence::default();
    };
    let root = home.join(".config").join("systemd").join("user");
    if !root.exists() {
        return PersistentDependencyEvidence {
            complete: true,
            ..PersistentDependencyEvidence::default()
        };
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return PersistentDependencyEvidence::default();
    };
    let mut evidence = PersistentDependencyEvidence {
        complete: true,
        ..PersistentDependencyEvidence::default()
    };
    let mut observed = 0usize;
    for entry in entries {
        observed = observed.saturating_add(1);
        if observed > MAX_SERVICE_FILES {
            evidence.complete = false;
            break;
        }
        let Ok(entry) = entry else {
            evidence.complete = false;
            continue;
        };
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("service") {
            continue;
        }
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            evidence.complete = false;
            continue;
        };
        if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > MAX_SERVICE_FILE_BYTES {
            evidence.complete = false;
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            evidence.complete = false;
            continue;
        };
        let mut referenced = false;
        for line in text.lines().map(str::trim) {
            let Some(command) = line.strip_prefix("ExecStart=") else {
                continue;
            };
            if command.contains('%') || command.contains("${") || command.contains("$HOME") {
                evidence.complete = false;
                continue;
            }
            referenced |= command
                .split_whitespace()
                .map(|token| token.trim_matches(|value| matches!(value, '\'' | '"' | '-' | '+' | '!' | '@' | ':')))
                .filter(|token| token.starts_with('/'))
                .map(PathBuf::from)
                .any(|reference| path_refers_into_root(&reference, cache_path));
        }
        if referenced {
            evidence.service_cache_dependencies =
                evidence.service_cache_dependencies.saturating_add(1);
        }
    }
    evidence
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn user_service_dependency_evidence(_cache_path: &Path) -> PersistentDependencyEvidence {
    PersistentDependencyEvidence::default()
}

fn persistent_dependency_evidence(
    cache_path: &Path,
    tools_path: &Path,
) -> Result<PersistentDependencyEvidence, String> {
    let mut evidence = user_service_dependency_evidence(cache_path);
    let (tool_cache_symlinks, tool_scan_complete) =
        scan_tool_symlink_coupling(tools_path, cache_path)?;
    evidence.tool_cache_symlinks = tool_cache_symlinks;
    evidence.complete &= tool_scan_complete;
    Ok(evidence)
}

fn fingerprint(
    path: &Path,
    identity: &str,
    version: &str,
    cache_path: &Path,
    tools_path: &Path,
    logical_bytes: u64,
    allocated_bytes: Option<u64>,
    skipped: u64,
    active_use: &GitWorktreeActiveUseEvidence,
    dependencies: &PersistentDependencyEvidence,
    blockers: &[String],
) -> String {
    let evidence = serde_json::to_vec(&(
        path.to_string_lossy(),
        identity,
        version,
        cache_path.to_string_lossy(),
        tools_path.to_string_lossy(),
        logical_bytes,
        allocated_bytes,
        skipped,
        active_use,
        dependencies.complete,
        dependencies.service_cache_dependencies,
        dependencies.tool_cache_symlinks,
        blockers,
    ))
    .expect("fixed uv cache evidence is serializable");
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-uv-cache-reclaim-plan-v2\0");
    hasher.update(&evidence);
    hasher.finalize().to_hex().to_string()
}

/// Build a read-only native uv reclaim plan from current executable, cache, process, and service evidence.
pub fn plan_uv_cache_reclaim(
    requested_uv_path: &Path,
    observed_at_ms: u64,
) -> Result<UvCacheReclaimPlan, String> {
    let (uv_path, executable_identity) = executable(requested_uv_path)?;
    let version = run_uv(&uv_path, &["--version"])?;
    if version.status_code != 0 || version.stdout.trim().is_empty() || version.truncated {
        return Err("uv-cache-reclaim-version-check-failed".into());
    }
    let cache_path = command_path(
        &uv_path,
        &["cache", "dir", "--no-config"],
        "uv-cache-reclaim-cache-dir-check-failed",
    )?;
    let cache_path = std::fs::canonicalize(cache_path)
        .map_err(|_| "uv-cache-reclaim-cache-directory-unavailable".to_string())?;
    let tools_path = command_path(
        &uv_path,
        &["tool", "dir", "--no-config"],
        "uv-cache-reclaim-tool-dir-check-failed",
    )?;
    let evidence = crate::reclaim::plan_reclaim_with_options(
        std::slice::from_ref(&cache_path),
        PlannedOperation::Delete,
        ReclaimPlanOptions {
            include_active_use: false,
        },
    )?;
    let cache = evidence
        .paths
        .first()
        .ok_or_else(|| "uv-cache-reclaim-cache-evidence-missing".to_string())?;
    let active_use = crate::git_worktree::active_use_evidence(
        &cache_path,
        crate::reclaim::ACTIVE_USE_PROBE_TIMEOUT_MS,
        crate::reclaim::ACTIVE_USE_PROBE_MAX_PIDS,
        true,
    );
    let dependencies = persistent_dependency_evidence(&cache_path, &tools_path)?;
    let mut blockers = Vec::new();
    if cache.skipped > 0 {
        blockers.push("cache-inventory-incomplete".into());
    }
    if !active_use.assessed || !active_use.evidence_complete {
        blockers.push("active-use-evidence-incomplete".into());
    } else if active_use.active {
        blockers.push("cache-is-active".into());
    }
    if !dependencies.complete {
        blockers.push("persistent-service-evidence-incomplete".into());
    }
    if dependencies.service_cache_dependencies > 0 {
        blockers.push("persistent-service-cache-dependency".into());
    }
    if dependencies.tool_cache_symlinks > 0
        || std::env::var("UV_LINK_MODE").is_ok_and(|value| value == "symlink")
    {
        blockers.push("persistent-tool-cache-symlink-coupling".into());
    }
    let plan_fingerprint = fingerprint(
        &uv_path,
        &executable_identity,
        version.stdout.trim(),
        &cache_path,
        &tools_path,
        cache.estimate.logical_bytes,
        cache.estimate.allocated_bytes,
        cache.skipped,
        &active_use,
        &dependencies,
        &blockers,
    );
    Ok(UvCacheReclaimPlan {
        schema_version: SCHEMA_VERSION,
        executable_path: uv_path.to_string_lossy().into_owned(),
        executable_identity,
        uv_version: version.stdout.trim().into(),
        cache_path: cache_path.to_string_lossy().into_owned(),
        tools_path: tools_path.to_string_lossy().into_owned(),
        cache_logical_bytes: cache.estimate.logical_bytes,
        cache_allocated_bytes: cache.estimate.allocated_bytes,
        cache_entries_skipped: cache.skipped,
        active_use,
        persistent_service_evidence_complete: dependencies.complete,
        persistent_service_cache_dependency_count: dependencies.service_cache_dependencies,
        persistent_tool_cache_symlink_count: dependencies.tool_cache_symlinks,
        blockers,
        observed_at_ms,
        exact_approval_phrase: format!("DiskSage uv cache prune approve {plan_fingerprint}"),
        plan_fingerprint,
    })
}

#[cfg(unix)]
fn filesystem_available_bytes(path: &Path) -> Result<u64, String> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let path = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| "uv-cache-reclaim-filesystem-path-invalid".to_string())?;
    let mut value = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    if unsafe { libc::statvfs(path.as_ptr(), value.as_mut_ptr()) } != 0 {
        return Err("uv-cache-reclaim-filesystem-capacity-unavailable".into());
    }
    let value = unsafe { value.assume_init() };
    Ok((value.f_bavail as u64).saturating_mul(value.f_frsize as u64))
}

#[cfg(not(unix))]
fn filesystem_available_bytes(_path: &Path) -> Result<u64, String> {
    Err("uv-cache-reclaim-filesystem-capacity-unavailable".into())
}

fn attempt_id(plan_fingerprint: &str, executed_at_ms: u64) -> Result<String, String> {
    let mut nonce = [0u8; 16];
    getrandom::fill(&mut nonce)
        .map_err(|_| "uv-cache-reclaim-attempt-id-unavailable".to_string())?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-uv-cache-reclaim-attempt-v2\0");
    hasher.update(plan_fingerprint.as_bytes());
    hasher.update(&executed_at_ms.to_le_bytes());
    hasher.update(&nonce);
    Ok(hasher.finalize().to_hex().to_string())
}

/// Execute one fresh, explicitly approved native `uv cache prune` and persist terminal evidence.
pub fn execute_uv_cache_reclaim(
    uv_path: &Path,
    approved_plan_fingerprint: &str,
    confirmation: &str,
    approved_by: &str,
    rationale: &str,
    record_dir: &Path,
    executed_at_ms: u64,
) -> Result<UvCacheReclaimReceipt, String> {
    let plan = plan_uv_cache_reclaim(uv_path, executed_at_ms)?;
    if plan.plan_fingerprint != approved_plan_fingerprint
        || confirmation != plan.exact_approval_phrase
    {
        return Err("uv-cache-reclaim-fresh-plan-approval-mismatch".into());
    }
    if !plan.eligible() {
        return Err("uv-cache-reclaim-current-plan-blocked".into());
    }
    crate::cloud_review::validate_review_attribution(approved_by, rationale)
        .map_err(|_| "uv-cache-reclaim-approval-attribution-invalid".to_string())?;
    let (current_path, current_identity) = executable(Path::new(&plan.executable_path))?;
    if current_identity != plan.executable_identity {
        return Err("uv-cache-reclaim-executable-identity-changed".into());
    }
    let cache_path = Path::new(&plan.cache_path);
    let before = filesystem_available_bytes(cache_path)?;
    let approval = UvCacheReclaimApproval {
        schema_version: SCHEMA_VERSION,
        plan_fingerprint: plan.plan_fingerprint.clone(),
        approved_at_ms: executed_at_ms,
        approved_by: approved_by.into(),
        rationale: rationale.into(),
        exact_approval_phrase: confirmation.into(),
    };
    let attempt_id = attempt_id(&plan.plan_fingerprint, executed_at_ms)?;
    let approval_name = format!("{}.{}.approval.json", plan.plan_fingerprint, attempt_id);
    crate::cloud_local_eviction::write_immutable_record(record_dir, &approval_name, &approval)?;
    let cache_path_argument = plan.cache_path.clone();
    let mut args = EXECUTE_ARGUMENTS.to_vec();
    args.push(&cache_path_argument);
    let execution = run_uv(&current_path, &args);
    let (after, capacity_postcheck_error) = match filesystem_available_bytes(cache_path) {
        Ok(value) => (Some(value), None),
        Err(error) => (None, Some(error)),
    };
    let (status_code, stdout, stderr, output_truncated, execution_error) = match execution {
        Ok(output) => (
            output.status_code,
            output.stdout,
            output.stderr,
            output.truncated,
            None,
        ),
        Err(error) => (-1, String::new(), String::new(), false, Some(error)),
    };
    let result_name = format!("{}.{}.result.json", plan.plan_fingerprint, attempt_id);
    let result_record_path = record_dir.join(&result_name).to_string_lossy().into_owned();
    let mut receipt = UvCacheReclaimReceipt {
        schema_version: RECEIPT_SCHEMA_VERSION,
        plan,
        approval,
        command: std::iter::once(current_path.to_string_lossy().into_owned())
            .chain(args.iter().map(|value| (*value).to_string()))
            .collect(),
        status_code,
        stdout,
        stderr,
        execution_error,
        output_truncated,
        filesystem_available_before_bytes: before,
        filesystem_available_after_bytes: after,
        filesystem_available_delta_bytes: after.and_then(|value| value.checked_sub(before)),
        capacity_postcheck_error,
        executed_at_ms,
        result_record_path,
        result_record_error: None,
    };
    if let Err(error) =
        crate::cloud_local_eviction::write_immutable_record(record_dir, &result_name, &receipt)
    {
        receipt.result_record_error = Some(error);
    }
    Ok(receipt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn fake_uv(path: &Path, cache: &Path, tools: &Path) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(
            path,
            format!(
                "#!/bin/sh\nset -eu\nif [ \"${{1:-}}\" = '--version' ]; then printf 'uv 0.test\\n'; exit 0; fi\nif [ \"${{1:-}} ${{2:-}}\" = 'cache dir' ]; then printf '%s\\n' '{}'; exit 0; fi\nif [ \"${{1:-}} ${{2:-}}\" = 'tool dir' ]; then printf '%s\\n' '{}'; exit 0; fi\nexit 99\n",
                cache.display(), tools.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn live_cache_handle_vetoes_native_prune() {
        let temp = tempfile::tempdir().unwrap();
        let cache = temp.path().join("cache");
        let tools = temp.path().join("tools");
        std::fs::create_dir(&cache).unwrap();
        std::fs::create_dir(&tools).unwrap();
        let lock = cache.join(".lock");
        std::fs::write(&lock, b"").unwrap();
        let _open = std::fs::File::open(&lock).unwrap();
        let uv = temp.path().join("uv");
        fake_uv(&uv, &cache, &tools);

        let plan = plan_uv_cache_reclaim(&uv, 1).unwrap();

        assert!(!plan.eligible());
        assert!(plan.blockers.iter().any(|value| value == "cache-is-active"));
        assert!(plan.active_use.active);
    }
}
