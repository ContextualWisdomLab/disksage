#![cfg(target_os = "windows")]

use std::process::Command;

#[test]
fn windows_cli_keeps_native_temp_planning_read_only_when_active_use_is_unavailable() {
    let temp = tempfile::tempdir().expect("isolated Windows temp root");
    let project = temp.path().join("000000-disksage-temp-project");
    let target = project.join("target");
    std::fs::create_dir_all(&target).expect("target directory");
    std::fs::write(project.join("Cargo.toml"), b"[package]\nname='temp-project'\nversion='0.1.0'\n")
        .expect("Cargo marker");
    std::fs::write(target.join("artifact.bin"), b"generated").expect("generated artifact");

    let binary = env!("CARGO_BIN_EXE_disksage-temp-reclaim");
    let output = Command::new(binary)
        .env("TEMP", temp.path())
        .env("TMP", temp.path())
        .output()
        .expect("temp reclaim CLI executes");

    assert!(
        output.status.success(),
        "Windows must support a read-only native temp plan instead of rejecting the platform: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("plan JSON");
    let candidate = report["candidates"]
        .as_array()
        .expect("candidate array")
        .iter()
        .find(|candidate| candidate["artifact"]["kind"] == "target")
        .expect("generated Cargo target remains visible for operator review");
    assert_eq!(candidate["eligible_for_approval"], false);
    assert!(candidate["exact_approval_phrase"].is_null());
    assert!(candidate["blockers"]
        .as_array()
        .expect("blocker array")
        .iter()
        .any(|blocker| blocker == "temporary-artifact-active-use-incomplete"));
    assert!(target.exists(), "read-only planning must not mutate the candidate");
}
