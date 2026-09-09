#![cfg(windows)]

#[path = "../src/windows_process_tree.rs"]
mod windows_process_tree;

use std::fs;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const PARENT_ENV: &str = "DISKSAGE_WINDOWS_IMMEDIATE_PARENT";
const GRANDCHILD_ENV: &str = "DISKSAGE_WINDOWS_IMMEDIATE_GRANDCHILD";
const PID_FILE_ENV: &str = "DISKSAGE_WINDOWS_IMMEDIATE_PID_FILE";

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
fn immediate_grandchild_fixture() {
    if std::env::var_os(GRANDCHILD_ENV).is_none() {
        return;
    }
    thread::sleep(Duration::from_secs(30));
}

#[test]
fn immediate_parent_fixture() {
    if std::env::var_os(PARENT_ENV).is_none() {
        return;
    }
    let pid_file = std::path::PathBuf::from(
        std::env::var_os(PID_FILE_ENV).expect("parent fixture requires a PID file"),
    );
    let executable = std::env::current_exe().expect("test executable path must be available");
    let grandchild = Command::new(executable)
        .arg("immediate_grandchild_fixture")
        .arg("--nocapture")
        .env(GRANDCHILD_ENV, "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("immediate descendant must spawn");
    fs::write(&pid_file, grandchild.id().to_string()).expect("descendant PID must be recorded");
    thread::sleep(Duration::from_secs(30));
}

#[test]
fn suspended_launch_contains_descendant_before_user_code_can_escape() {
    let root = std::env::temp_dir().join(format!(
        "disksage-windows-suspended-tree-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("fixture directory must be creatable");
    let pid_file = root.join("grandchild.pid");
    let executable = std::env::current_exe().expect("test executable path must be available");
    let mut command = Command::new(executable);
    command
        .arg("immediate_parent_fixture")
        .arg("--nocapture")
        .env(PARENT_ENV, "1")
        .env(PID_FILE_ENV, &pid_file)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    windows_process_tree::ProcessTreeGuard::prepare_suspended(&mut command);
    let mut child = command.spawn().expect("suspended parent fixture must spawn");
    let guard = windows_process_tree::ProcessTreeGuard::attach_and_resume(&child)
        .expect("process-tree control must attach before the parent executes");

    assert!(
        wait_for_path(&pid_file, Duration::from_secs(5)),
        "resumed parent did not record its immediate descendant"
    );
    let descendant_id: u32 = fs::read_to_string(&pid_file)
        .expect("descendant PID must be readable")
        .trim()
        .parse()
        .expect("descendant PID must be numeric");

    drop(guard);
    let _ = child.wait();

    let started = Instant::now();
    while process_is_running(descendant_id) && started.elapsed() < Duration::from_secs(2) {
        thread::sleep(Duration::from_millis(50));
    }
    if process_is_running(descendant_id) {
        force_kill(descendant_id);
        panic!(
            "suspended launch let descendant PID {descendant_id} escape the kill-on-close Job Object"
        );
    }
    let _ = fs::remove_dir_all(root);
}
