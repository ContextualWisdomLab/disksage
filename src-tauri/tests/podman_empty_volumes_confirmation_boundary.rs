#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

#[test]
fn missing_confirmation_rejects_before_any_podman_probe() {
    let temp = tempfile::tempdir().expect("temporary test directory should be creatable");
    let marker = temp.path().join("podman-invoked");
    let fake_podman = temp.path().join("podman");
    fs::write(
        &fake_podman,
        format!("#!/bin/sh\nprintf invoked > '{}'\nexit 1\n", marker.display()),
    )
    .expect("fake Podman should be writable");
    let mut permissions = fs::metadata(&fake_podman)
        .expect("fake Podman metadata should be readable")
        .permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&fake_podman, permissions)
        .expect("fake Podman should be executable");

    let output = Command::new(env!("CARGO_BIN_EXE_disksage-podman-empty-volumes"))
        .arg("--execute")
        .arg("--rationale")
        .arg("reviewed cleanup")
        .arg("--podman-bin")
        .arg(&fake_podman)
        .output()
        .expect("Podman empty-volume CLI should start");

    assert!(!output.status.success());
    assert!(
        !marker.exists(),
        "missing destructive confirmation must fail before invoking Podman"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--execute requires --confirmation-phrase"));
}
