#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_disksage-podman-storage-repair")
}

#[test]
fn sole_help_is_successful_but_mixed_help_fails_before_runtime_work() {
    let help = Command::new(binary())
        .arg("--help")
        .output()
        .expect("launch help");
    assert!(help.status.success());
    assert!(help.stderr.is_empty());
    assert!(String::from_utf8(help.stdout)
        .expect("UTF-8 help")
        .starts_with("Usage: disksage-podman-storage-repair "));

    let temp = tempfile::tempdir().expect("temporary fake runtime");
    let fake_podman = temp.path().join("podman");
    let marker = temp.path().join("invoked");
    fs::write(
        &fake_podman,
        format!("#!/bin/sh\ntouch '{}'\nexit 99\n", marker.display()),
    )
    .expect("write fake Podman");
    let mut permissions = fs::metadata(&fake_podman).unwrap().permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&fake_podman, permissions).unwrap();

    let mixed = Command::new(binary())
        .args([
            "--podman-bin",
            fake_podman.to_str().unwrap(),
            "--execute",
            "--help",
        ])
        .output()
        .expect("launch mixed help");
    assert!(!mixed.status.success(), "help is terminal only as the sole argument");
    assert!(mixed.stdout.is_empty());
    assert!(!marker.exists(), "invalid mixed help must not invoke Podman");
}

#[test]
fn unknown_options_are_bounded_and_not_reflected() {
    let sentinel = "--secret-option=/Users/private/customer-path";
    let output = Command::new(binary())
        .arg(sentinel)
        .output()
        .expect("launch unknown option");
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("bounded UTF-8 diagnostic");
    assert!(!stderr.contains(sentinel));
    assert!(stderr.len() <= 256, "diagnostic remains small and bounded");
}

#[test]
fn duplicate_singleton_options_fail_before_runtime_work() {
    let temp = tempfile::tempdir().expect("temporary fake runtime");
    let fake_podman = temp.path().join("podman");
    let marker = temp.path().join("invoked");
    fs::write(
        &fake_podman,
        format!("#!/bin/sh\ntouch '{}'\nexit 99\n", marker.display()),
    )
    .expect("write fake Podman");
    let mut permissions = fs::metadata(&fake_podman).unwrap().permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&fake_podman, permissions).unwrap();

    let output = Command::new(binary())
        .args([
            "--podman-bin",
            fake_podman.to_str().unwrap(),
            "--machine",
            "podman-machine-default",
            "--machine",
            "other-machine",
        ])
        .output()
        .expect("launch duplicate singleton request");
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(!marker.exists(), "duplicate authority options must fail before Podman");
}
