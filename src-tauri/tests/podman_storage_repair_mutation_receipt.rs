#![cfg(unix)]

use disksage_lib::podman_reclaim::{
    execute_podman_storage_repair, plan_podman_storage_repair,
};
use std::fs;
use std::os::unix::fs::PermissionsExt;

#[test]
fn post_spawn_repair_capture_failure_still_returns_an_auditable_receipt() {
    let temp = tempfile::tempdir().expect("temporary runtime directory");
    let fake = temp.path().join("podman");
    let layer_id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let script = format!(
        r#"#!/bin/sh
root="$(dirname "$0")"
case " $* " in
  *" system check --quick --repair "*)
    touch "$root/repair-ran"
    head -c 1048577 /dev/zero | tr '\000' x
    exit 0
    ;;
  *" system check --quick "*)
    echo "Damaged layer {layer_id}:"
    echo "Error: damage detected in local storage"
    exit 1
    ;;
esac
exit 2
"#
    );
    fs::write(&fake, script).expect("write fake Podman");
    let mut permissions = fs::metadata(&fake).unwrap().permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&fake, permissions).unwrap();

    let approval = plan_podman_storage_repair(&fake, "podman-machine-default")
        .expect("initial damaged-layer evidence")
        .exact_approval_phrase
        .expect("repair approval phrase");

    let receipt = execute_podman_storage_repair(
        &fake,
        "podman-machine-default",
        &approval,
        "retain evidence after a spawned repair loses bounded output capture",
        1,
    )
    .expect("once the mutating command has spawned, capture failure must not erase its receipt");

    assert!(temp.path().join("repair-ran").exists());
    assert!(receipt.status_code < 0, "post-spawn capture failure needs an explicit non-success sentinel");
    assert!(!receipt.executed, "capture failure cannot be promoted to verified success");
    assert!(receipt.postcheck_complete, "a fresh postcheck should still be attempted after capture failure");
    assert_eq!(receipt.repaired_layer_records, Some(0));
    assert_eq!(receipt.remaining_damaged_layer_records, Some(1));
}
