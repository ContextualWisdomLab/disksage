#![cfg(target_os = "linux")]

use serde_json::Value;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

fn unique_temp_dir(label: &str) -> std::path::PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let path =
        std::env::temp_dir().join(format!("disksage-{label}-{}-{nonce}", std::process::id()));
    std::fs::create_dir_all(&path).expect("create temp directory");
    path
}

#[test]
fn trim_does_not_credit_a_replaced_podman_backing_file() {
    let temp = unique_temp_dir("runtime-storage-image-identity");
    let fake_bin = temp.join("bin");
    let home = temp.join("home");
    let config_dir = temp.join("podman-config");
    let image = temp.join("podman-machine.raw");
    std::fs::create_dir_all(&fake_bin).expect("create fake bin");
    std::fs::create_dir_all(&home).expect("create fake home");
    std::fs::create_dir_all(&config_dir).expect("create fake Podman config directory");
    std::fs::write(&image, vec![0x5a; 1024 * 1024]).expect("create allocated image fixture");
    std::fs::write(
        config_dir.join("podman-machine-default.json"),
        format!(r#"{{"ImagePath":{{"Path":{}}}}}"#, serde_json::to_string(&image.to_string_lossy()).unwrap()),
    )
    .expect("write machine config");

    let podman = fake_bin.join("podman");
    let machine_record = serde_json::json!([{
        "ConfigDir": {"Path": config_dir.to_string_lossy()},
        "Name": "podman-machine-default",
        "State": "running",
        "Resources": {"DiskSize": 32}
    }])
    .to_string();
    std::fs::write(
        &podman,
        format!(
            r#"#!/bin/sh
case "$*" in
  "--version") exit 0 ;;
  "machine inspect podman-machine-default --format {{.State}}") printf '%s\n' 'running'; exit 0 ;;
  "machine ssh podman-machine-default -- true") exit 0 ;;
  "--connection podman-machine-default ps --format json") printf '%s\n' '[]'; exit 0 ;;
  "machine inspect podman-machine-default") printf '%s\n' '{}'; exit 0 ;;
  "machine ssh podman-machine-default -- sudo fstrim -av")
    rm -- "$DISKSAGE_TEST_IMAGE"
    : > "$DISKSAGE_TEST_IMAGE"
    printf '%s\n' 'trim complete'
    exit 0
    ;;
  *) printf '%s\n' "unexpected fake podman invocation: $*" >&2; exit 64 ;;
esac
"#,
            machine_record
        ),
    )
    .expect("write fake podman");
    std::fs::set_permissions(&podman, std::fs::Permissions::from_mode(0o755))
        .expect("make fake podman executable");

    let inherited_path = std::env::var_os("PATH").unwrap_or_default();
    let mut path_entries = vec![fake_bin.clone()];
    path_entries.extend(std::env::split_paths(&inherited_path));
    let joined_path = std::env::join_paths(path_entries).expect("construct PATH");
    let binary = env!("CARGO_BIN_EXE_disksage-runtime-storage");

    let plan_output = Command::new(binary)
        .args(["--runtime", "podman-machine"])
        .env("PATH", &joined_path)
        .env("HOME", &home)
        .env("DISKSAGE_TEST_IMAGE", &image)
        .output()
        .expect("run read-only plan");
    assert!(
        plan_output.status.success(),
        "plan failed: {}",
        String::from_utf8_lossy(&plan_output.stderr)
    );
    let plan: Value = serde_json::from_slice(&plan_output.stdout).expect("plan JSON");
    let phrase = plan["exact_approval_phrase"]
        .as_str()
        .expect("fresh plan exposes approval phrase")
        .to_owned();

    let execute_output = Command::new(binary)
        .args([
            "--runtime",
            "podman-machine",
            "--execute",
            "--confirm",
            &phrase,
            "--rationale",
            "prove backing image identity remains attribution-bound",
        ])
        .env("PATH", &joined_path)
        .env("HOME", &home)
        .env("DISKSAGE_TEST_IMAGE", &image)
        .output()
        .expect("run trim execution");
    assert!(
        execute_output.status.success(),
        "trim command itself should succeed: {}",
        String::from_utf8_lossy(&execute_output.stderr)
    );
    let receipt: Value = serde_json::from_slice(&execute_output.stdout).expect("execution JSON");

    assert_eq!(
        receipt["runtime_image_evidence_error"],
        "runtime-storage-image-changed",
        "a replacement at the same path must invalidate attribution evidence"
    );
    assert!(receipt["runtime_image_allocated_bytes_before"].is_null());
    assert!(receipt["runtime_image_allocated_bytes_after"].is_null());
    assert!(receipt["runtime_image_reclaimed_bytes"].is_null());

    let _ = std::fs::remove_dir_all(temp);
}
