#![cfg(unix)]

use disksage_lib::podman_reclaim::{
    execute_podman_storage_repair, plan_podman_storage_repair,
};
use std::fs;
use std::os::unix::fs::PermissionsExt;

#[test]
fn changed_damage_requires_fresh_podman_storage_approval() {
    let temp = tempfile::tempdir().expect("temporary runtime directory");
    let fake = temp.path().join("podman");
    let first = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let second = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let script = format!(
        r#"#!/bin/sh
root="$(dirname "$0")"
case " $* " in
  *" system check --quick --repair "*)
    touch "$root/repair-ran"
    exit 0
    ;;
  *" system check --quick "*)
    count=0
    [ ! -f "$root/check-count" ] || count=$(cat "$root/check-count")
    count=$((count + 1))
    printf '%s' "$count" > "$root/check-count"
    if [ "$count" -eq 1 ]; then
      echo "Damaged layer {first}:"
      echo "Error: damage detected in local storage"
      exit 1
    fi
    echo "Damaged layer {second}:"
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

    let plan = plan_podman_storage_repair(&fake, "podman-machine-default")
        .expect("initial damaged-layer evidence");
    let approval = plan
        .exact_approval_phrase
        .expect("repair approval phrase for initial evidence");

    let error = execute_podman_storage_repair(
        &fake,
        "podman-machine-default",
        &approval,
        "require fresh approval after damaged-layer drift",
        1,
    )
    .expect_err("changed damaged-layer evidence must invalidate the stale approval");

    assert_eq!(error, "podman-storage-repair-confirmation-mismatch");
    assert!(
        !temp.path().join("repair-ran").exists(),
        "no broad native repair may run under approval for a different damage set"
    );
}
