//! Fail-closed review-boundary regressions for observed Podman candidates.
//!
//! Review booleans are decision-support evidence, not mutation authority. They still must not be
//! false when the same projected payload contains a non-zero candidate in that object domain,
//! even if an upstream assessment accidentally omits its recommended-action record.

use disksage_lib::podman_desktop::redact_podman_reclaim_plan;
use disksage_lib::podman_reclaim::{
    PodmanHostCompactionPlan, PodmanReclaimAssessment, PodmanReclaimPlan, PodmanStoreEvidence,
    PodmanSystemDfCategoryEvidence, PodmanSystemDfEvidence, PodmanUnusedImageEvidence,
    PODMAN_RECLAIM_SCHEMA_KIND,
};

/// Build one deterministic `podman system df` category observation.
fn category(reclaimable_bytes: u64) -> PodmanSystemDfCategoryEvidence {
    PodmanSystemDfCategoryEvidence {
        total: 2,
        active: 1,
        size_bytes: reclaimable_bytes.saturating_add(10),
        reclaimable_bytes,
    }
}

/// Build a plan with candidates but deliberately omit every recommended action.
fn candidate_plan_without_actions() -> PodmanReclaimPlan {
    PodmanReclaimPlan {
        schema_kind: PODMAN_RECLAIM_SCHEMA_KIND,
        schema_version: 3,
        platform: "macos",
        evidence_complete: true,
        elapsed_ms: 1,
        machine: None,
        raw_image: None,
        guest_filesystem: None,
        store: Some(PodmanStoreEvidence {
            graph_root: "/private/graph-root".to_string(),
            graph_root_allocated_bytes: 600,
            graph_root_used_bytes: 450,
            images: 4,
            containers_total: 3,
            containers_running: 1,
            containers_stopped: 2,
        }),
        system_df: Some(PodmanSystemDfEvidence {
            images: category(200),
            containers: category(30),
            local_volumes: category(70),
        }),
        unused_images: Some(PodmanUnusedImageEvidence {
            total_records: 4,
            referenced_records: 2,
            unused_records: 2,
            unused_untagged_records: 1,
            unused_tagged_records: 1,
            candidate_record_size_sum: 200,
            candidate_set_sha256: "a".repeat(64),
        }),
        dangling_prune_approval_phrase: None,
        host_compaction: PodmanHostCompactionPlan {
            supported: false,
            machine_identity_sha256: None,
            backing_file_identity_sha256: None,
            backing_file_freshness_sha256: None,
            active_container_count: Some(1),
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
            podman_reported_reclaimable_bytes: Some(300),
            raw_allocated_minus_guest_used_bytes: None,
            status: "unverified".to_string(),
            reason_codes: vec!["host-physical-reclaim-unverified".to_string()],
            recommended_actions: vec![],
        },
        issues: vec![],
    }
}

/// Candidate observations themselves conservatively require review in their own domain.
#[test]
fn observed_candidates_force_independent_review_boundaries() {
    let evidence = redact_podman_reclaim_plan(candidate_plan_without_actions());

    assert!(evidence.review_boundaries.image_review_required);
    assert!(evidence.review_boundaries.stopped_container_review_required);
    assert!(evidence.review_boundaries.volume_review_required);
}
