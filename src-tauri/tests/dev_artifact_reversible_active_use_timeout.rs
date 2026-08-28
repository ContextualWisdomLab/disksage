#![cfg(unix)]

//! Process-level regression for the interactive development-artifact cleanup timeout.
//!
//! Reversible cleanup must fail closed quickly when recursive active-use evidence stalls. The
//! irreversible path may use a longer probe budget, but the GUI-backed Trash path must not inherit
//! that latency.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use std::time::{Duration, Instant};

#[test]
fn reversible_cleanup_caps_stalled_active_use_probe() {
    let fixture = tempfile::tempdir().expect("temporary fixture");
    let root = fixture.path().join("workspace");
    let project = root.join("app");
    let artifact = project.join("node_modules");
    let fake_bin = fixture.path().join("bin");
    fs::create_dir_all(&artifact).expect("artifact directory");
    fs::create_dir(&fake_bin).expect("fake bin directory");
    fs::write(project.join("package.json"), b"{}\n").expect("project marker");
    fs::write(artifact.join("payload.bin"), b"generated").expect("artifact payload");

    let fake_lsof = fake_bin.join("lsof");
    fs::write(
        &fake_lsof,
        b"#!/bin/sh\nsleep 6\nprintf 'probe failed\\n' >&2\nexit 2\n",
    )
    .expect("fake lsof");
    fs::set_permissions(&fake_lsof, fs::Permissions::from_mode(0o755)).expect("fake lsof mode");

    let original_path = std::env::var_os("PATH").unwrap_or_default();
    let mut path_parts = vec![fake_bin.clone()];
    path_parts.extend(std::env::split_paths(&original_path));
    let child_path = std::env::join_paths(path_parts).expect("PATH composition");
    let journal = fixture.path().join("journal.jsonl");

    let started = Instant::now();
    let output = Command::new(env!("CARGO_BIN_EXE_disksage-dev-artifacts"))
        .env("PATH", child_path)
        .arg("--root")
        .arg(&root)
        .arg("--min-age-days")
        .arg("0")
        .arg("--journal-path")
        .arg(&journal)
        .arg("--execute")
        .output()
        .expect("development-artifact CLI should start");
    let elapsed = started.elapsed();

    assert!(output.status.success(), "CLI should return a bounded result report");
    assert!(
        elapsed < Duration::from_millis(4_500),
        "reversible cleanup inherited a destructive-path active-use timeout: {elapsed:?}"
    );

    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON report");
    let result = report["results"]
        .as_array()
        .and_then(|results| results.first())
        .expect("one cleanup result");
    assert_eq!(result["ok"], false);
    assert_eq!(
        result["error"],
        "development artifact active-use evidence incomplete; rescan before cleanup"
    );
    assert!(artifact.exists(), "timeout must fail closed before Trash mutation");
    assert!(!journal.exists(), "timeout must not create mutation evidence");
}
