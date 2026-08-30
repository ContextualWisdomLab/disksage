#![cfg(target_os = "windows")]

use serde_json::Value;
use std::ffi::c_void;
use std::fs;
use std::io::Write;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::AsRawHandle;
use std::process::Command;

fn mark_sparse(file: &fs::File) {
    const FSCTL_SET_SPARSE: u32 = 0x0009_00c4;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn DeviceIoControl(
            device: *mut c_void,
            control_code: u32,
            input: *mut c_void,
            input_size: u32,
            output: *mut c_void,
            output_size: u32,
            bytes_returned: *mut u32,
            overlapped: *mut c_void,
        ) -> i32;
    }

    let mut bytes_returned = 0_u32;
    // SAFETY: `file` remains open for the call, the handle is the file handle returned by the
    // standard library, and FSCTL_SET_SPARSE accepts null input/output buffers to mark a file sparse.
    let result = unsafe {
        DeviceIoControl(
            file.as_raw_handle().cast(),
            FSCTL_SET_SPARSE,
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            0,
            &mut bytes_returned,
            std::ptr::null_mut(),
        )
    };
    assert_ne!(result, 0, "mark generated fixture sparse");
}

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
    mark_sparse(&generated);
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

#[test]
fn long_windows_build_paths_keep_physical_allocation_evidence_complete() {
    let temp = tempfile::tempdir().expect("create fixture root");
    let home = temp.path().join("home");
    let appdata = home.join("AppData/Roaming");
    let project = temp.path().join("long-cargo-app");
    let target = project.join("target");
    fs::create_dir_all(&appdata).expect("create appdata");
    fs::create_dir_all(&target).expect("create target");
    fs::write(
        project.join("Cargo.toml"),
        b"[package]\nname='long-fixture'\nversion='0.1.0'\n",
    )
    .expect("write Cargo marker");
    fs::write(project.join("Cargo.lock"), b"version = 4\n").expect("write Cargo lock");

    let segment = format!("segment-{}", "x".repeat(52));
    let mut deep = target.clone();
    for index in 0..5 {
        deep.push(format!("{index}-{segment}"));
    }
    fs::create_dir_all(&deep).expect("create long build path");
    let generated_path = deep.join("generated.bin");
    fs::write(&generated_path, [0x5a; 4096]).expect("write long-path generated file");
    assert!(
        generated_path.as_os_str().encode_wide().count() > 260,
        "fixture must exceed legacy MAX_PATH: {}",
        generated_path.display()
    );

    let output = Command::new(env!("CARGO_BIN_EXE_disksage-dev-artifacts"))
        .args(["--root", temp.path().to_str().expect("UTF-8 temp path")])
        .env_remove("HOME")
        .env("USERPROFILE", &home)
        .env("APPDATA", &appdata)
        .output()
        .expect("run development artifact inventory");

    assert!(
        output.status.success(),
        "long-path inventory failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).expect("inventory JSON");
    assert_eq!(report["candidate_count"], 1);
    assert_eq!(report["candidates"][0]["kind"], "target");
    assert_eq!(report["candidates"][0]["project"], "long-cargo-app");
    assert_eq!(report["candidates"][0]["scan_complete"], true);
    assert_eq!(report["candidates"][0]["skipped"], 0);
    assert_eq!(report["candidates"][0]["files"], 1);
    assert!(
        report["candidates"][0]["allocated_bytes"]
            .as_u64()
            .is_some_and(|bytes| bytes > 0)
    );
}

#[test]
fn absolute_windows_root_with_parent_component_is_rejected_before_inventory() {
    let temp = tempfile::tempdir().expect("create fixture root");
    let home = temp.path().join("home");
    let appdata = home.join("AppData/Roaming");
    let workspace = temp.path().join("workspace");
    let detour = workspace.join("detour");
    let project = workspace.join("cargo-app");
    let target = project.join("target");
    fs::create_dir_all(&appdata).expect("create appdata");
    fs::create_dir_all(&detour).expect("create detour directory");
    fs::create_dir_all(&target).expect("create target");
    fs::write(
        project.join("Cargo.toml"),
        b"[package]\nname='dot-fixture'\nversion='0.1.0'\n",
    )
    .expect("write Cargo marker");
    fs::write(project.join("Cargo.lock"), b"version = 4\n").expect("write Cargo lock");
    fs::write(target.join("generated.bin"), [0x5a; 4096]).expect("write generated file");

    let root_with_parent = detour.join("..");
    assert!(root_with_parent.is_absolute());
    assert!(root_with_parent.to_string_lossy().contains(".."));
    let output = Command::new(env!("CARGO_BIN_EXE_disksage-dev-artifacts"))
        .args(["--root", root_with_parent.to_str().expect("UTF-8 root path")])
        .env_remove("HOME")
        .env("USERPROFILE", &home)
        .env("APPDATA", &appdata)
        .output()
        .expect("run development artifact inventory through parent component");

    assert!(
        !output.status.success(),
        "lexical parent traversal must fail closed before inventory"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("--root는 존재하는 절대 디렉터리여야 함"),
        "unexpected stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
