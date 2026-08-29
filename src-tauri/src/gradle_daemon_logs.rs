//! Evidence-bound reclamation for inactive Gradle daemon log files.

use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

const ACTIVE_USE_TIMEOUT_MS: u64 = 30_000;
const MAX_LOGS: usize = 1_024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GradleDaemonLogCandidate {
    pub path: PathBuf,
    pub pid: u32,
    pub object_id: String,
    pub bytes: u64,
    pub modified_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GradleDaemonLogReceipt {
    pub path: PathBuf,
    pub pid: u32,
    pub bytes: u64,
    pub removed: bool,
    pub reason: String,
}

fn daemon_pid(path: &Path) -> Option<u32> {
    let pid = path
        .file_name()?
        .to_str()?
        .strip_prefix("daemon-")?
        .strip_suffix(".out.log")?
        .parse()
        .ok()?;
    (pid <= i32::MAX as u32).then_some(pid)
}

#[cfg(unix)]
fn pid_is_live(pid: u32) -> bool {
    let result = unsafe { libc::kill(pid as libc::pid_t, 0) };
    result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(not(unix))]
fn pid_is_live(_pid: u32) -> bool {
    true
}

fn observed(path: &Path, pid: u32) -> Result<GradleDaemonLogCandidate, String> {
    let metadata =
        fs::symlink_metadata(path).map_err(|_| "gradle-daemon-log-metadata-unavailable")?;
    if !metadata.file_type().is_file() {
        return Err("gradle-daemon-log-not-regular-file".into());
    }
    let object_id = crate::safety::object_id_from_metadata(&metadata)
        .ok_or("gradle-daemon-log-identity-unavailable")?;
    let modified_ms = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|value| value.as_millis() as u64)
        .ok_or("gradle-daemon-log-modified-time-unavailable")?;
    Ok(GradleDaemonLogCandidate {
        path: path.to_path_buf(),
        pid,
        object_id,
        bytes: metadata.len(),
        modified_ms,
    })
}

fn unchanged(candidate: &GradleDaemonLogCandidate) -> bool {
    observed(&candidate.path, candidate.pid)
        .map(|fresh| fresh == *candidate)
        .unwrap_or(false)
}

fn no_open_handles(root: &Path) -> Result<bool, String> {
    let evidence = crate::git_worktree::active_use_evidence(root, ACTIVE_USE_TIMEOUT_MS, 1, true);
    if !evidence.assessed || !evidence.evidence_complete {
        return Err("gradle-daemon-log-open-handle-evidence-incomplete".into());
    }
    Ok(!evidence.active)
}

/// Plans only direct Gradle daemon logs whose filename PID is no longer live and which have no
/// open file handle. Other daemon state, registry files, and locks are never candidates.
pub fn plan_gradle_daemon_logs(root: &Path) -> Result<Vec<GradleDaemonLogCandidate>, String> {
    if !fs::symlink_metadata(root)
        .map_err(|_| "gradle-daemon-root-unreadable")?
        .file_type()
        .is_dir()
    {
        return Err("gradle-daemon-root-not-directory".into());
    }
    if !no_open_handles(root)? {
        return Ok(Vec::new());
    }
    let mut candidates = Vec::new();
    let versions = fs::read_dir(root).map_err(|_| "gradle-daemon-root-unreadable")?;
    for version in versions {
        let version = version.map_err(|_| "gradle-daemon-root-entry-unreadable")?;
        if !version
            .file_type()
            .map_err(|_| "gradle-daemon-version-type-unavailable")?
            .is_dir()
        {
            continue;
        }
        for entry in fs::read_dir(version.path()).map_err(|_| "gradle-daemon-version-unreadable")? {
            let path = entry
                .map_err(|_| "gradle-daemon-log-entry-unreadable")?
                .path();
            let Some(pid) = daemon_pid(&path) else {
                continue;
            };
            if !pid_is_live(pid) {
                candidates.push(observed(&path, pid)?);
                if candidates.len() > MAX_LOGS {
                    return Err("gradle-daemon-log-limit-exceeded".into());
                }
            }
        }
    }
    candidates.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(candidates)
}

/// Permanently removes only planned daemon logs after repeating PID, handle, and identity checks.
pub fn execute_gradle_daemon_logs(
    candidates: &[GradleDaemonLogCandidate],
    journal_path: &Path,
) -> Result<Vec<GradleDaemonLogReceipt>, String> {
    if let Some(parent) = journal_path.parent() {
        fs::create_dir_all(parent).map_err(|_| "gradle-daemon-log-journal-parent-failed")?;
    }
    let mut journal = OpenOptions::new()
        .create(true)
        .append(true)
        .open(journal_path)
        .map_err(|_| "gradle-daemon-log-journal-open-failed")?;
    let mut receipts = Vec::with_capacity(candidates.len());
    let root = candidates
        .first()
        .and_then(|candidate| candidate.path.parent()?.parent());
    let handles_clear = root.map(no_open_handles).transpose()?.unwrap_or(true);
    for candidate in candidates {
        serde_json::to_writer(
            &mut journal,
            &serde_json::json!({
                "operation": "gradle_daemon_log_delete_intent",
                "candidate": candidate,
            }),
        )
        .map_err(|_| "gradle-daemon-log-journal-write-failed")?;
        journal
            .write_all(b"\n")
            .map_err(|_| "gradle-daemon-log-journal-write-failed")?;
        journal
            .sync_all()
            .map_err(|_| "gradle-daemon-log-journal-sync-failed")?;
        let blocker = if !handles_clear {
            Some("gradle-daemon-log-open-handle-detected")
        } else if root != candidate.path.parent().and_then(Path::parent) {
            Some("gradle-daemon-log-root-mismatch")
        } else if daemon_pid(&candidate.path) != Some(candidate.pid) {
            Some("gradle-daemon-log-pid-identity-mismatch")
        } else if pid_is_live(candidate.pid) {
            Some("gradle-daemon-log-pid-live")
        } else if !unchanged(candidate) {
            Some("gradle-daemon-log-filesystem-identity-changed")
        } else {
            None
        };
        let (removed, reason) = if blocker.is_none() {
            fs::remove_file(&candidate.path)
                .map(|_| (true, "inactive-gradle-daemon-log-removed".to_string()))
                .unwrap_or_else(|_| (false, "gradle-daemon-log-remove-failed".to_string()))
        } else {
            (false, blocker.unwrap().to_string())
        };
        let receipt = GradleDaemonLogReceipt {
            path: candidate.path.clone(),
            pid: candidate.pid,
            bytes: candidate.bytes,
            removed,
            reason,
        };
        serde_json::to_writer(&mut journal, &receipt)
            .map_err(|_| "gradle-daemon-log-journal-write-failed")?;
        journal
            .write_all(b"\n")
            .map_err(|_| "gradle-daemon-log-journal-write-failed")?;
        journal
            .sync_all()
            .map_err(|_| "gradle-daemon-log-journal-sync-failed")?;
        receipts.push(receipt);
    }
    Ok(receipts)
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn stale_daemon_log_is_revalidated_removed_and_journaled() {
        let temp = tempfile::tempdir().unwrap();
        let daemon_root = temp.path().join("daemon");
        let version = daemon_root.join("8.0");
        fs::create_dir_all(&version).unwrap();
        let log = version.join("daemon-2147483647.out.log");
        fs::write(&log, b"stale daemon output").unwrap();
        fs::write(version.join("registry.bin"), b"retain").unwrap();
        let plan = plan_gradle_daemon_logs(&daemon_root).unwrap();
        assert_eq!(plan.len(), 1);
        let journal = temp.path().join("evidence/journal.jsonl");
        let receipts = execute_gradle_daemon_logs(&plan, &journal).unwrap();
        assert!(receipts[0].removed, "{receipts:?}");
        assert!(!log.exists());
        assert!(version.join("registry.bin").exists());
        assert!(fs::read_to_string(journal)
            .unwrap()
            .contains("inactive-gradle-daemon-log-removed"));
    }
}
