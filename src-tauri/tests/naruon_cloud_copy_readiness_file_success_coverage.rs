//! Credential-free success-path coverage for the NarUon readiness file reader.
//!
//! The fixture is produced by DiskSage's real deterministic export path, serialized to one
//! bounded temporary file, and read back through the public no-symlink admission boundary. No
//! provider request, credential, cloud write, or source-eviction authority is exercised.

use disksage_lib::cloud::{
    CloudAccountScope, CloudPlanOptions, CloudPlanReport, CloudProvider, CloudRoot,
    ExactDuplicateSummary,
};
use disksage_lib::naruon_cloud_copy_readiness::{
    export_naruon_cloud_copy_readiness, read_and_validate_naruon_cloud_copy_readiness,
    validate_naruon_cloud_copy_readiness,
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
            id: "file-success-coverage-root".into(),
            provider,
            account_scope: CloudAccountScope::Personal,
            label: "File success coverage root".into(),
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
fn readiness_file_reader_round_trips_a_valid_exported_envelope() {
    let expected = valid_empty_envelope();
    assert!(validate_naruon_cloud_copy_readiness(&expected).is_ok());

    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("naruon-copy-readiness.json");
    std::fs::write(&path, serde_json::to_vec(&expected).unwrap()).unwrap();

    let observed = read_and_validate_naruon_cloud_copy_readiness(&path).unwrap();

    assert_eq!(observed.schema_kind, expected.schema_kind);
    assert_eq!(observed.schema_version, expected.schema_version);
    assert_eq!(observed.provider, expected.provider);
    assert_eq!(observed.candidate_count, 0);
    assert_eq!(observed.candidate_bytes, 0);
    assert_eq!(
        observed.readiness_fingerprint_sha256,
        expected.readiness_fingerprint_sha256
    );
    assert!(!observed.cloud_write_executed);
    assert!(!observed.source_eviction_authorized);
    assert!(validate_naruon_cloud_copy_readiness(&observed).is_ok());
}
