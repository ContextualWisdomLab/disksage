use disksage_lib::podman_desktop::redact_podman_reclaim_plan;
use disksage_lib::podman_reclaim::{
    GuestFilesystemEvidence, PodmanMachineEvidence, PodmanReclaimAssessment, PodmanReclaimPlan,
    PodmanRecommendedAction, PodmanRecommendedActionKind, PodmanStoreEvidence,
    PodmanSystemDfCategoryEvidence, PodmanSystemDfEvidence, PodmanUnusedImageEvidence,
    RawImageEvidence, PODMAN_RECLAIM_SCHEMA_KIND,
};

/// Build one deterministic `podman system df` category for projection tests.
fn category(reclaimable_bytes: u64) -> PodmanSystemDfCategoryEvidence {
    PodmanSystemDfCategoryEvidence {
        total: 2,
        active: 1,
        size_bytes: reclaimable_bytes.saturating_add(10),
        reclaimable_bytes,
    }
}

/// Build a complete plan whose private identifiers must never cross the desktop boundary.
fn complete_plan() -> PodmanReclaimPlan {
    PodmanReclaimPlan {
        schema_kind: PODMAN_RECLAIM_SCHEMA_KIND,
        schema_version: 3,
        platform: "macos",
        evidence_complete: true,
        elapsed_ms: 17,
        machine: Some(PodmanMachineEvidence {
            name: "private-machine".to_string(),
            state: "running".to_string(),
            configured_disk_bytes: Some(1_000),
        }),
        raw_image: Some(RawImageEvidence {
            path: "/Users/private/.local/share/private-machine.raw".to_string(),
            logical_bytes: 900,
            allocated_bytes: Some(700),
            identity_sha256: Some("d".repeat(64)),
        }),
        guest_filesystem: Some(GuestFilesystemEvidence {
            total_bytes: 800,
            used_bytes: 500,
            available_bytes: 300,
        }),
        store: Some(PodmanStoreEvidence {
            graph_root: "/var/home/private/containers".to_string(),
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
            candidate_set_sha256: "abcdef0123456789".repeat(4),
        }),
        dangling_prune_approval_phrase: None,
        assessment: PodmanReclaimAssessment {
            physically_reclaimable_bytes: None,
            podman_reported_reclaimable_bytes: Some(300),
            raw_allocated_minus_guest_used_bytes: Some(200),
            status: "unverified".to_string(),
            reason_codes: vec!["host-physical-reclaim-unverified".to_string()],
            recommended_actions: vec![],
        },
        issues: vec![],
    }
}

/// Exercise every character-class and length boundary of privacy-safe issue-code admission.
#[test]
fn issue_code_projection_covers_length_prefix_and_character_boundaries() {
    let mut plan = complete_plan();
    plan.issues = vec![
        "stable-code9:private-detail".to_string(),
        "stable--0".to_string(),
        "a".repeat(97),
        "1starts-with-digit".to_string(),
        "-starts-with-hyphen".to_string(),
        "with space".to_string(),
        "éclair".to_string(),
    ];

    let evidence = redact_podman_reclaim_plan(plan);

    assert!(evidence.issue_codes.contains(&"stable-code9".to_string()));
    assert!(evidence.issue_codes.contains(&"stable--0".to_string()));
    assert!(evidence
        .issue_codes
        .contains(&"podman-evidence-error".to_string()));
    assert_eq!(
        evidence
            .issue_codes
            .iter()
            .filter(|code| code.as_str() == "podman-evidence-error")
            .count(),
        1
    );
}

/// Reject lowercase non-hexadecimal fingerprints that otherwise satisfy the exact length bound.
#[test]
fn fingerprint_validation_rejects_lowercase_non_hex_at_exact_length() {
    let mut plan = complete_plan();
    plan.unused_images
        .as_mut()
        .expect("fixture has unused image evidence")
        .candidate_set_sha256 = "g".repeat(64);

    let evidence = redact_podman_reclaim_plan(plan);

    assert!(!evidence.evidence_complete);
    assert_eq!(evidence.candidates.image_candidate_set_sha256, None);
    assert!(evidence
        .issue_codes
        .contains(&"podman-desktop-invalid-candidate-fingerprint".to_string()));
}

/// Preserve fail-closed candidate review while distinguishing action-approval branches.
#[test]
fn observed_candidates_force_review_even_without_matching_approval() {
    let mut plan = complete_plan();
    plan.assessment.recommended_actions = vec![
        PodmanRecommendedAction {
            kind: PodmanRecommendedActionKind::ReviewUnusedImages,
            requires_human_approval: false,
            rationale: "image observation only".to_string(),
        },
        PodmanRecommendedAction {
            kind: PodmanRecommendedActionKind::InvestigateApi,
            requires_human_approval: true,
            rationale: "unrelated approval".to_string(),
        },
        PodmanRecommendedAction {
            kind: PodmanRecommendedActionKind::ReviewStoppedContainers,
            requires_human_approval: true,
            rationale: "container review".to_string(),
        },
        PodmanRecommendedAction {
            kind: PodmanRecommendedActionKind::ReviewUnusedVolumes,
            requires_human_approval: false,
            rationale: "volume observation only".to_string(),
        },
    ];

    let evidence = redact_podman_reclaim_plan(plan);

    // The stopped-container path is satisfied by a matching approved action. Image and volume
    // deliberately are not, but their non-zero observed candidates still force review. An
    // unrelated approved action cannot substitute for the object-domain boundary.
    assert!(evidence.review_boundaries.image_review_required);
    assert!(evidence.review_boundaries.stopped_container_review_required);
    assert!(evidence.review_boundaries.volume_review_required);
}

/// Preserve unknown inner optional measurements even when their enclosing observations exist.
#[test]
fn nested_optional_capacity_values_remain_unknown() {
    let mut plan = complete_plan();
    plan.machine
        .as_mut()
        .expect("fixture has machine evidence")
        .configured_disk_bytes = None;
    plan.raw_image
        .as_mut()
        .expect("fixture has raw-image evidence")
        .allocated_bytes = None;

    let evidence = redact_podman_reclaim_plan(plan);

    assert_eq!(evidence.capacity.configured_disk_bytes, None);
    assert_eq!(evidence.capacity.host_allocated_bytes, None);
    assert_eq!(evidence.capacity.raw_logical_bytes, Some(900));
}
