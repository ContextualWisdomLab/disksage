//! Credential-free failure-matrix coverage for the Podman reclaim probe.
//!
//! The fixtures exercise exact public probe behavior through a synthetic executable. No real
//! Podman machine, socket, image, container, volume, or mutation is consulted.

#[cfg(unix)]
use disksage_lib::podman_reclaim::{probe_podman_reclaim, DEFAULT_PODMAN_MACHINE};
#[cfg(unix)]
use serde_json::json;
#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::time::Duration;

#[cfg(unix)]
fn write_executable(path: &std::path::Path, body: &str) {
    fs::write(path, body).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

#[cfg(unix)]
#[test]
fn probe_accumulates_independent_parser_failures_without_claiming_evidence() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(
        temp.path().join(format!("{DEFAULT_PODMAN_MACHINE}.json")),
        json!({ "ImagePath": { "Path": "relative-machine.raw" } }).to_string(),
    )
    .unwrap();

    let inspect = json!([{
        "ConfigDir": { "Path": temp.path().to_string_lossy().into_owned() },
        "Name": DEFAULT_PODMAN_MACHINE,
        "State": "running",
        "Resources": { "DiskSize": u64::MAX }
    }])
    .to_string();
    let uppercase_id = "A".repeat(64);
    let fake_podman = temp.path().join("podman");
    write_executable(
        &fake_podman,
        &format!(
            r#"#!/bin/sh
case "$*" in
  "machine inspect podman-machine-default")
    cat <<'DISKSAGE_INSPECT_JSON'
{inspect}
DISKSAGE_INSPECT_JSON
    ;;
  "machine ssh podman-machine-default -- df -B1 --output=size,used,avail /")
    printf '%s\n' '1B-blocks Used Avail' '10 9 9'
    ;;
  "--connection podman-machine-default info --format json")
    printf '%s\n' '{{"store":{{"graphRoot":"/var/lib/containers"}}}}'
    ;;
  "--connection podman-machine-default system df --format json")
    printf '%s\n' '[{{"Type":"Images","Total":1,"Active":2,"RawSize":1,"RawReclaimable":0}}]'
    ;;
  "--connection podman-machine-default images --all --format json")
    printf '%s\n' '[{{"Id":"{uppercase_id}","RepoTags":[],"Containers":0,"Size":1}}]'
    ;;
  *)
    printf '%s\n' "unexpected arguments: $*" >&2
    exit 91
    ;;
esac
"#
        ),
    );

    let plan = probe_podman_reclaim(
        &fake_podman,
        DEFAULT_PODMAN_MACHINE,
        Duration::from_secs(2),
    );

    assert!(!plan.evidence_complete);
    assert_eq!(
        plan.machine
            .as_ref()
            .and_then(|machine| machine.configured_disk_bytes),
        None
    );
    assert!(plan.raw_image.is_none());
    assert!(plan.guest_filesystem.is_none());
    assert!(plan.store.is_none());
    assert!(plan.system_df.is_none());
    assert!(plan.unused_images.is_none());
    assert_eq!(
        plan.issues,
        vec![
            "raw-image-path-not-absolute",
            "guest-df-inconsistent",
            "podman-info-field-missing:store.graphRootAllocated",
            "podman-system-df-inconsistent",
            "podman-images-invalid-id",
        ]
    );
    assert!(plan
        .assessment
        .reason_codes
        .iter()
        .any(|code| code == "podman-api-evidence-missing"));
    assert!(plan
        .assessment
        .reason_codes
        .iter()
        .any(|code| code == "partial-evidence"));
}

#[cfg(unix)]
#[test]
fn failed_probe_flattens_stderr_and_keeps_provider_detail_bounded() {
    let temp = tempfile::tempdir().unwrap();
    let fake_podman = temp.path().join("podman");
    write_executable(
        &fake_podman,
        r#"#!/bin/sh
printf '%s\n' 'first failure line' 'second failure line' >&2
exit 7
"#,
    );

    let plan = probe_podman_reclaim(
        &fake_podman,
        DEFAULT_PODMAN_MACHINE,
        Duration::from_secs(2),
    );

    assert!(!plan.evidence_complete);
    assert_eq!(plan.issues.len(), 1);
    assert!(plan.issues[0].starts_with("podman-machine-inspect-failed:"));
    assert!(plan.issues[0].contains("first failure line second failure line"));
    assert!(!plan.issues[0].contains('\n'));
    assert!(plan.issues[0].len() <= "podman-machine-inspect-failed:".len() + 512);
}
