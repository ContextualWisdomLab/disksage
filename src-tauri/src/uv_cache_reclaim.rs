//! Native uv cache pruning with fresh identity, active-use, and capacity evidence.

use crate::git_worktree::GitWorktreeActiveUseEvidence;
use crate::reclaim::{PlannedOperation, ReclaimPlanOptions};
use serde::Serialize;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub const SCHEMA_VERSION: u32 = 1;
pub const RECEIPT_SCHEMA_VERSION: u32 = 3;
const COMMAND_TIMEOUT_MS: u64 = 120_000;
const MAX_OUTPUT_BYTES: usize = 32 * 1024;
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UvCacheReclaimPlan {
    pub schema_version: u32,
    pub executable_path: String,
    pub executable_identity: String,
    pub uv_version: String,
    pub cache_path: String,
    pub cache_logical_bytes: u64,
    pub cache_allocated_bytes: Option<u64>,
    pub cache_entries_skipped: u64,
    pub active_use: GitWorktreeActiveUseEvidence,
    pub blockers: Vec<String>,
    pub observed_at_ms: u64,
    pub plan_fingerprint: String,
    pub exact_approval_phrase: String,
}

impl UvCacheReclaimPlan {
    pub fn eligible(&self) -> bool {
        self.blockers.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UvCacheReclaimApproval {
    pub schema_version: u32,
    pub plan_fingerprint: String,
    pub approved_at_ms: u64,
    pub approved_by: String,
    pub rationale: String,
    pub exact_approval_phrase: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UvCacheReclaimReceipt {
    pub schema_version: u32,
    pub plan: UvCacheReclaimPlan,
    pub approval: UvCacheReclaimApproval,
    pub command: Vec<String>,
    pub status_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub execution_error: Option<String>,
    pub output_truncated: bool,
    pub filesystem_available_before_bytes: u64,
    pub filesystem_available_after_bytes: Option<u64>,
    pub filesystem_available_delta_bytes: Option<u64>,
    pub capacity_postcheck_error: Option<String>,
    pub executed_at_ms: u64,
    pub result_record_path: String,
    pub result_record_error: Option<String>,
}

struct CommandOutput {
    status_code: i32,
    stdout: String,
    stderr: String,
    truncated: bool,
}

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

fn fingerprint(
    path: &Path,
    identity: &str,
    version: &str,
    cache_path: &Path,
    logical_bytes: u64,
    allocated_bytes: Option<u64>,
    skipped: u64,
    active_use: &GitWorktreeActiveUseEvidence,
    blockers: &[String],
) -> String {
    let evidence = serde_json::to_vec(&(
        path.to_string_lossy(),
        identity,
        version,
        cache_path.to_string_lossy(),
        logical_bytes,
        allocated_bytes,
        skipped,
        active_use,
        blockers,
    ))
    .expect("fixed uv cache evidence is serializable");
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-uv-cache-reclaim-plan-v1\0");
    hasher.update(&evidence);
    hasher.finalize().to_hex().to_string()
}

pub fn plan_uv_cache_reclaim(
    requested_uv_path: &Path,
    observed_at_ms: u64,
) -> Result<UvCacheReclaimPlan, String> {
    let (uv_path, executable_identity) = executable(requested_uv_path)?;
    let version = run_uv(&uv_path, &["--version"])?;
    if version.status_code != 0 || version.stdout.trim().is_empty() || version.truncated {
        return Err("uv-cache-reclaim-version-check-failed".into());
    }
    let cache_dir = run_uv(&uv_path, &["cache", "dir", "--no-config"])?;
    if cache_dir.status_code != 0 || cache_dir.stdout.trim().is_empty() || cache_dir.truncated {
        return Err("uv-cache-reclaim-cache-dir-check-failed".into());
    }
    let cache_path = std::fs::canonicalize(cache_dir.stdout.trim())
        .map_err(|_| "uv-cache-reclaim-cache-directory-unavailable".to_string())?;
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
    let mut blockers = Vec::new();
    if cache.skipped > 0 {
        blockers.push("cache-inventory-incomplete".into());
    }
    if !active_use.assessed || !active_use.evidence_complete {
        blockers.push("active-use-evidence-incomplete".into());
    } else if active_use.active {
        blockers.push("cache-is-active".into());
    }
    let plan_fingerprint = fingerprint(
        &uv_path,
        &executable_identity,
        version.stdout.trim(),
        &cache_path,
        cache.estimate.logical_bytes,
        cache.estimate.allocated_bytes,
        cache.skipped,
        &active_use,
        &blockers,
    );
    Ok(UvCacheReclaimPlan {
        schema_version: SCHEMA_VERSION,
        executable_path: uv_path.to_string_lossy().into_owned(),
        executable_identity,
        uv_version: version.stdout.trim().into(),
        cache_path: cache_path.to_string_lossy().into_owned(),
        cache_logical_bytes: cache.estimate.logical_bytes,
        cache_allocated_bytes: cache.estimate.allocated_bytes,
        cache_entries_skipped: cache.skipped,
        active_use,
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

fn valid_author(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

fn valid_rationale(value: &str) -> bool {
    value == value.trim()
        && !value.is_empty()
        && value.chars().count() <= 1_000
        && !value.chars().any(char::is_control)
}

fn attempt_id(plan_fingerprint: &str, executed_at_ms: u64) -> Result<String, String> {
    let mut nonce = [0u8; 16];
    getrandom::fill(&mut nonce)
        .map_err(|_| "uv-cache-reclaim-attempt-id-unavailable".to_string())?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-uv-cache-reclaim-attempt-v1\0");
    hasher.update(plan_fingerprint.as_bytes());
    hasher.update(&executed_at_ms.to_le_bytes());
    hasher.update(&nonce);
    Ok(hasher.finalize().to_hex().to_string())
}

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
    if !valid_author(approved_by) || !valid_rationale(rationale) {
        return Err("uv-cache-reclaim-approval-attribution-invalid".into());
    }
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
    #[test]
    fn live_cache_handle_vetoes_native_prune() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let cache = temp.path().join("cache");
        std::fs::create_dir(&cache).unwrap();
        let lock = cache.join(".lock");
        std::fs::write(&lock, b"").unwrap();
        let _open = std::fs::File::open(&lock).unwrap();
        let uv = temp.path().join("uv");
        std::fs::write(
            &uv,
            format!(
                "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'uv 0.test'; exit 0; fi\nif [ \"$1 $2\" = \"cache dir\" ]; then echo '{}'; exit 0; fi\nexit 99\n",
                cache.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&uv, std::fs::Permissions::from_mode(0o700)).unwrap();

        let plan = plan_uv_cache_reclaim(&uv, 1).unwrap();

        assert!(!plan.eligible());
        assert_eq!(plan.blockers, vec!["cache-is-active"]);
        assert!(plan.active_use.active);
    }
}
