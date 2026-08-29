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
