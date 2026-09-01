#![cfg(unix)]

use disksage_lib::podman_reclaim::{plan_empty_dangling_volumes, prune_empty_dangling_volumes};
use std::fs;
use std::os::unix::fs::PermissionsExt;

#[test]
fn empty_volume_cleanup_refuses_non_atomic_check_then_remove_authority() {
    let temp = tempfile::tempdir().expect("temporary test directory should be creatable");
    let fake_podman = temp.path().join("podman");
    let removal_marker = temp.path().join("volume-rm-invoked");
    let script = format!(
        r#"#!/bin/sh
case "$*" in
  *"volume ls"*)
    printf '%s\n' '[{{"Name":"customer_data","Mountpoint":"/var/home/core/.local/share/containers/storage/volumes/customer_data/_data","MountCount":0}}]'
    exit 0
    ;;
  *"machine ssh"*)
    # The approved volume looks empty during both pre-mutation probes.
    exit 0
    ;;
  *"volume rm"*)
    printf invoked > '{}'
    exit 0
    ;;
esac
exit 1
"#,
        removal_marker.display()
    );
    fs::write(&fake_podman, script).expect("fake Podman should be writable");
    let mut permissions = fs::metadata(&fake_podman)
        .expect("fake Podman metadata should be readable")
        .permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&fake_podman, permissions).expect("fake Podman should be executable");

    let plan = plan_empty_dangling_volumes(&fake_podman, "contract-machine")
        .expect("read-only empty-volume evidence should remain available");
    assert_eq!(plan.candidate_count, 1);
    let phrase = plan
        .exact_approval_phrase
        .as_deref()
        .expect("current implementation exposes destructive approval before the repair");

    let error = prune_empty_dangling_volumes(
        &fake_podman,
        "contract-machine",
        phrase,
        "reviewed reclaim",
    )
    .expect_err("check-then-remove volume deletion must fail closed until mutation is atomic");

    assert_eq!(error, "podman-empty-volume-atomic-removal-unavailable");
    assert!(
        !removal_marker.exists(),
        "DiskSage must not issue volume rm after a non-atomic emptiness recheck"
    );
}
