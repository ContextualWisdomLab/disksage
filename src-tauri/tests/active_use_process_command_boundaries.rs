#![cfg(unix)]

use disksage_lib::git_worktree::active_use_evidence;
use std::fs;
use std::process::{Child, Command};

fn spawn_command_with_argument(argument: &str) -> Child {
    Command::new("sh")
        .args(["-c", "sleep 20 & wait", argument])
        .spawn()
        .expect("spawn process whose command line carries the fixture path")
}

#[test]
fn right_aligned_ps_pid_still_detects_active_cache_argument() {
    let temp = tempfile::tempdir().expect("temporary active-use fixture");
    let marker = temp.path().join("npx environment with spaces");
    fs::create_dir(&marker).expect("create cache environment fixture");
    let mut child = spawn_command_with_argument(marker.to_str().expect("utf-8 fixture path"));

    let evidence = active_use_evidence(&marker, 5_000, 64, false);
    let child_pid = child.id();
    let _ = child.kill();
    let _ = child.wait();

    assert!(evidence.evidence_complete, "{evidence:?}");
    assert!(evidence.active, "{evidence:?}");
    assert!(
        evidence.observed_pids.contains(&child_pid),
        "right-aligned ps output must not hide the process: {evidence:?}"
    );
}

#[test]
fn process_argument_path_prefix_does_not_block_unrelated_cache() {
    let temp = tempfile::tempdir().expect("temporary active-use prefix fixture");
    let marker = temp.path().join("env");
    let longer = temp.path().join("env-old");
    fs::create_dir(&marker).expect("create target environment");
    fs::create_dir(&longer).expect("create unrelated environment");
    let mut child = spawn_command_with_argument(longer.to_str().expect("utf-8 fixture path"));

    let evidence = active_use_evidence(&marker, 5_000, 64, false);
    let child_pid = child.id();
    let _ = child.kill();
    let _ = child.wait();

    assert!(evidence.evidence_complete, "{evidence:?}");
    assert!(
        !evidence.observed_pids.contains(&child_pid),
        "a longer sibling path must not count as exact active use: {evidence:?}"
    );
}

#[test]
fn option_assignment_exact_path_is_detected_as_active_use() {
    let temp = tempfile::tempdir().expect("temporary active-use option fixture");
    let marker = temp.path().join("npx-option-environment");
    fs::create_dir(&marker).expect("create option-bound cache environment");
    let argument = format!("--cache={}", marker.to_string_lossy());
    let mut child = spawn_command_with_argument(&argument);

    let evidence = active_use_evidence(&marker, 5_000, 64, false);
    let child_pid = child.id();
    let _ = child.kill();
    let _ = child.wait();

    assert!(evidence.evidence_complete, "{evidence:?}");
    assert!(evidence.active, "{evidence:?}");
    assert!(
        evidence.observed_pids.contains(&child_pid),
        "--option=/target must preserve the active-use veto: {evidence:?}"
    );
}

#[test]
fn option_assignment_recursive_descendant_is_detected_without_sibling_prefix_false_positive() {
    let temp = tempfile::tempdir().expect("temporary recursive option fixture");
    let marker = temp.path().join("worktree");
    let descendant = marker.join("nested");
    let sibling = temp.path().join("worktree-old");
    fs::create_dir_all(&descendant).expect("create recursive target fixture");
    fs::create_dir(&sibling).expect("create sibling fixture");

    let descendant_argument = format!("--cwd={}", descendant.to_string_lossy());
    let mut descendant_child = spawn_command_with_argument(&descendant_argument);
    let descendant_evidence = active_use_evidence(&marker, 5_000, 64, true);
    let descendant_pid = descendant_child.id();
    let _ = descendant_child.kill();
    let _ = descendant_child.wait();

    assert!(descendant_evidence.evidence_complete, "{descendant_evidence:?}");
    assert!(
        descendant_evidence.observed_pids.contains(&descendant_pid),
        "--option=/target/child must count as recursive active use: {descendant_evidence:?}"
    );

    let sibling_argument = format!("--cwd={}", sibling.to_string_lossy());
    let mut sibling_child = spawn_command_with_argument(&sibling_argument);
    let sibling_evidence = active_use_evidence(&marker, 5_000, 64, true);
    let sibling_pid = sibling_child.id();
    let _ = sibling_child.kill();
    let _ = sibling_child.wait();

    assert!(sibling_evidence.evidence_complete, "{sibling_evidence:?}");
    assert!(
        !sibling_evidence.observed_pids.contains(&sibling_pid),
        "--option=/target-old must not be accepted as a target boundary: {sibling_evidence:?}"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn long_process_command_still_detects_late_cache_argument() {
    let temp = tempfile::tempdir().expect("temporary long-command active-use fixture");
    let marker = temp.path().join("late-cache-environment");
    fs::create_dir(&marker).expect("create cache environment fixture");
    let filler = "x".repeat(8 * 1024);
    let mut child = Command::new("sh")
        .arg("-c")
        .arg("sleep 20 & wait")
        .arg(&filler)
        .arg(marker.to_str().expect("utf-8 fixture path"))
        .spawn()
        .expect("spawn process with target path beyond normal ps command width");

    let evidence = active_use_evidence(&marker, 5_000, 64, false);
    let child_pid = child.id();
    let _ = child.kill();
    let _ = child.wait();

    assert!(evidence.evidence_complete, "{evidence:?}");
    assert!(evidence.active, "{evidence:?}");
    assert!(
        evidence.observed_pids.contains(&child_pid),
        "macOS process command evidence must not truncate a late target path: {evidence:?}"
    );
}
