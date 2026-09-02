#![cfg(windows)]

use std::io;
use std::process::Child;

/// Owns the operating-system boundary used to terminate one spawned command and its descendants.
///
/// The initial implementation intentionally models the pre-fix behavior so the Windows regression
/// demonstrates that killing only the direct child is insufficient. The production repair replaces
/// this placeholder with a Windows Job Object whose lifetime contains the whole subprocess tree.
pub(crate) struct ProcessTreeGuard;

impl ProcessTreeGuard {
    /// Attaches lifecycle control to a newly spawned child process.
    pub(crate) fn attach(_child: &Child) -> io::Result<Self> {
        Ok(Self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::{Duration, Instant};

    const PID_FILE_ENV: &str = "DISKSAGE_WINDOWS_TREE_PID_FILE";
    const START_FILE_ENV: &str = "DISKSAGE_WINDOWS_TREE_START_FILE";
    const GRANDCHILD_ENV: &str = "DISKSAGE_WINDOWS_TREE_GRANDCHILD";

    fn wait_for_path(path: &std::path::Path, timeout: Duration) -> bool {
        let started = Instant::now();
        while started.elapsed() < timeout {
            if path.exists() {
                return true;
            }
            thread::sleep(Duration::from_millis(25));
        }
        path.exists()
    }

    fn process_is_running(process_id: u32) -> bool {
        let filter = format!("PID eq {process_id}");
        let Ok(output) = Command::new("tasklist")
            .args(["/FI", &filter, "/FO", "CSV", "/NH"])
            .output()
        else {
            return true;
        };
        if !output.status.success() {
            return true;
        }
        let text = String::from_utf8_lossy(&output.stdout);
        text.lines().any(|line| {
            line.split(',')
                .nth(1)
                .map(str::trim)
                .map(|field| field.trim_matches('"') == process_id.to_string())
                .unwrap_or(false)
        })
    }

    fn force_kill(process_id: u32) {
        let _ = Command::new("taskkill")
            .args(["/PID", &process_id.to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }

    #[test]
    fn windows_process_tree_grandchild_fixture() {
        if std::env::var_os(GRANDCHILD_ENV).is_none() {
            return;
        }
        thread::sleep(Duration::from_secs(30));
    }

    #[test]
    fn windows_process_tree_parent_fixture() {
        let Some(pid_file) = std::env::var_os(PID_FILE_ENV).map(std::path::PathBuf::from) else {
            return;
        };
        let Some(start_file) = std::env::var_os(START_FILE_ENV).map(std::path::PathBuf::from) else {
            return;
        };
        assert!(
            wait_for_path(&start_file, Duration::from_secs(5)),
            "parent fixture was never released after process-tree control attached"
        );
        let executable = std::env::current_exe().expect("test executable path must be available");
        let grandchild = Command::new(executable)
            .arg("windows_process_tree_grandchild_fixture")
            .arg("--nocapture")
            .env(GRANDCHILD_ENV, "1")
            .spawn()
            .expect("grandchild fixture must spawn");
        fs::write(&pid_file, grandchild.id().to_string()).expect("grandchild PID must be recorded");
        thread::sleep(Duration::from_secs(30));
    }

    #[test]
    fn dropping_guard_terminates_descendants_that_inherit_output_handles() {
        let root = std::env::temp_dir().join(format!(
            "disksage-windows-process-tree-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("fixture directory must be creatable");
        let pid_file = root.join("grandchild.pid");
        let start_file = root.join("start");
        let executable = std::env::current_exe().expect("test executable path must be available");
        let mut child = Command::new(executable)
            .arg("windows_process_tree_parent_fixture")
            .arg("--nocapture")
            .env(PID_FILE_ENV, &pid_file)
            .env(START_FILE_ENV, &start_file)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("parent fixture must spawn");

        let guard = ProcessTreeGuard::attach(&child).expect("process-tree control must attach");
        fs::write(&start_file, b"go").expect("parent fixture release marker must be writable");
        assert!(
            wait_for_path(&pid_file, Duration::from_secs(5)),
            "parent fixture did not record its descendant"
        );
        let descendant_id: u32 = fs::read_to_string(&pid_file)
            .expect("grandchild PID must be readable")
            .trim()
            .parse()
            .expect("grandchild PID must be numeric");

        drop(guard);
        let _ = child.kill();
        let _ = child.wait();

        let started = Instant::now();
        while process_is_running(descendant_id) && started.elapsed() < Duration::from_secs(2) {
            thread::sleep(Duration::from_millis(50));
        }
        if process_is_running(descendant_id) {
            force_kill(descendant_id);
            panic!(
                "dropping process-tree control left descendant PID {descendant_id} alive after the parent was terminated"
            );
        }
        let _ = fs::remove_dir_all(root);
    }
}
