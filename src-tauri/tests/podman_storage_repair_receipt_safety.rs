#![cfg(unix)]

use disksage_lib::podman_reclaim::{
    execute_podman_storage_repair, plan_podman_storage_repair,
};
use std::fs;
use std::os::unix::fs::PermissionsExt;

fn fake_podman() -> (tempfile::TempDir, std::path::PathBuf) {
    let temp = tempfile::tempdir().expect("temporary runtime directory");
    let fake = temp.path().join("podman");
    let layer_id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let script = format!(
        r#"#!/bin/sh
root="$(dirname "$0")"
case " $* " in
  *" system check --quick --repair "*)
    echo "repair command did not provide a trustworthy result" >&2
    exit 1
    ;;
  *" system check --quick "*)
    count=0
    if [ -f "$root/check-count" ]; then
      count=$(cat "$root/check-count")
    fi
    count=$((count + 1))
    printf '%s' "$count" > "$root/check-count"
    if [ "$count" -le 2 ]; then
      echo "Damaged layer {layer_id}:"
      echo "Error: damage detected in local storage"
      exit 1
    fi
    echo "postcheck unavailable" >&2
    exit 1
    ;;
esac
echo "unexpected fake Podman invocation: $*" >&2
exit 2
"#
    );
    fs::write(&fake, script).expect("write fake Podman");
    let mut permissions = fs::metadata(&fake).unwrap().permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&fake, permissions).unwrap();
    (temp, fake)
}

#[test]
fn incomplete_postcheck_never_serializes_unverified_repair_counts() {
    let (_temp, fake) = fake_podman();
    let plan = plan_podman_storage_repair(&fake, "podman-machine-default")
        .expect("initial damaged-layer evidence");
    let approval = plan
        .exact_approval_phrase
        .as_deref()
        .expect("repair approval phrase");

    let receipt = execute_podman_storage_repair(
        &fake,
        "podman-machine-default",
        approval,
        "test incomplete postcheck",
        1,
    )
    .expect("the receipt must preserve that the command ran even when verification is incomplete");
    let json = serde_json::to_value(receipt).expect("serialize receipt");

    assert_eq!(json["postcheck_complete"], false);
    assert!(
        json["repaired_layer_records"].is_null(),
        "an incomplete postcheck cannot turn the precheck count into a verified repair count"
    );
    assert!(
        json["remaining_damaged_layer_records"].is_null(),
        "remaining damaged layers are unknown when the postcheck is incomplete"
    );
    assert_eq!(json["executed"], false);
}

#[test]
fn storage_repair_rejects_unbounded_or_control_character_rationales_before_probe() {
    let (_temp, fake) = fake_podman();
    let overlong = "x".repeat(1_001);
    assert_eq!(
        execute_podman_storage_repair(
            &fake,
            "podman-machine-default",
            "unused",
            &overlong,
            1,
        )
        .unwrap_err(),
        "podman-storage-repair-request-invalid"
    );
    assert_eq!(
        execute_podman_storage_repair(
            &fake,
            "podman-machine-default",
            "unused",
            "operator\ncontrol",
            1,
        )
        .unwrap_err(),
        "podman-storage-repair-request-invalid"
    );
    assert!(
        !tempfile::TempDir::path(&_temp).join("check-count").exists(),
        "invalid rationale must be rejected before invoking Podman"
    );
}
