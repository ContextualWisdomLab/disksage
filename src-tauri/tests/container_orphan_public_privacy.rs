use std::process::Command;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[cfg(unix)]
#[test]
fn shipped_orphan_plan_cli_redacts_runtime_stderr_from_json() {
    let temp = tempfile::tempdir().expect("temporary runtime directory");
    let runtime = temp.path().join("docker");
    let secret = "/Users/customer/private/docker.sock bearer-secret-token";
    std::fs::write(
        &runtime,
        format!(
            "#!/bin/sh\nset -eu\nprintf '%s\\n' '{}' >&2\nexit 17\n",
            secret
        ),
    )
    .expect("write fake Docker runtime");
    let mut permissions = std::fs::metadata(&runtime)
        .expect("fake runtime metadata")
        .permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&runtime, permissions).expect("make fake runtime executable");

    let output = Command::new(env!("CARGO_BIN_EXE_disksage-container-orphan-plan"))
        .arg("--runtime")
        .arg("docker-native")
        .arg("--bin")
        .arg(&runtime)
        .output()
        .expect("run shipped container orphan plan CLI");

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).expect("machine-readable UTF-8 evidence");
    let document: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON plan");
    assert_eq!(
        document["runtime"]["detail_issue"],
        serde_json::json!("runtime-info-failed")
    );
    assert_eq!(
        document["issues"],
        serde_json::json!(["runtime-info-failed"])
    );
    assert!(!stdout.contains(secret));
    assert!(!stdout.contains("private/docker.sock"));
    assert!(!stdout.contains("bearer-secret-token"));
}
