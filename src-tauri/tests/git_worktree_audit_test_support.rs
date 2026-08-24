use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const STALE_TARGET_MAX_AGE: Duration = Duration::from_secs(15 * 60);

#[cfg(unix)]
fn process_is_alive(pid: u32) -> bool {
    // SAFETY: kill(pid, 0) only probes process existence and does not deliver a signal.
    let result = unsafe { libc::kill(pid as libc::pid_t, 0) };
    result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(not(unix))]
fn process_is_alive(_pid: u32) -> bool {
    false
}

fn prune_stale_targets(root: &Path, prefix: &str, current_pid: u32) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let Some(pid_text) = name.strip_prefix(prefix) else {
            continue;
        };
        let Ok(pid) = pid_text.parse::<u32>() else {
            continue;
        };
        if pid == current_pid {
            continue;
        }
        // ponytail: non-Unix fallback uses a 15-minute age ceiling; add a native process probe if
        // cross-platform test builds ever exceed that duration.
        let stale = if process_is_alive(pid) {
            false
        } else {
            entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .ok()
                .and_then(|modified| SystemTime::now().duration_since(modified).ok())
                .is_some_and(|age| age >= STALE_TARGET_MAX_AGE)
        };
        if stale {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

pub fn new_target_dir(prefix: &str) -> PathBuf {
    let root = std::env::temp_dir();
    prune_stale_targets(&root, prefix, std::process::id());
    tempfile::Builder::new()
        .prefix(&format!("{prefix}{}-", std::process::id()))
        .tempdir_in(&root)
        .expect("Cargo target directory should be created")
        .keep()
}
