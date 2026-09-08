#![cfg(all(unix, not(target_os = "macos")))]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

#[test]
fn invalid_approval_cannot_create_journal_directories() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    fs::create_dir_all(project.join("target")).unwrap();
    fs::write(
        project.join("Cargo.toml"),
        b"[package]\nname='fixture'\nversion='0.0.0'\n",
    )
    .unwrap();
    fs::write(project.join("target/output"), b"generated").unwrap();

    let tools = temp.path().join("tools");
    fs::create_dir(&tools).unwrap();
    for name in ["lsof", "ps"] {
        let tool = tools.join(name);
        fs::write(&tool, b"#!/bin/sh\nexit 1\n").unwrap();
        fs::set_permissions(&tool, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let mut path = tools.into_os_string();
    path.push(":");
    path.push(std::env::var_os("PATH").unwrap_or_default());

    let binary = env!("CARGO_BIN_EXE_disksage-temp-reclaim");
    let plan_output = Command::new(binary)
        .env("TMPDIR", temp.path())
        .env("PATH", &path)
        .output()
        .unwrap();
    assert!(plan_output.status.success());
    let plan: serde_json::Value = serde_json::from_slice(&plan_output.stdout).unwrap();
    let candidate = plan["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|candidate| candidate["artifact"]["kind"] == "target")
        .expect("target candidate");
    let fingerprint = candidate["candidate_fingerprint"].as_str().unwrap();
    let journal = temp.path().join("untrusted/new/tree/journal.jsonl");
    assert!(!journal.parent().unwrap().exists());

    let output = Command::new(binary)
        .env("TMPDIR", temp.path())
        .env("PATH", &path)
        .arg("--execute-fingerprint")
        .arg(fingerprint)
        .arg("--approved-by")
        .arg("local:test-user")
        .arg("--approval-phrase")
        .arg("definitely-not-the-backend-phrase")
        .arg("--journal-path")
        .arg(&journal)
        .output()
        .unwrap();

    assert!(!output.status.success(), "invalid approval must fail");
    assert!(
        !journal.parent().unwrap().exists(),
        "an unvalidated request must not gain directory-creation authority"
    );
    assert!(
        project.join("target/output").is_file(),
        "invalid approval must not mutate the candidate"
    );
}
