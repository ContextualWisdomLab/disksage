#![cfg(windows)]

use std::fs;
use std::process::Command;

#[test]
fn idle_windows_target_reclaim_uses_native_authority_and_cleans() {
    let root = tempfile::tempdir().expect("temp root");
    let project = root.path().join("project");
    let target = project.join("target");
    let artifact = target.join("artifact.bin");

    fs::create_dir_all(&target).expect("target dir");
    fs::write(
        project.join("Cargo.toml"),
        "[package]\nname=\"windows-cargo-reclaim\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )
    .expect("manifest");
    fs::write(&artifact, vec![0x5a; 4096]).expect("artifact");

    let output = Command::new(env!("CARGO_BIN_EXE_disksage-cargo-target-clean"))
        .arg("--project-dir")
        .arg(&project)
        .output()
        .expect("run cargo target clean CLI");

    assert!(
        output.status.success(),
        "an idle owned Windows target must reach the native deletion authority; status={:?}, stderr={} ",
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
