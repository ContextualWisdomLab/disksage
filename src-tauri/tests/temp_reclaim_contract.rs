use std::fs;
use std::process::Command;

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
    let project = temp.path().join("project");
    fs::create_dir_all(project.join("target")).unwrap();
    fs::write(project.join("Cargo.toml"), b"[package]\nname='fixture'\nversion='0.0.0'\n").unwrap();
    fs::write(project.join("target/output"), b"generated").unwrap();

    let tools = temp.path().join("tools");
    fs::create_dir(&tools).unwrap();
    let lsof = tools.join("lsof");
    let ps = tools.join("ps");
    fs::write(&lsof, b"#!/bin/sh\nexit 1\n").unwrap();
    fs::write(&ps, b"#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&lsof, fs::Permissions::from_mode(0o700)).unwrap();
    fs::set_permissions(&ps, fs::Permissions::from_mode(0o700)).unwrap();
    let existing_path = std::env::var_os("PATH").unwrap_or_default();
    let mut joined = tools.into_os_string();
    joined.push(":");
    joined.push(existing_path);

    let output = run_plan_with_tmp(temp.path(), Some(&joined));
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
    assert_eq!(
        candidate["exact_approval_phrase"].as_str(),
        Some(format!("MOVE GENERATED TEMP ARTIFACT {fingerprint} TO TRASH").as_str()),
        "an operator must be able to execute from plan output without reconstructing hidden text"
    );
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
