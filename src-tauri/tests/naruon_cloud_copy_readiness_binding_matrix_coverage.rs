//! Credential-free structural coverage for NarUon readiness binding invariants.
//!
//! The baseline starts from a real exported zero-candidate envelope and is promoted in memory to a
//! structurally coherent blocked candidate. Its stale fingerprint proves validation reached the
//! final integrity boundary. Focused mutations then exercise independent fail-closed bindings
//! without provider requests, credentials, cloud writes, or source-eviction authority.

use disksage_lib::cloud::{
    CloudAccountScope, CloudPlanOptions, CloudPlanReport, CloudProvider, CloudRoot,
    ExactDuplicateSummary,
};
use disksage_lib::naruon_cloud_copy_readiness::{
    export_naruon_cloud_copy_readiness, validate_naruon_cloud_copy_readiness,
    CloudCopyReadinessState, CountBytes, NaruonCloudCopyReadinessEnvelope,
};
use disksage_lib::provider_capacity::{
    assess_capacity, unavailable_capacity, DEFAULT_CAPACITY_RESERVE_BYTES,
};
use disksage_lib::provider_client_runtime::assess_provider_client_runtime;

fn valid_empty_envelope() -> NaruonCloudCopyReadinessEnvelope {
    let provider = CloudProvider::Onedrive;
    let capacity = assess_capacity(
        unavailable_capacity(provider, 10, "capacity-unavailable"),
        0,
        0,
        DEFAULT_CAPACITY_RESERVE_BYTES,
    );
    let report = CloudPlanReport {
        cloud_root: CloudRoot {
            id: "binding-matrix-coverage-root".into(),
            provider,
            account_scope: CloudAccountScope::Personal,
            label: "Binding matrix coverage root".into(),
            path: "/coverage/cloud".into(),
            readable: true,
            access_issue: None,
        },
        generated_at_ms: 20,
        source_selection_policy: Some(CloudPlanOptions {
            min_size_bytes: 1,
            min_age_days: 0,
            limit: 1,
        }),
        candidates: Vec::new(),
        candidate_bytes: 0,
        potentially_reclaimable_bytes: 0,
        exact_duplicates: ExactDuplicateSummary::default(),
        capacity: Some(capacity),
        notices: Vec::new(),
    };
    let runtime = assess_provider_client_runtime(
        provider,
        Some(b"OneDrive Sync Service\n"),
        25,
    );
    export_naruon_cloud_copy_readiness(&report, &runtime, None).unwrap()
}

fn blocked_candidate_envelope() -> NaruonCloudCopyReadinessEnvelope {
    let mut envelope = valid_empty_envelope();
    let snapshot = envelope.capacity.snapshot.clone();
    let reserve = envelope.capacity.reserve_bytes;
    envelope.capacity = assess_capacity(snapshot, 42, 42, reserve);
    envelope.candidate_count = 1;
    envelope.candidate_bytes = 42;
    envelope.potentially_reclaimable_bytes = 42;
    envelope.planner_unblocked = CountBytes {
        count: 1,
        bytes: 42,
    };
    envelope.requires_human_review = CountBytes::default();
    envelope.ready_without_new_review = CountBytes::default();
    envelope.production_time_evidence.embedded_metadata = CountBytes {
        count: 1,
        bytes: 42,
    };
    envelope.candidate_blocker_counts.clear();
    envelope.candidate_blocker_counts.insert(
        "capacity-unavailable".into(),
        CountBytes {
            count: 1,
            bytes: 42,
        },
    );
    envelope.candidate_blocker_counts.insert(
        "provider-global-sync-evidence-unavailable".into(),
        CountBytes {
            count: 1,
            bytes: 42,
        },
    );
    envelope.readiness_state = CloudCopyReadinessState::Blocked;
    envelope
}

#[test]
fn coherent_synthetic_candidate_reaches_only_the_final_fingerprint_boundary() {
    let envelope = blocked_candidate_envelope();
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&envelope).unwrap_err(),
        "naruon-copy-readiness-fingerprint-invalid"
    );
}

#[test]
fn review_planner_and_ready_bindings_fail_closed_independently() {
    let baseline = blocked_candidate_envelope();

    let mut missing_review_blocker = baseline.clone();
    missing_review_blocker.requires_human_review = CountBytes {
        count: 1,
        bytes: 42,
    };
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&missing_review_blocker).unwrap_err(),
        "naruon-copy-readiness-review-binding-invalid"
    );

    let mut spurious_review_blocker = baseline.clone();
    spurious_review_blocker.candidate_blocker_counts.insert(
        "review-required".into(),
        CountBytes {
            count: 1,
            bytes: 42,
        },
    );
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&spurious_review_blocker).unwrap_err(),
        "naruon-copy-readiness-review-binding-invalid"
    );

    let mut missing_planner_blocker = baseline.clone();
    missing_planner_blocker.planner_unblocked = CountBytes::default();
    missing_planner_blocker.potentially_reclaimable_bytes = 0;
    let snapshot = missing_planner_blocker.capacity.snapshot.clone();
    let reserve = missing_planner_blocker.capacity.reserve_bytes;
    missing_planner_blocker.capacity = assess_capacity(snapshot, 0, 42, reserve);
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&missing_planner_blocker).unwrap_err(),
        "naruon-copy-readiness-planner-binding-invalid"
    );

    let mut spurious_planner_blocker = baseline.clone();
    spurious_planner_blocker.candidate_blocker_counts.insert(
        "planner-blocked".into(),
        CountBytes {
            count: 1,
            bytes: 42,
        },
    );
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&spurious_planner_blocker).unwrap_err(),
        "naruon-copy-readiness-planner-binding-invalid"
    );

    let mut overlapping_review = baseline.clone();
    overlapping_review.requires_human_review = CountBytes {
        count: 1,
        bytes: 42,
    };
    overlapping_review.ready_without_new_review = CountBytes {
        count: 1,
        bytes: 42,
    };
    overlapping_review.candidate_blocker_counts.insert(
        "review-required".into(),
        CountBytes {
            count: 1,
            bytes: 42,
        },
    );
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&overlapping_review).unwrap_err(),
        "naruon-copy-readiness-review-overlap-invalid"
    );

    let mut forged_ready = baseline;
    forged_ready.ready_without_new_review = CountBytes {
        count: 1,
        bytes: 42,
    };
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&forged_ready).unwrap_err(),
        "naruon-copy-readiness-ready-gate-invalid"
    );
}

#[test]
fn runtime_capacity_and_global_sync_bindings_require_candidate_wide_blockers() {
    let baseline = blocked_candidate_envelope();

    let mut runtime = baseline.clone();
    runtime.provider_runtime = assess_provider_client_runtime(CloudProvider::Onedrive, None, 25);
    runtime.provider_runtime_prerequisite_met = false;
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&runtime).unwrap_err(),
        "naruon-copy-readiness-runtime-binding-invalid"
    );

    let mut capacity = baseline.clone();
    capacity
        .candidate_blocker_counts
        .remove("capacity-unavailable");
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&capacity).unwrap_err(),
        "naruon-copy-readiness-capacity-binding-invalid"
    );

    let mut global_sync = baseline.clone();
    global_sync
        .candidate_blocker_counts
        .remove("provider-global-sync-evidence-unavailable");
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&global_sync).unwrap_err(),
        "naruon-copy-readiness-provider-global-sync-binding-invalid"
    );

    let mut unexpected_global_sync = baseline;
    unexpected_global_sync.candidate_blocker_counts.insert(
        "provider-global-sync-unexpected".into(),
        CountBytes {
            count: 1,
            bytes: 42,
        },
    );
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&unexpected_global_sync).unwrap_err(),
        "naruon-copy-readiness-provider-global-sync-binding-invalid"
    );
}

#[test]
fn capacity_binding_rejects_scope_size_and_time_drift() {
    let baseline = blocked_candidate_envelope();

    let mut scope = baseline.clone();
    scope.capacity.snapshot.account_scope = Some(CloudAccountScope::Organization);
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&scope).unwrap_err(),
        "naruon-copy-readiness-capacity-binding-invalid"
    );

    let mut requested = baseline.clone();
    let snapshot = requested.capacity.snapshot.clone();
    let reserve = requested.capacity.reserve_bytes;
    requested.capacity = assess_capacity(snapshot, 0, 42, reserve);
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&requested).unwrap_err(),
        "naruon-copy-readiness-capacity-binding-invalid"
    );

    let mut largest = baseline.clone();
    let snapshot = largest.capacity.snapshot.clone();
    let reserve = largest.capacity.reserve_bytes;
    largest.capacity = assess_capacity(snapshot, 42, 43, reserve);
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&largest).unwrap_err(),
        "naruon-copy-readiness-capacity-binding-invalid"
    );

    let mut stale = baseline;
    stale.generated_at_ms = stale.provider_runtime.observed_at_ms.saturating_sub(1);
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&stale).unwrap_err(),
        "naruon-copy-readiness-capacity-binding-invalid"
    );
}

#[test]
fn aggregate_overflow_blocker_bounds_and_blocker_totals_fail_closed() {
    let baseline = blocked_candidate_envelope();

    let mut count_overflow = baseline.clone();
    count_overflow.production_time_evidence.embedded_metadata.count = u64::MAX;
    count_overflow
        .production_time_evidence
        .explicit_filename_date
        .count = 1;
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&count_overflow).unwrap_err(),
        "naruon-copy-readiness-count-overflow"
    );

    let mut bytes_overflow = baseline.clone();
    bytes_overflow.production_time_evidence.embedded_metadata.bytes = u64::MAX;
    bytes_overflow
        .production_time_evidence
        .explicit_filename_date
        .bytes = 1;
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&bytes_overflow).unwrap_err(),
        "naruon-copy-readiness-bytes-overflow"
    );

    let mut blocker_aggregate = baseline.clone();
    blocker_aggregate.candidate_blocker_counts.insert(
        "coverage-blocker".into(),
        CountBytes {
            count: 2,
            bytes: 42,
        },
    );
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&blocker_aggregate).unwrap_err(),
        "naruon-copy-readiness-blocker-aggregate-invalid"
    );

    let mut too_many_blockers = baseline;
    for index in 0..129 {
        too_many_blockers.candidate_blocker_counts.insert(
            format!("coverage-blocker-{index}"),
            CountBytes::default(),
        );
    }
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&too_many_blockers).unwrap_err(),
        "naruon-copy-readiness-bounds-invalid"
    );
}