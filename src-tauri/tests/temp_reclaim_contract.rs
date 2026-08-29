use std::fs;
use std::process::Command;

#[cfg(all(unix, not(target_os = "macos")))]
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[cfg(all(unix, not(target_os = "macos")))]
fn run_plan_with_tmp(root: &std::path::Path, path: Option<&std::ffi::OsStr>) -> std::process::Output {
    let binary = env!("CARGO_BIN_EXE_disksage-temp-reclaim");
    let mut command = Command::new(binary);
    command.env("TMPDIR", root);
    if let Some(path) = path {
        command.env("PATH", path);
    }
    command.output().unwrap()
}

#[cfg(all(unix, not(target_os = "macos")))]
fn isolated_tool_path(temp: &tempfile::TempDir, lsof_body: &[u8]) -> std::ffi::OsString {
    let tools = temp.path().join("tools");
    fs::create_dir(&tools).unwrap();
    let lsof = tools.join("lsof");
    let ps = tools.join("ps");
    fs::write(&lsof, lsof_body).unwrap();
    fs::write(&ps, b"#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&lsof, fs::Permissions::from_mode(0o700)).unwrap();
    fs::set_permissions(&ps, fs::Permissions::from_mode(0o700)).unwrap();
    let existing_path = std::env::var_os("PATH").unwrap_or_default();
    let mut joined = tools.into_os_string();
    joined.push(":");
    joined.push(existing_path);
    joined
}

#[cfg(all(unix, not(target_os = "macos")))]
fn marker_bound_target(temp: &tempfile::TempDir) {
    let project = temp.path().join("project");
    fs::create_dir_all(project.join("target")).unwrap();
    fs::write(
        project.join("Cargo.toml"),
        b"[package]\nname='fixture'\nversion='0.0.0'\n",
    )
    .unwrap();
    fs::write(project.join("target/output"), b"generated").unwrap();
}

#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn markerless_cache_names_never_gain_native_temp_authority() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    fs::create_dir_all(project.join("__pycache__")).unwrap();
    fs::create_dir_all(project.join(".codegraph")).unwrap();
    fs::write(project.join("__pycache__/module.pyc"), b"not enough authority").unwrap();
    fs::write(project.join(".codegraph/index"), b"not enough authority").unwrap();

    let output = run_plan_with_tmp(temp.path(), None);
    assert!(
        output.status.success(),
        "planning failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let candidates = plan["candidates"].as_array().unwrap();
    assert!(
        candidates.iter().all(|candidate| {
            !matches!(
                candidate["artifact"]["kind"].as_str(),
                Some("__pycache__" | ".codegraph")
            )
        }),
        "markerless cache names must remain unavailable: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn eligible_plan_serializes_every_execution_authority_value() {
    let temp = tempfile::tempdir().unwrap();
    marker_bound_target(&temp);
    let path = isolated_tool_path(&temp, b"#!/bin/sh\nexit 1\n");

    let output = run_plan_with_tmp(temp.path(), Some(&path));
    assert!(
        output.status.success(),
        "planning failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let candidate = plan["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|candidate| candidate["artifact"]["kind"] == "target")
        .expect("target candidate");
    assert_eq!(candidate["eligible_for_approval"], true);
    let fingerprint = candidate["candidate_fingerprint"].as_str().unwrap();
    let expected = format!("MOVE GENERATED TEMP ARTIFACT {fingerprint} TO TRASH");
    assert_eq!(
        candidate["exact_approval_phrase"].as_str(),
        Some(expected.as_str()),
        "an operator must be able to execute from plan output without reconstructing hidden text"
    );
}

#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn exactly_sixty_four_candidates_remain_a_complete_eligible_plan() {
    let temp = tempfile::tempdir().unwrap();
    for index in 0..64 {
        let project = temp.path().join(format!("project-{index:02}"));
        fs::create_dir_all(project.join("target")).unwrap();
        fs::write(
            project.join("Cargo.toml"),
            format!("[package]\nname='fixture-{index:02}'\nversion='0.0.0'\n"),
        )
        .unwrap();
        fs::write(project.join("target/output"), b"generated").unwrap();
    }
    let path = isolated_tool_path(&temp, b"#!/bin/sh\nexit 1\n");

    let output = run_plan_with_tmp(temp.path(), Some(&path));
    assert!(
        output.status.success(),
        "planning failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let candidates = plan["candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 64, "all bounded candidates must be returned");
    assert_eq!(plan["scan_complete"], true, "the supported boundary is inclusive");
    assert!(candidates.iter().all(|candidate| {
        candidate["eligible_for_approval"] == true
            && candidate["exact_approval_phrase"].is_string()
    }));
}

#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn active_use_probe_cannot_overrun_native_temp_discovery_budget() {
    let temp = tempfile::tempdir().unwrap();
    marker_bound_target(&temp);
    let path = isolated_tool_path(&temp, b"#!/bin/sh\nsleep 5\nexit 1\n");

    let started = Instant::now();
    let output = run_plan_with_tmp(temp.path(), Some(&path));
    let elapsed = started.elapsed();
    assert!(
        output.status.success(),
        "planning should fail closed in evidence rather than hang: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        elapsed < Duration::from_secs(4),
        "one candidate probe overran the two-second discovery budget: {elapsed:?}"
    );
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(plan["scan_complete"], false);
    assert!(plan["candidates"].as_array().unwrap().iter().all(|candidate| {
        candidate["eligible_for_approval"] == false
            && candidate["exact_approval_phrase"].is_null()
    }));
}

#[cfg(windows)]
#[test]
fn windows_cli_fails_explicitly_when_native_handle_evidence_is_unavailable() {
    let binary = env!("CARGO_BIN_EXE_disksage-temp-reclaim");
    let output = Command::new(binary).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("temporary-reclaim-platform-unsupported"),
        "Windows must not advertise an execution path that can never become eligible"
    );
}
