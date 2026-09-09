//! Fail-closed regression for contradictory Podman physical-reclaim evidence.
//!
//! A headless plan with an `unverified` assessment may not publish a concrete host-physical
//! reclaim amount to the desktop. The Rust projection must clear the claim and mark the evidence
//! incomplete before the untrusted IPC boundary, rather than relying on frontend rejection.

use disksage_lib::podman_desktop::redact_podman_reclaim_plan;
use disksage_lib::podman_reclaim::{
    PodmanReclaimAssessment, PodmanReclaimPlan, PODMAN_RECLAIM_SCHEMA_KIND,
};

/// Build the smallest contradictory plan that carries an unverified physical-reclaim claim.
fn contradictory_plan() -> PodmanReclaimPlan {
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
        assessment: PodmanReclaimAssessment {
            physically_reclaimable_bytes: Some(4096),
            podman_reported_reclaimable_bytes: None,
            raw_allocated_minus_guest_used_bytes: None,
            status: "unverified".to_string(),
            reason_codes: vec!["host-physical-reclaim-unverified".to_string()],
            recommended_actions: vec![],
        },
        issues: vec![],
    }
}

/// Contradictory physical-reclaim claims are removed and make the projection incomplete.
#[test]
fn unverified_physical_reclaim_claim_fails_closed_in_rust_projection() {
    let evidence = redact_podman_reclaim_plan(contradictory_plan());

    assert_eq!(evidence.assessment_status, "unverified");
    assert_eq!(evidence.physically_reclaimable_bytes, None);
    assert!(!evidence.evidence_complete);
    assert!(evidence
        .issue_codes
        .contains(&"podman-desktop-unverified-physical-reclaim-claim".to_string()));
}
