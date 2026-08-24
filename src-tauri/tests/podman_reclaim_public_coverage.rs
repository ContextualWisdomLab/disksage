//! Public fail-closed coverage for the Podman reclaim probe.
//!
//! These tests deliberately avoid a real Podman installation, account, VM, or mutation. They
//! exercise public admission, process, parsing, and assessment boundaries with synthetic inputs.

use disksage_lib::podman_reclaim::{
    probe_podman_reclaim, PodmanRecommendedActionKind, DEFAULT_PODMAN_MACHINE,
    PODMAN_RECLAIM_SCHEMA_KIND,
};
#[cfg(unix)]
use serde_json::json;
#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::Duration;

#[test]
fn unsafe_machine_name_fails_closed_without_spawning_a_process() {
    let plan = probe_podman_reclaim(
        Path::new("this-executable-must-not-be-consulted"),
        "../escape",
        Duration::from_millis(1),
    );

    assert_eq!(plan.schema_kind, PODMAN_RECLAIM_SCHEMA_KIND);
    assert_eq!(plan.schema_version, 3);
    assert!(!plan.evidence_complete);
    assert!(plan.machine.is_none());
    assert!(plan.raw_image.is_none());
    assert!(plan.guest_filesystem.is_none());
    assert!(plan.store.is_none());
    assert!(plan.system_df.is_none());
    assert!(plan.unused_images.is_none());
    assert_eq!(plan.issues, vec!["unsafe-requested-machine-name"]);
    assert_eq!(plan.assessment.physically_reclaimable_bytes, None);
    assert!(plan
        .assessment
        .reason_codes
        .iter()
        .any(|code| code == "partial-evidence"));
}

#[test]
fn missing_podman_executable_is_observable_and_never_claims_complete_evidence() {
    let temp = tempfile::tempdir().unwrap();
    let missing = temp.path().join("definitely-not-podman");
    let plan = probe_podman_reclaim(
        &missing,
        DEFAULT_PODMAN_MACHINE,
        Duration::from_millis(25),
    );

    assert!(!plan.evidence_complete);
    assert!(plan.machine.is_none());
    assert_eq!(plan.assessment.podman_reported_reclaimable_bytes, None);
    assert_eq!(plan.assessment.raw_allocated_minus_guest_used_bytes, None);
    assert!(plan
        .issues
        .iter()
        .any(|issue| issue.starts_with("podman-machine-inspect-spawn:")));
    assert!(plan
        .assessment
        .reason_codes
        .iter()
        .any(|code| code == "host-physical-reclaim-unverified"));
    assert!(plan
        .assessment
        .reason_codes
        .iter()
        .any(|code| code == "partial-evidence"));
}

#[cfg(unix)]
#[test]
fn synthetic_read_only_probe_collects_complete_exact_evidence() {
    let temp = tempfile::tempdir().unwrap();
    let raw_image_path = temp.path().join("podman-machine.raw");
    fs::write(&raw_image_path, b"synthetic-read-only-raw-image").unwrap();
    fs::write(
        temp.path().join(format!("{DEFAULT_PODMAN_MACHINE}.json")),
        json!({
            "ImagePath": {
                "Path": raw_image_path.to_string_lossy().into_owned()
            }
        })
        .to_string(),
    )
    .unwrap();

    let inspect = json!([{
        "ConfigDir": { "Path": temp.path().to_string_lossy().into_owned() },
        "Name": DEFAULT_PODMAN_MACHINE,
        "State": "running",
        "Resources": { "DiskSize": 100 }
    }])
    .to_string();
    let info = json!({
        "store": {
            "graphRoot": "/var/home/core/.local/share/containers/storage",
            "graphRootAllocated": 4096,
            "graphRootUsed": 2048,
            "imageStore": { "number": 2 },
            "containerStore": { "number": 1, "running": 0, "stopped": 1 }
        }
    })
    .to_string();
    let system_df = json!([
        { "Type": "Images", "Total": 2, "Active": 1, "RawSize": 300, "RawReclaimable": 200 },
        { "Type": "Containers", "Total": 1, "Active": 0, "RawSize": 50, "RawReclaimable": 50 },
        { "Type": "Local Volumes", "Total": 1, "Active": 0, "RawSize": 100, "RawReclaimable": 100 }
    ])
    .to_string();
    let images = json!([
        {
            "Id": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "RepoTags": ["used:latest"],
            "Containers": 1,
            "Size": 100
        },
        {
            "Id": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "RepoTags": ["unused:latest"],
            "Containers": 0,
            "Size": 200
        }
    ])
    .to_string();

    let fake_podman = temp.path().join("podman");
    fs::write(
        &fake_podman,
        format!(
            r#"#!/bin/sh
case "$*" in
  "machine inspect podman-machine-default")
    cat <<'DISKSAGE_INSPECT_JSON'
{inspect}
DISKSAGE_INSPECT_JSON
    ;;
  "machine ssh podman-machine-default -- df -B1 --output=size,used,avail /")
    printf '%s\n' '1B-blocks Used Avail' '10737418240 1073741824 8589934592'
    ;;
  "--connection podman-machine-default info --format json")
    cat <<'DISKSAGE_INFO_JSON'
{info}
DISKSAGE_INFO_JSON
    ;;
  "--connection podman-machine-default system df --format json")
    cat <<'DISKSAGE_SYSTEM_DF_JSON'
{system_df}
DISKSAGE_SYSTEM_DF_JSON
    ;;
  "--connection podman-machine-default images --all --format json")
    cat <<'DISKSAGE_IMAGES_JSON'
{images}
DISKSAGE_IMAGES_JSON
    ;;
  *)
    printf '%s\n' "unexpected arguments: $*" >&2
    exit 91
    ;;
esac
"#
        ),
    )
    .unwrap();
    fs::set_permissions(&fake_podman, fs::Permissions::from_mode(0o700)).unwrap();

    let plan = probe_podman_reclaim(
        &fake_podman,
        DEFAULT_PODMAN_MACHINE,
        Duration::from_secs(2),
    );

    assert!(plan.evidence_complete, "unexpected issues: {:?}", plan.issues);
    assert!(plan.issues.is_empty());
    assert_eq!(
        plan.machine.as_ref().map(|machine| machine.state.as_str()),
        Some("running")
    );
    assert_eq!(
        plan.machine
            .as_ref()
            .and_then(|machine| machine.configured_disk_bytes),
        Some(107_374_182_400)
    );
    assert_eq!(
        plan.guest_filesystem
            .as_ref()
            .map(|guest| guest.available_bytes),
        Some(8_589_934_592)
    );
    assert_eq!(
        plan.store.as_ref().map(|store| store.containers_stopped),
        Some(1)
    );
    assert_eq!(
        plan.unused_images
            .as_ref()
            .map(|images| images.unused_records),
        Some(1)
    );
    assert_eq!(
        plan.unused_images
            .as_ref()
            .map(|images| images.candidate_set_sha256.len()),
        Some(64)
    );
    assert_eq!(plan.assessment.podman_reported_reclaimable_bytes, Some(350));
    assert!(!plan
        .assessment
        .reason_codes
        .iter()
        .any(|code| code == "partial-evidence"));
    assert!(plan
        .assessment
        .recommended_actions
        .iter()
        .any(|action| action.kind == PodmanRecommendedActionKind::ReviewStoppedContainers));
    assert!(plan
        .assessment
        .recommended_actions
        .iter()
        .any(|action| action.kind == PodmanRecommendedActionKind::ReviewUnusedImages));
    assert!(plan
        .assessment
        .recommended_actions
        .iter()
        .any(|action| action.kind == PodmanRecommendedActionKind::ReviewUnusedVolumes));
}
