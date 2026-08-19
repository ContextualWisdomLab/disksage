//! Public-boundary coverage for Naruon cloud-copy readiness envelope integrity.
//!
//! These tests mutate only path-free, credential-free exported evidence and prove that aggregate,
//! policy, binding, and fingerprint drift fail closed before any cloud-write or eviction authority
//! can be inferred.

use std::collections::BTreeMap;

use disksage_lib::cloud::{
    CloudAccountScope, CloudPlanOptions, CloudPlanReport, CloudProvider, CloudRoot,
    ExactDuplicateSummary,
};
use disksage_lib::icloud_sync_health::{
    IcloudSyncHealthReport, IcloudUploadQueueSummary, ManagedDatabaseFileEvidence,
    ICLOUD_SYNC_HEALTH_SCHEMA_VERSION,
};
use disksage_lib::naruon_cloud_copy_readiness::{
    export_naruon_cloud_copy_readiness, validate_naruon_cloud_copy_readiness,
    CloudCopyReadinessState, CountBytes, NaruonCloudCopyReadinessEnvelope,
};
use disksage_lib::provider_capacity::{
    assess_capacity, unavailable_capacity, DEFAULT_CAPACITY_RESERVE_BYTES,
};
use disksage_lib::provider_client_runtime::assess_provider_client_runtime;

fn report() -> CloudPlanReport {
    let provider = CloudProvider::Icloud;
    CloudPlanReport {
        cloud_root: CloudRoot {
            id: "icloud-envelope-integrity-root".into(),
            provider,
            account_scope: CloudAccountScope::Personal,
            label: "iCloud envelope integrity root".into(),
            path: "/coverage/icloud".into(),
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
        capacity: Some(assess_capacity(
            unavailable_capacity(provider, 10, "capacity-unavailable"),
            0,
            0,
            DEFAULT_CAPACITY_RESERVE_BYTES,
        )),
        notices: Vec::new(),
    }
}

fn fully_blocked_health() -> IcloudSyncHealthReport {
    let blockers = vec![
        "icloud-upload-queue-nonempty".into(),
        "icloud-upload-in-flight".into(),
        "icloud-upload-blocked-on-sync-up".into(),
        "icloud-upload-out-of-quota".into(),
        "icloud-upload-queue-state-unclassified".into(),
        "icloud-local-sync-item-error-present".into(),
    ];
    IcloudSyncHealthReport {
        schema_version: ICLOUD_SYNC_HEALTH_SCHEMA_VERSION,
        output_mode: "icloud-local-sync-health".into(),
        observed_at_ms: 30,
        provider: "icloud".into(),
        evidence_kind: "supplementary-local-cloud-docs-private-schema".into(),
        evidence_complete: true,
        database_snapshot_includes_wal: true,
        database_sidecar_write_permitted: false,
        managed_database_files: vec![ManagedDatabaseFileEvidence {
            role: "client.db".into(),
            present: true,
            logical_bytes: 1,
            allocated_bytes: 1,
            modified_ms: Some(1),
        }],
        managed_database_allocated_bytes: 1,
        upload_queue: IcloudUploadQueueSummary {
            scheduled_waiting_count: 1,
            scheduled_waiting_bytes: 10,
            scheduled_active_count: 1,
            scheduled_active_bytes: 20,
            scheduled_count: 2,
            scheduled_bytes: 30,
            blocked_on_sync_up_count: 1,
            out_of_quota_count: 1,
            out_of_quota_bytes: 10,
            other_state_count: 1,
            item_error_count: 2,
            item_error_octagon_not_signed_in_count: 1,
            item_error_unclassified_count: 1,
            newest_item_error_timestamp_ms: Some(25),
            ..IcloudUploadQueueSummary::default()
        },
        sync_backlog_present: true,
        new_copy_admission_state: "blocked".into(),
        new_copy_admission_blockers: blockers.clone(),
        blockers,
        notices: Vec::new(),
        paths_redacted: true,
        user_filenames_read: false,
        user_file_contents_read: false,
        remote_capacity_verified: false,
        provider_sync_attested: false,
        local_eviction_authorized: false,
        mutation_performed: false,
    }
}

fn baseline() -> NaruonCloudCopyReadinessEnvelope {
    let runtime = assess_provider_client_runtime(CloudProvider::Icloud, None, 25);
    let envelope =
        export_naruon_cloud_copy_readiness(&report(), &runtime, Some(&fully_blocked_health()))
            .expect("credential-free blocked evidence must export");
    validate_naruon_cloud_copy_readiness(&envelope)
        .expect("the exporter must produce a valid baseline envelope");
    envelope
}

fn one_candidate_shape(envelope: &mut NaruonCloudCopyReadinessEnvelope) {
    envelope.candidate_count = 1;
    envelope.candidate_bytes = 1;
    envelope.production_time_evidence.unclassified = CountBytes { count: 1, bytes: 1 };
}

#[test]
fn schema_policy_and_selection_claim_drift_fail_closed() {
    let baseline = baseline();

    let mut schema_variants = Vec::new();
    let mut value = baseline.clone();
    value.schema_kind = "wrong.schema".into();
    schema_variants.push(value);
    let mut value = baseline.clone();
    value.schema_version = value.schema_version.saturating_add(1);
    schema_variants.push(value);
    let mut value = baseline.clone();
    value.decision_batch_fingerprint_version =
        value.decision_batch_fingerprint_version.saturating_add(1);
    schema_variants.push(value);
    let mut value = baseline.clone();
    value.decision_batch_fingerprint = "A".repeat(64);
    schema_variants.push(value);
    let mut value = baseline.clone();
    value.readiness_fingerprint_canonicalization = "wrong-canonicalization".into();
    schema_variants.push(value);
    for envelope in schema_variants {
        assert_eq!(
            validate_naruon_cloud_copy_readiness(&envelope).unwrap_err(),
            "naruon-copy-readiness-schema-invalid"
        );
    }

    let mut policy_variants = Vec::new();
    let mut value = baseline.clone();
    value.local_paths_included = true;
    policy_variants.push(value);
    let mut value = baseline.clone();
    value.relative_names_included = true;
    policy_variants.push(value);
    let mut value = baseline.clone();
    value.raw_metadata_values_included = true;
    policy_variants.push(value);
    let mut value = baseline.clone();
    value.account_identifiers_included = true;
    policy_variants.push(value);
    let mut value = baseline.clone();
    value.provider_sync_attested = true;
    policy_variants.push(value);
    let mut value = baseline.clone();
    value.cloud_write_executed = true;
    policy_variants.push(value);
    let mut value = baseline.clone();
    value.source_eviction_authorized = true;
    policy_variants.push(value);
    let mut value = baseline.clone();
    value.human_review_decisions_applied = true;
    policy_variants.push(value);
    let mut value = baseline.clone();
    value.filename_dates_are_auxiliary = false;
    policy_variants.push(value);
    let mut value = baseline.clone();
    value.metadata_policy.pop();
    policy_variants.push(value);
    for envelope in policy_variants {
        assert_eq!(
            validate_naruon_cloud_copy_readiness(&envelope).unwrap_err(),
            "naruon-copy-readiness-policy-claim-invalid"
        );
    }

    let mut selection_variants = Vec::new();
    let mut value = baseline.clone();
    value.source_selection_policy.min_size_bytes = 0;
    selection_variants.push(value);
    let mut value = baseline.clone();
    value.source_selection_policy.limit = 0;
    selection_variants.push(value);
    let mut value = baseline;
    value.source_selection_policy.limit = 1_001;
    selection_variants.push(value);
    for envelope in selection_variants {
        assert_eq!(
            validate_naruon_cloud_copy_readiness(&envelope).unwrap_err(),
            "naruon-copy-readiness-selection-policy-invalid"
        );
    }
}

#[test]
fn provider_capacity_and_icloud_binding_drift_fail_closed() {
    let baseline = baseline();

    let mut value = baseline.clone();
    value.provider_runtime_prerequisite_met = !value.provider_runtime_prerequisite_met;
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&value).unwrap_err(),
        "naruon-copy-readiness-provider-binding-invalid"
    );

    let mut value = baseline.clone();
    value.remote_capacity_verified = !value.remote_capacity_verified;
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&value).unwrap_err(),
        "naruon-copy-readiness-provider-binding-invalid"
    );

    let mut value = baseline.clone();
    value.potentially_reclaimable_bytes = 1;
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&value).unwrap_err(),
        "naruon-copy-readiness-capacity-binding-invalid"
    );

    let mut value = baseline.clone();
    value.generated_at_ms = 0;
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&value).unwrap_err(),
        "naruon-copy-readiness-capacity-binding-invalid"
    );

    let mut value = baseline.clone();
    value.icloud_new_copy_admission_met = Some(true);
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&value).unwrap_err(),
        "naruon-copy-readiness-icloud-binding-invalid"
    );

    let mut value = baseline;
    let generated_at_ms = value.generated_at_ms;
    let admission = value
        .icloud_new_copy_admission
        .as_mut()
        .expect("iCloud admission must exist");
    admission.observed_at_ms = generated_at_ms.saturating_add(1);
    admission.newest_item_error_age_ms = admission
        .newest_item_error_timestamp_ms
        .and_then(|timestamp| admission.observed_at_ms.checked_sub(timestamp));
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&value).unwrap_err(),
        "naruon-copy-readiness-icloud-binding-invalid"
    );
}

#[test]
fn aggregate_overflow_and_bounds_drift_fail_closed() {
    let baseline = baseline();

    let mut value = baseline.clone();
    value
        .candidate_blocker_counts
        .insert("Invalid_Blocker".into(), CountBytes::default());
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&value).unwrap_err(),
        "naruon-copy-readiness-blocker-invalid"
    );

    let mut value = baseline.clone();
    value.candidate_count = 1_001;
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&value).unwrap_err(),
        "naruon-copy-readiness-bounds-invalid"
    );

    let mut value = baseline.clone();
    value.candidate_blocker_counts = (0..129)
        .map(|index| (format!("bounded-blocker-{index}"), CountBytes::default()))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&value).unwrap_err(),
        "naruon-copy-readiness-bounds-invalid"
    );

    let mut value = baseline.clone();
    value.production_time_evidence.embedded_metadata.count = u64::MAX;
    value.production_time_evidence.explicit_filename_date.count = 1;
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&value).unwrap_err(),
        "naruon-copy-readiness-count-overflow"
    );

    let mut value = baseline.clone();
    value.production_time_evidence.embedded_metadata.bytes = u64::MAX;
    value.production_time_evidence.explicit_filename_date.bytes = 1;
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&value).unwrap_err(),
        "naruon-copy-readiness-bytes-overflow"
    );

    let mut aggregate_variants = Vec::new();
    let mut value = baseline.clone();
    value.candidate_count = 1;
    aggregate_variants.push(value);
    let mut value = baseline.clone();
    value.candidate_bytes = 1;
    aggregate_variants.push(value);
    let mut value = baseline.clone();
    value.planner_unblocked.count = 1;
    aggregate_variants.push(value);
    let mut value = baseline.clone();
    value.requires_human_review.count = 1;
    aggregate_variants.push(value);
    let mut value = baseline.clone();
    value.ready_without_new_review.count = 1;
    aggregate_variants.push(value);
    let mut value = baseline.clone();
    value.planner_unblocked.bytes = 1;
    aggregate_variants.push(value);
    let mut value = baseline.clone();
    value.requires_human_review.bytes = 1;
    aggregate_variants.push(value);
    let mut value = baseline.clone();
    value.ready_without_new_review.bytes = 1;
    aggregate_variants.push(value);

    let mut value = baseline.clone();
    one_candidate_shape(&mut value);
    value.ready_without_new_review = CountBytes { count: 1, bytes: 0 };
    aggregate_variants.push(value);

    let mut value = baseline.clone();
    one_candidate_shape(&mut value);
    value.planner_unblocked = CountBytes { count: 1, bytes: 0 };
    value.ready_without_new_review = CountBytes { count: 0, bytes: 1 };
    aggregate_variants.push(value);

    let mut value = baseline;
    value.candidate_bytes = 1;
    value.production_time_evidence.unclassified.bytes = 1;
    value.planner_unblocked.bytes = 1;
    aggregate_variants.push(value);

    for envelope in aggregate_variants {
        assert_eq!(
            validate_naruon_cloud_copy_readiness(&envelope).unwrap_err(),
            "naruon-copy-readiness-aggregate-invalid"
        );
    }
}

#[test]
fn aggregate_binding_state_and_fingerprint_drift_fail_closed() {
    let baseline = baseline();

    let mut value = baseline.clone();
    value
        .candidate_blocker_counts
        .insert("capacity-unavailable".into(), CountBytes { count: 1, bytes: 0 });
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&value).unwrap_err(),
        "naruon-copy-readiness-blocker-aggregate-invalid"
    );

    let mut value = baseline.clone();
    one_candidate_shape(&mut value);
    value.requires_human_review = CountBytes { count: 1, bytes: 1 };
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&value).unwrap_err(),
        "naruon-copy-readiness-review-binding-invalid"
    );

    let mut value = baseline.clone();
    value
        .candidate_blocker_counts
        .insert("review-required".into(), CountBytes::default());
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&value).unwrap_err(),
        "naruon-copy-readiness-review-binding-invalid"
    );

    let mut value = baseline.clone();
    one_candidate_shape(&mut value);
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&value).unwrap_err(),
        "naruon-copy-readiness-planner-binding-invalid"
    );

    let mut value = baseline.clone();
    value
        .candidate_blocker_counts
        .insert("planner-blocked".into(), CountBytes::default());
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&value).unwrap_err(),
        "naruon-copy-readiness-planner-binding-invalid"
    );

    let mut value = baseline.clone();
    value
        .candidate_blocker_counts
        .insert("capacity-unavailable".into(), CountBytes::default());
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&value).unwrap_err(),
        "naruon-copy-readiness-empty-blockers-invalid"
    );

    let mut value = baseline.clone();
    one_candidate_shape(&mut value);
    value.planner_unblocked = CountBytes { count: 1, bytes: 1 };
    value.potentially_reclaimable_bytes = 1;
    let capacity_snapshot = value.capacity.snapshot.clone();
    let capacity_reserve_bytes = value.capacity.reserve_bytes;
    value.capacity = assess_capacity(capacity_snapshot, 1, 1, capacity_reserve_bytes);
    value.requires_human_review = CountBytes { count: 1, bytes: 1 };
    value.ready_without_new_review = CountBytes { count: 1, bytes: 1 };
    value
        .candidate_blocker_counts
        .insert("review-required".into(), CountBytes { count: 1, bytes: 1 });
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&value).unwrap_err(),
        "naruon-copy-readiness-review-overlap-invalid"
    );

    let mut value = baseline.clone();
    one_candidate_shape(&mut value);
    value.planner_unblocked = CountBytes { count: 1, bytes: 1 };
    value.potentially_reclaimable_bytes = 1;
    value.ready_without_new_review = CountBytes { count: 1, bytes: 1 };
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&value).unwrap_err(),
        "naruon-copy-readiness-ready-gate-invalid"
    );

    let mut value = baseline.clone();
    one_candidate_shape(&mut value);
    value
        .candidate_blocker_counts
        .insert("planner-blocked".into(), CountBytes { count: 1, bytes: 1 });
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&value).unwrap_err(),
        "naruon-copy-readiness-runtime-binding-invalid"
    );

    let mut value = baseline.clone();
    value.readiness_state = CloudCopyReadinessState::Blocked;
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&value).unwrap_err(),
        "naruon-copy-readiness-state-invalid"
    );

    let mut value = baseline;
    value.readiness_fingerprint_sha256 = "0".repeat(64);
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&value).unwrap_err(),
        "naruon-copy-readiness-fingerprint-invalid"
    );
}