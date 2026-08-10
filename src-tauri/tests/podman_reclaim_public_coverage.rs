//! Public fail-closed coverage for the Podman reclaim probe.
//!
//! These tests deliberately avoid a real Podman installation, account, VM, or mutation. They
//! exercise only public admission and process-spawn failure boundaries with synthetic inputs.

use disksage_lib::podman_reclaim::{
    probe_podman_reclaim, DEFAULT_PODMAN_MACHINE, PODMAN_RECLAIM_SCHEMA_KIND,
};
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
