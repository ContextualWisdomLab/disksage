//! Review regressions for the privacy-safe Podman desktop boundary.
//!
//! These tests exercise two fail-closed contracts discovered during exact-head review: assessment
//! text may not cross IPC as unbounded local detail, and the registered Tauri command may not
//! disappear from a `coverage` configuration while `lib.rs` still references it.

use disksage_lib::podman_desktop::redact_podman_reclaim_plan;
use disksage_lib::podman_reclaim::{
    PodmanHostCompactionPlan, PodmanReclaimAssessment, PodmanReclaimPlan,
    PODMAN_RECLAIM_SCHEMA_KIND,
};

/// Build the smallest public plan that can carry hostile assessment text into projection.
fn plan_with_assessment(status: &str, reason_codes: &[&str]) -> PodmanReclaimPlan {
    PodmanReclaimPlan {
        schema_kind: PODMAN_RECLAIM_SCHEMA_KIND,
        schema_version: 3,
        platform: "macos",
        evidence_complete: true,
        elapsed_ms: 1,
        machine: None,
        raw_image: None,
        guest_filesystem: None,
        store: None,
        system_df: None,
        unused_images: None,
        dangling_prune_approval_phrase: None,
        host_compaction: PodmanHostCompactionPlan {
            supported: false,
            machine_identity_sha256: None,
            backing_file_identity_sha256: None,
            backing_file_freshness_sha256: None,
            active_container_count: None,
            stop_command: None,
            compaction_command: None,
            exact_approval_phrase: None,
            rollback_policy: "require-runtime-native-rollback-before-execution",
            restart_policy: "restore-observed-running-state-after-success-or-rollback",
            blockers: vec!["active-container-count-must-be-zero".to_string()],
            execution_performed: false,
        },
        assessment: PodmanReclaimAssessment {
            physically_reclaimable_bytes: None,
            podman_reported_reclaimable_bytes: None,
            raw_allocated_minus_guest_used_bytes: None,
            status: status.to_string(),
            reason_codes: reason_codes
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            recommended_actions: vec![],
        },
        issues: vec![],
    }
}

/// Host paths, socket-like text, and duplicate detail never survive assessment projection.
#[test]
fn hostile_assessment_text_is_redacted_and_fails_completeness_closed() {
    let evidence = redact_podman_reclaim_plan(plan_with_assessment(
        "/Users/alice/private-machine.sock",
        &[
            "host-physical-reclaim-unverified:/Users/alice/private-machine.sock",
            "/run/user/501/podman.sock",
            "host-physical-reclaim-unverified:duplicate-private-detail",
        ],
    ));

    assert_eq!(evidence.assessment_status, "unverified");
    assert_eq!(
        evidence.reason_codes,
        vec![
            "host-physical-reclaim-unverified".to_string(),
            "podman-assessment-error".to_string(),
        ]
    );
    assert!(!evidence.evidence_complete);
    assert!(evidence
        .issue_codes
        .contains(&"podman-desktop-invalid-assessment-code".to_string()));

    let json = serde_json::to_string(&evidence).expect("desktop evidence must serialize");
    assert!(!json.contains("alice"));
    assert!(!json.contains("private-machine"));
    assert!(!json.contains("/Users/"));
    assert!(!json.contains("/run/user/"));
}

/// The public command definition and Tauri registration must remain cfg-compatible.
#[test]
fn registered_command_is_not_removed_only_from_coverage_builds() {
    let command_source = include_str!("../src/podman_desktop.rs").replace("\r\n", "\n");
    let library_source = include_str!("../src/lib.rs").replace("\r\n", "\n");

    assert!(library_source.contains("podman_desktop_bridge::inspect_podman_desktop_evidence",));
    assert!(!command_source
        .contains("#[cfg(not(coverage))]\n#[tauri::command]\npub fn inspect_podman_reclaim",));
}
