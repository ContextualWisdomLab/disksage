//! Credential-free edge coverage for the NarUon readiness validation boundary.
//!
//! Fixtures come from the real deterministic export path with zero cloud candidates. The tests
//! then tamper only in-memory evidence claims to prove provider binding and blocker reason-code
//! validation fail closed. No provider request, credential, cloud write, or source eviction occurs.

use disksage_lib::cloud::{
    CloudAccountScope, CloudPlanOptions, CloudPlanReport, CloudProvider, CloudRoot,
    ExactDuplicateSummary,
};
use disksage_lib::naruon_cloud_copy_readiness::{
    export_naruon_cloud_copy_readiness, validate_naruon_cloud_copy_readiness, CountBytes,
    NaruonCloudCopyReadinessEnvelope,
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
            id: "binding-edge-coverage-root".into(),
            provider,
            account_scope: CloudAccountScope::Personal,
            label: "Binding edge coverage root".into(),
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
fn derived_provider_gate_flags_cannot_be_forged() {
    let baseline = valid_empty_envelope();
    assert!(validate_naruon_cloud_copy_readiness(&baseline).is_ok());

    let mut runtime_gate = baseline.clone();
    runtime_gate.provider_runtime_prerequisite_met =
        !runtime_gate.provider_runtime.copy_prerequisite_met;
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&runtime_gate).unwrap_err(),
        "naruon-copy-readiness-provider-binding-invalid"
    );

    let mut capacity_gate = baseline;
    capacity_gate.remote_capacity_verified = true;
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&capacity_gate).unwrap_err(),
        "naruon-copy-readiness-provider-binding-invalid"
    );
}

#[test]
fn blocker_reason_code_grammar_rejects_each_invalid_shape() {
    for reason in [
        "".to_string(),
        "Uppercase".to_string(),
        "ends-".to_string(),
        "double--dash".to_string(),
        "x".repeat(129),
    ] {
        let mut envelope = valid_empty_envelope();
        envelope
            .candidate_blocker_counts
            .insert(reason, CountBytes::default());
        assert_eq!(
            validate_naruon_cloud_copy_readiness(&envelope).unwrap_err(),
            "naruon-copy-readiness-blocker-invalid"
        );
    }
}
