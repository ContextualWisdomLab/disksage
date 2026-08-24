use disksage_lib::podman_reclaim::{probe_podman_reclaim, PODMAN_RECLAIM_SCHEMA_KIND};
use std::path::Path;
use std::time::Duration;

#[test]
fn unsafe_machine_names_fail_closed_without_starting_podman() {
    for machine in ["", ".", "..", "-option", "../escape", "bad/name", "name with space"] {
        let plan = probe_podman_reclaim(
            Path::new("/definitely-not-an-executable"),
            machine,
            Duration::from_millis(1),
        );

        assert_eq!(plan.schema_kind, PODMAN_RECLAIM_SCHEMA_KIND);
        assert_eq!(plan.schema_version, 3);
        assert!(!plan.evidence_complete);
        assert_eq!(plan.machine, None);
        assert_eq!(plan.raw_image, None);
        assert_eq!(plan.guest_filesystem, None);
        assert_eq!(plan.store, None);
        assert_eq!(plan.system_df, None);
        assert_eq!(plan.unused_images, None);
        assert_eq!(plan.assessment.physically_reclaimable_bytes, None);
        assert!(plan
            .assessment
            .reason_codes
            .contains(&"partial-evidence".to_string()));
        assert_eq!(plan.issues, vec!["unsafe-requested-machine-name"]);
    }
}

#[test]
fn machine_name_length_limit_fails_closed_before_process_spawn() {
    let too_long = "a".repeat(129);
    let plan = probe_podman_reclaim(
        Path::new("/definitely-not-an-executable"),
        &too_long,
        Duration::from_millis(1),
    );

    assert_eq!(plan.issues, vec!["unsafe-requested-machine-name"]);
    assert!(!plan.evidence_complete);
}
