#![cfg(target_os = "windows")]

use serde_json::Value;
use std::fs;
use std::io::Write;
use std::process::Command;

#[test]
fn userprofile_only_windows_environment_still_discovers_build_roots() {
    let temp = tempfile::tempdir().expect("create fixture root");
    let home = temp.path().join("home");
    let appdata = home.join("AppData/Roaming");
    let project = temp.path().join("cargo-app");
    let target = project.join("target");
    fs::create_dir_all(&appdata).expect("create appdata");
    fs::create_dir_all(&target).expect("create target");
    fs::write(
        project.join("Cargo.toml"),
        b"[package]\nname='fixture'\nversion='0.1.0'\n",
    )
    .expect("write Cargo marker");
    fs::write(project.join("Cargo.lock"), b"version = 4\n").expect("write Cargo lock");
    let mut generated = fs::File::create(target.join("generated.bin"))
        .expect("create generated sparse fixture");
    generated.write_all(&[0x5a; 4096]).expect("write allocated prefix");
    generated.set_len(64 * 1024 * 1024).expect("extend sparse fixture");

    let output = Command::new(env!("CARGO_BIN_EXE_disksage-dev-artifacts"))
        .args(["--root", temp.path().to_str().expect("UTF-8 temp path")])
        .env_remove("HOME")
        .env("USERPROFILE", &home)
        .env("APPDATA", &appdata)
        .output()
        .expect("run development artifact inventory");

    assert!(
        output.status.success(),
        "inventory failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).expect("inventory JSON");
    assert_eq!(report["candidate_count"], 1);
    assert_eq!(report["candidates"][0]["kind"], "target");
    assert_eq!(report["candidates"][0]["project"], "cargo-app");
    let logical = report["candidates"][0]["bytes"].as_u64().expect("logical bytes");
    let allocated = report["candidates"][0]["allocated_bytes"]
        .as_u64()
        .expect("allocated bytes");
    assert_eq!(logical, 64 * 1024 * 1024);
    assert!(allocated > 0 && allocated < logical);

    let home_text = home.to_string_lossy();
    let fallback = Command::new(env!("CARGO_BIN_EXE_disksage-dev-artifacts"))
        .args(["--root", temp.path().to_str().expect("UTF-8 temp path")])
        .env_remove("HOME")
        .env_remove("USERPROFILE")
        .env("HOMEDRIVE", &home_text[..2])
        .env("HOMEPATH", &home_text[2..])
        .env("APPDATA", &appdata)
        .output()
        .expect("run development artifact inventory with drive/path fallback");
    assert!(
        fallback.status.success(),
        "fallback inventory failed: {}",
        String::from_utf8_lossy(&fallback.stderr)
    );
    let fallback_report: Value =
        serde_json::from_slice(&fallback.stdout).expect("fallback inventory JSON");
    assert_eq!(fallback_report["candidate_count"], 1);
}
