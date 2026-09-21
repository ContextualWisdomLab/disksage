#![cfg(windows)]

use std::fs;
use std::fs::OpenOptions;
use std::os::windows::fs::OpenOptionsExt;
use std::process::Command;

fn create_project_with_target(name: &str) -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let root = tempfile::tempdir().expect("temp root");
    let project = root.path().join("project");
    let target = project.join("target");
    let artifact = target.join("artifact.bin");

    fs::create_dir_all(&target).expect("target dir");
    fs::write(
        project.join("Cargo.toml"),
        format!(
            "[package]\nname=\"{name}\"\nversion=\"0.1.0\"\nedition=\"2021\"\n"
        ),
    )
    .expect("manifest");
    fs::write(&artifact, vec![0x5a; 4096]).expect("artifact");

    (root, project, artifact)
}

fn run_reclaim(project: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_disksage-cargo-target-clean"))
        .arg("--project-dir")
        .arg(project)
        .output()
        .expect("run cargo target clean CLI")
}

#[test]
fn idle_windows_target_reclaim_uses_native_authority_and_cleans() {
    let (_root, project, artifact) = create_project_with_target("windows-cargo-reclaim-idle");
    let output = run_reclaim(&project);

    assert!(
        output.status.success(),
        "an idle owned Windows target must reach the native deletion authority; status={:?}, stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("CLI emits JSON receipt");
    assert_eq!(receipt["executed"], true);
    assert!(
        receipt["observed_reduction_bytes"].as_u64().unwrap_or(0) >= 4096,
        "reclaim receipt must report the removed real artifact"
    );
    assert!(
        !artifact.exists(),
        "the reviewed target artifact must be removed after successful reclaim"
    );
}

#[test]
fn active_windows_target_holder_refuses_before_mutation() {
    let (_root, project, artifact) = create_project_with_target("windows-cargo-reclaim-active");
    let holder = OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0)
        .open(&artifact)
        .expect("open artifact with no sharing");

    let output = run_reclaim(&project);
    drop(holder);

    assert!(
        !output.status.success(),
        "an actively held Windows target must be refused"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("cargo-target-active-holders-present"),
        "refusal must come from the native active-use authority, not a generic unsupported/error path; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        artifact.is_file(),
        "active-holder refusal must preserve the reviewed artifact"
    );
}
