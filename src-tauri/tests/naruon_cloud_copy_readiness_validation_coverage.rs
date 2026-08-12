//! Credential-free branch coverage for the NarUon readiness envelope validator.
//!
//! The fixture is produced by the real deterministic export path from a zero-candidate local plan.
//! Mutations then prove that malformed authority/evidence claims fail closed before fingerprint
//! acceptance. No provider request, credential, cloud write, or source eviction is performed.

use disksage_lib::cloud::{
    CloudAccountScope, CloudPlanOptions, CloudPlanReport, CloudProvider, CloudRoot,
    ExactDuplicateSummary,
};
use disksage_lib::naruon_cloud_copy_readiness::{
    export_naruon_cloud_copy_readiness, validate_naruon_cloud_copy_readiness,
    CloudCopyReadinessState, CountBytes,
};
use disksage_lib::provider_capacity::{
    assess_capacity, unavailable_capacity, DEFAULT_CAPACITY_RESERVE_BYTES,
};
use disksage_lib::provider_client_runtime::assess_provider_client_runtime;

fn valid_empty_envelope() -> disksage_lib::naruon_cloud_copy_readiness::NaruonCloudCopyReadinessEnvelope {
    let provider = CloudProvider::Onedrive;
    let capacity = assess_capacity(
        unavailable_capacity(provider, 10, "capacity-unavailable"),
        0,
        0,
        DEFAULT_CAPACITY_RESERVE_BYTES,
    );
    let report = CloudPlanReport {
        cloud_root: CloudRoot {
            id: "coverage-root".into(),
            provider,
            account_scope: CloudAccountScope::Personal,
            label: "Coverage root".into(),
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

#[test]
fn canonical_zero_candidate_envelope_is_valid_and_non_authorizing() {
    let envelope = valid_empty_envelope();

    assert!(validate_naruon_cloud_copy_readiness(&envelope).is_ok());
    assert_eq!(envelope.candidate_count, 0);
    assert_eq!(envelope.candidate_bytes, 0);
    assert_eq!(envelope.readiness_state, CloudCopyReadinessState::NoCandidates);
    assert!(!envelope.cloud_write_executed);
    assert!(!envelope.source_eviction_authorized);
    assert!(!envelope.human_review_decisions_applied);
    assert_eq!(envelope.readiness_fingerprint_sha256.len(), 64);
}

#[test]
fn schema_identity_variants_fail_closed_before_other_claims() {
    let mut variants = Vec::new();

    let mut value = valid_empty_envelope();
    value.schema_kind = "wrong-kind".into();
    variants.push(value);

    let mut value = valid_empty_envelope();
    value.schema_version = value.schema_version.saturating_add(1);
    variants.push(value);

    let mut value = valid_empty_envelope();
    value.decision_batch_fingerprint_version =
        value.decision_batch_fingerprint_version.saturating_add(1);
    variants.push(value);

    let mut value = valid_empty_envelope();
    value.decision_batch_fingerprint = "A".repeat(64);
    variants.push(value);

    let mut value = valid_empty_envelope();
    value.decision_batch_fingerprint = "a".repeat(63);
    variants.push(value);

    let mut value = valid_empty_envelope();
    value.readiness_fingerprint_canonicalization = "unsupported".into();
    variants.push(value);

    for envelope in variants {
        assert_eq!(
            validate_naruon_cloud_copy_readiness(&envelope).unwrap_err(),
            "naruon-copy-readiness-schema-invalid"
        );
    }
}

#[test]
fn authority_and_privacy_claim_variants_fail_closed() {
    let mut variants = Vec::new();

    for field in 0..9 {
        let mut value = valid_empty_envelope();
        match field {
            0 => value.local_paths_included = true,
            1 => value.relative_names_included = true,
            2 => value.raw_metadata_values_included = true,
            3 => value.account_identifiers_included = true,
            4 => value.provider_sync_attested = true,
            5 => value.cloud_write_executed = true,
            6 => value.source_eviction_authorized = true,
            7 => value.human_review_decisions_applied = true,
            8 => value.filename_dates_are_auxiliary = false,
            _ => unreachable!(),
        }
        variants.push(value);
    }

    let mut wrong_policy = valid_empty_envelope();
    wrong_policy.metadata_policy.swap(0, 1);
    variants.push(wrong_policy);

    for envelope in variants {
        assert_eq!(
            validate_naruon_cloud_copy_readiness(&envelope).unwrap_err(),
            "naruon-copy-readiness-policy-claim-invalid"
        );
    }
}

#[test]
fn selection_provider_capacity_and_icloud_bindings_fail_closed() {
    for mutate in 0..3 {
        let mut value = valid_empty_envelope();
        match mutate {
            0 => value.source_selection_policy.min_size_bytes = 0,
            1 => value.source_selection_policy.limit = 0,
            2 => value.source_selection_policy.limit = 1_001,
            _ => unreachable!(),
        }
        assert_eq!(
            validate_naruon_cloud_copy_readiness(&value).unwrap_err(),
            "naruon-copy-readiness-selection-policy-invalid"
        );
    }

    let mut provider = valid_empty_envelope();
    provider.provider = CloudProvider::GoogleDrive;
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&provider).unwrap_err(),
        "naruon-copy-readiness-provider-binding-invalid"
    );

    let mut capacity = valid_empty_envelope();
    let capacity_snapshot = capacity.capacity.snapshot.clone();
    let capacity_largest_candidate_bytes = capacity.capacity.largest_candidate_bytes;
    let capacity_reserve_bytes = capacity.capacity.reserve_bytes;
    capacity.capacity = assess_capacity(
        capacity_snapshot,
        1,
        capacity_largest_candidate_bytes,
        capacity_reserve_bytes,
    );
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&capacity).unwrap_err(),
        "naruon-copy-readiness-capacity-binding-invalid"
    );

    let mut icloud = valid_empty_envelope();
    icloud.icloud_new_copy_admission_met = Some(false);
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&icloud).unwrap_err(),
        "naruon-copy-readiness-icloud-binding-invalid"
    );
}

#[test]
fn blocker_bounds_aggregate_state_and_fingerprint_checks_are_independent() {
    let mut invalid_reason = valid_empty_envelope();
    invalid_reason
        .candidate_blocker_counts
        .insert("-invalid".into(), CountBytes::default());
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&invalid_reason).unwrap_err(),
        "naruon-copy-readiness-blocker-invalid"
    );

    let mut bounds = valid_empty_envelope();
    bounds.candidate_count = 1_001;
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&bounds).unwrap_err(),
        "naruon-copy-readiness-bounds-invalid"
    );

    let mut aggregate = valid_empty_envelope();
    aggregate.candidate_count = 1;
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&aggregate).unwrap_err(),
        "naruon-copy-readiness-aggregate-invalid"
    );

    let mut blocker_aggregate = valid_empty_envelope();
    blocker_aggregate.candidate_blocker_counts.insert(
        "coverage-blocker".into(),
        CountBytes { count: 1, bytes: 0 },
    );
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&blocker_aggregate).unwrap_err(),
        "naruon-copy-readiness-blocker-aggregate-invalid"
    );

    let mut empty_blockers = valid_empty_envelope();
    empty_blockers
        .candidate_blocker_counts
        .insert("coverage-blocker".into(), CountBytes::default());
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&empty_blockers).unwrap_err(),
        "naruon-copy-readiness-empty-blockers-invalid"
    );

    let mut state = valid_empty_envelope();
    state.readiness_state = CloudCopyReadinessState::Blocked;
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&state).unwrap_err(),
        "naruon-copy-readiness-state-invalid"
    );

    let mut fingerprint = valid_empty_envelope();
    fingerprint.readiness_fingerprint_sha256 = "0".repeat(64);
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&fingerprint).unwrap_err(),
        "naruon-copy-readiness-fingerprint-invalid"
    );
}
