use serde_json::Value;
use std::process::Command;

#[cfg(unix)]
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

#[cfg(target_os = "linux")]
#[test]
fn failed_native_trim_preserves_receipt_but_exits_nonzero() {
    use std::os::unix::fs::PermissionsExt;

    let temp = unique_temp_dir("runtime-storage-cli");
    let fake_bin = temp.join("bin");
    let home = temp.join("home");
    std::fs::create_dir_all(&fake_bin).expect("create fake bin");
    std::fs::create_dir_all(&home).expect("create fake home");

    let colima = fake_bin.join("colima");
    let podman_marker = temp.join("podman-invoked");
    let podman = fake_bin.join("podman");
    std::fs::write(
        &podman,
        format!("#!/bin/sh\ntouch '{}'\nexit 99\n", podman_marker.display()),
    )
    .expect("write fake podman");
    std::fs::set_permissions(&podman, std::fs::Permissions::from_mode(0o755))
        .expect("make fake podman executable");
    std::fs::write(
        &colima,
        r#"#!/bin/sh
case "$*" in
  "--version") exit 0 ;;
  "status --json") printf '%s\n' '{"status":"running"}'; exit 0 ;;
  "ssh -- true") exit 0 ;;
  "ssh -- sudo fstrim -av") printf '%s\n' 'bounded trim output'; printf '%s\n' 'simulated trim failure' >&2; exit 7 ;;
  *) exit 64 ;;
esac
"#,
    )
    .expect("write fake colima");
    std::fs::set_permissions(&colima, std::fs::Permissions::from_mode(0o755))
        .expect("make fake colima executable");

    let inherited_path = std::env::var_os("PATH").unwrap_or_default();
    let mut path_entries = vec![fake_bin.clone()];
    path_entries.extend(std::env::split_paths(&inherited_path));
    let joined_path = std::env::join_paths(path_entries).expect("construct PATH");

    let binary = env!("CARGO_BIN_EXE_disksage-runtime-storage");
    let plan_output = Command::new(binary)
        .args(["--runtime", "colima"])
        .env("PATH", &joined_path)
        .env("HOME", &home)
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
    assert!(
        !podman_marker.exists(),
        "selecting Colima must not invoke Podman"
    );

    let execute_output = Command::new(binary)
        .args([
            "--runtime",
            "colima",
            "--execute",
            "--confirm",
            &phrase,
            "--rationale",
            "verify native failure propagation",
        ])
        .env("PATH", &joined_path)
        .env("HOME", &home)
        .output()
        .expect("run trim execution");

    assert!(
        !execute_output.status.success(),
        "native trim failure must be visible in the CLI exit status"
    );
    let receipt: Value = serde_json::from_slice(&execute_output.stdout)
        .expect("failed execution still preserves the JSON receipt");
    assert_eq!(receipt["status_code"], 7);
    assert_eq!(receipt["executed"], false);
    assert_eq!(receipt["stdout"], "bounded trim output\n");
    assert_eq!(receipt["stderr"], "simulated trim failure\n");
    assert!(
        String::from_utf8_lossy(&execute_output.stderr)
            .find("panicked")
            .is_none(),
        "native command failure must remain an ordinary controlled CLI outcome"
    );

    let _ = std::fs::remove_dir_all(temp);
}

#[cfg(unix)]
#[test]
fn non_utf8_argument_fails_without_panicking_or_reflecting_payload() {
    use std::os::unix::ffi::OsStringExt;

    let opaque = std::ffi::OsString::from_vec(vec![b'-', b'-', 0xff, b'x']);
    let output = Command::new(env!("CARGO_BIN_EXE_disksage-runtime-storage"))
        .arg(opaque)
        .output()
        .expect("run runtime-storage CLI");

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("bounded diagnostics are UTF-8");
    assert_eq!(stderr, "disksage-runtime-storage: argument-not-utf8\n");
    assert!(!stderr.contains("panicked"));
}
