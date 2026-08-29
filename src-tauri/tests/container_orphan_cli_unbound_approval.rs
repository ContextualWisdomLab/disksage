use std::process::Command;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[cfg(unix)]
#[test]
fn docker_native_read_only_cli_does_not_publish_an_unusable_approval_phrase() {
    const FULL_ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let temp = tempfile::tempdir().expect("temporary runtime directory");
    let runtime = temp.path().join("docker");
    let script = format!(
        r#"#!/bin/sh
set -eu
case "${{1:-}}" in
  info) exit 0 ;;
  container)
    if [ "${{2:-}}" = "ps" ]; then
      printf '%s\n' '{{"ID":"{FULL_ID}","State":"exited","Names":[]}}'
      exit 0
    fi
    [ "${{2:-}}" = "inspect" ] || exit 91
    printf '%s\n' '[{{"Id":"{FULL_ID}","Mounts":[]}}]'
    ;;
  images) exit 0 ;;
  volume)
    [ "${{2:-}}" = "ls" ] || exit 92
    exit 0
    ;;
  network)
    [ "${{2:-}}" = "ls" ] || exit 93
    exit 0
    ;;
  *) exit 94 ;;
esac
"#
    );
    std::fs::write(&runtime, script).expect("write fake Docker runtime");
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
    let container = document["categories"]
        .as_array()
        .expect("category array")
        .iter()
        .find(|category| category["category"] == serde_json::json!("container"))
        .expect("container category");

    assert_eq!(container["evidence"]["candidate_records"], serde_json::json!(1));
    assert_eq!(container["approval_phrase"], serde_json::Value::Null);
}
