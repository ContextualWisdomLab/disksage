//! Evidence-bound reclamation for inactive Gradle daemon log files.

use serde::Serialize;
use std::fs;
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
    /// Audit durability warning emitted only after the filesystem mutation completed.
    pub audit_warning: Option<String>,
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

fn active_use_blocker(path: &Path) -> Option<&'static str> {
    let evidence = crate::git_worktree::active_use_evidence(path, ACTIVE_USE_TIMEOUT_MS, 1, false);
    if !evidence.assessed || !evidence.evidence_complete {
        Some("gradle-daemon-log-open-handle-evidence-incomplete")
    } else if evidence.active {
        Some("gradle-daemon-log-open-handle-detected")
    } else {
        None
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_millis() as u64)
        .unwrap_or(0)
}

fn journal(
    journal_path: &Path,
    candidate: &GradleDaemonLogCandidate,
    outcome: &str,
) -> Result<(), String> {
    crate::safety::journal_append(
        journal_path,
        &crate::safety::JournalEntry {
            ts_ms: now_ms(),
            op: "permanent_gradle_daemon_log_delete".into(),
            path: candidate.path.to_string_lossy().into_owned(),
            bytes: candidate.bytes,
            outcome: outcome.into(),
        },
    )
    .map_err(|_| "gradle-daemon-log-journal-write-failed".to_string())
}

fn staging_path(candidate: &GradleDaemonLogCandidate) -> Result<PathBuf, String> {
    let parent = candidate
        .path
        .parent()
        .ok_or("gradle-daemon-log-parent-unavailable")?;
    for serial in 0..16u8 {
        let path = parent.join(format!(
            ".disksage-gradle-daemon-log-{}-{}-{serial}",
            candidate.pid,
            now_ms()
        ));
        if !path.exists() {
            return Ok(path);
        }
    }
    Err("gradle-daemon-log-staging-name-unavailable".into())
}

fn staged_unchanged(candidate: &GradleDaemonLogCandidate, staged: &Path) -> bool {
    observed(staged, candidate.pid).is_ok_and(|fresh| {
        fresh.pid == candidate.pid
            && fresh.object_id == candidate.object_id
            && fresh.bytes == candidate.bytes
            && fresh.modified_ms == candidate.modified_ms
    })
}

fn restore_staged(staged: &Path, original: &Path) -> Result<(), String> {
    if original.exists() {
        return Err("gradle-daemon-log-restore-path-occupied".into());
    }
    fs::rename(staged, original).map_err(|_| "gradle-daemon-log-restore-failed".into())
}

/// Plans only direct Gradle daemon logs whose filename PID is no longer live and which have no
/// open file handle. Other daemon state, registry files, and locks are never candidates.
pub fn plan_gradle_daemon_logs(root: &Path) -> Result<Vec<GradleDaemonLogCandidate>, String> {
    let metadata = match fs::symlink_metadata(root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err("gradle-daemon-root-unreadable".into()),
    };
    if !metadata.file_type().is_dir() {
        return Err("gradle-daemon-root-not-directory".into());
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
            if !pid_is_live(pid) && active_use_blocker(&path).is_none() {
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
    let mut receipts = Vec::with_capacity(candidates.len());
    let root = candidates
        .first()
        .and_then(|candidate| candidate.path.parent()?.parent());
    for candidate in candidates {
        journal(journal_path, candidate, "pending")?;
        let blocker = if root != candidate.path.parent().and_then(Path::parent) {
            Some("gradle-daemon-log-root-mismatch")
        } else if daemon_pid(&candidate.path) != Some(candidate.pid) {
            Some("gradle-daemon-log-pid-identity-mismatch")
        } else if pid_is_live(candidate.pid) {
            Some("gradle-daemon-log-pid-live")
        } else if let Some(reason) = active_use_blocker(&candidate.path) {
            Some(reason)
        } else if !unchanged(candidate) {
            Some("gradle-daemon-log-filesystem-identity-changed")
        } else {
            None
        };
        let (removed, reason) = if let Some(reason) = blocker {
            (false, reason.to_string())
        } else {
            let staged = match staging_path(candidate) {
                Ok(path) => path,
                Err(reason) => {
                    let outcome = format!("error:{reason}");
                    journal(journal_path, candidate, &outcome)?;
                    receipts.push(GradleDaemonLogReceipt {
                        path: candidate.path.clone(),
                        pid: candidate.pid,
                        bytes: candidate.bytes,
                        removed: false,
                        reason,
                        audit_warning: None,
                    });
                    continue;
                }
            };
            if fs::rename(&candidate.path, &staged).is_err() {
                (false, "gradle-daemon-log-stage-failed".into())
            } else if !staged_unchanged(candidate, &staged) {
                let reason = restore_staged(&staged, &candidate.path)
                    .err()
                    .unwrap_or_else(|| "gradle-daemon-log-staged-identity-changed".into());
                (false, reason)
            } else if let Some(reason) = active_use_blocker(&staged) {
                let restore = restore_staged(&staged, &candidate.path);
                (false, restore.err().unwrap_or_else(|| reason.into()))
            } else {
                fs::remove_file(&staged)
                    .map(|_| (true, "inactive-gradle-daemon-log-removed".into()))
                    .unwrap_or_else(|_| {
                        let restore = restore_staged(&staged, &candidate.path);
                        (
                            false,
                            restore
                                .err()
                                .unwrap_or_else(|| "gradle-daemon-log-remove-failed".into()),
                        )
                    })
            }
        };
        let outcome = if removed {
            "ok".to_string()
        } else {
            format!("error:{reason}")
        };
        let audit_warning = match journal(journal_path, candidate, &outcome) {
            Ok(()) => None,
            Err(error) if removed => Some(error),
            Err(error) => return Err(error),
        };
        let receipt = GradleDaemonLogReceipt {
            path: candidate.path.clone(),
            pid: candidate.pid,
            bytes: candidate.bytes,
            removed,
            reason,
            audit_warning,
        };
        receipts.push(receipt);
        if receipts
            .last()
            .is_some_and(|receipt| receipt.audit_warning.is_some())
        {
            break;
        }
    }
    Ok(receipts)
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn missing_daemon_history_is_an_empty_plan() {
        let temp = tempfile::tempdir().unwrap();

        let plan = plan_gradle_daemon_logs(&temp.path().join("daemon")).unwrap();

        assert!(plan.is_empty());
    }

    #[test]
    fn stale_daemon_log_is_revalidated_removed_and_journaled() {
        let temp = tempfile::tempdir().unwrap();
        let daemon_root = temp.path().join("daemon");
        let version = daemon_root.join("8.0");
        fs::create_dir_all(&version).unwrap();
        let log = version.join("daemon-2147483647.out.log");
        fs::write(&log, b"stale daemon output").unwrap();
        let live_log = version.join(format!("daemon-{}.out.log", std::process::id()));
        fs::write(&live_log, b"active daemon output").unwrap();
        fs::write(version.join("registry.bin"), b"retain").unwrap();
        let plan = plan_gradle_daemon_logs(&daemon_root).unwrap();
        assert_eq!(plan.len(), 1);
        let journal = temp.path().join("evidence/journal.jsonl");
        let receipts = execute_gradle_daemon_logs(&plan, &journal).unwrap();
        assert!(receipts[0].removed, "{receipts:?}");
        assert!(!log.exists());
        assert!(live_log.exists());
        assert!(version.join("registry.bin").exists());
        let history = crate::safety::journal_recent(&journal, 10);
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].op, "permanent_gradle_daemon_log_delete");
        assert_eq!(history[0].outcome, "ok");
        assert_eq!(history[1].outcome, "pending");
    }
}
