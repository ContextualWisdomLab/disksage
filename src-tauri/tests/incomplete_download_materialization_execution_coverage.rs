//! Public contract coverage for incomplete-download materialization receipts.
//!
//! These tests exercise the shipped integrity validator through its public API so exact-head
//! coverage measures fail-closed receipt admission rather than source-string or private-helper
//! surrogates. No filesystem, provider, network, or mutation authority is used.

use disksage_lib::cloud::{CloudAccountScope, CloudProvider};
use disksage_lib::content_digest::ContentDigests;
use disksage_lib::incomplete_download_materialization::MaterializationUnitKind;
use disksage_lib::incomplete_download_materialization_execution::{
    incomplete_download_materialization_receipt_integrity_valid,
    summarize_incomplete_download_materialization_receipt, IncompleteDownloadMaterializationReceipt,
    IncompleteDownloadMaterializedUnit, INCOMPLETE_DOWNLOAD_EXECUTION_VERSION,
};
use disksage_lib::provider_capacity::{
    assess_capacity, CapacityEvidenceKind, CloudCapacitySnapshot, CloudCapacityState,
    CAPACITY_SCHEMA_VERSION,
};

fn capacity_snapshot() -> CloudCapacitySnapshot {
    CloudCapacitySnapshot {
        schema_version: CAPACITY_SCHEMA_VERSION,
        provider: CloudProvider::Icloud,
        account_scope: Some(CloudAccountScope::Personal),
        evidence_kind: CapacityEvidenceKind::ProviderNativeStatus,
        observed_at_ms: 1_000,
        total_bytes: None,
        used_bytes: None,
        remaining_bytes: Some(10_000),
        trashed_bytes: None,
        max_upload_size_bytes: None,
        state: CloudCapacityState::Available,
        evidence_fingerprint: Some("a".repeat(64)),
        unavailable_reason: None,
    }
}

fn materialized_unit() -> IncompleteDownloadMaterializedUnit {
    IncompleteDownloadMaterializedUnit {
        materialization_unit_fingerprint: "b".repeat(64),
        kind: MaterializationUnitKind::FullZipFile,
        source_relative_path: "source.part".into(),
        range_start: 0,
        range_end: 10,
        destination_relative_path: "Recovered/output.zip".into(),
        output_bytes: 10,
        content_digests: ContentDigests {
            blake3: "c".repeat(64),
            sha256: "d".repeat(64),
            quick_xor_base64: format!("{}=", "A".repeat(27)),
        },
        source_stable: true,
        output_verified: true,
        write_performed: true,
    }
}

fn receipt_fixture() -> IncompleteDownloadMaterializationReceipt {
    let snapshot = capacity_snapshot();
    IncompleteDownloadMaterializationReceipt {
        schema_version: INCOMPLETE_DOWNLOAD_EXECUTION_VERSION,
        // A syntactically valid but intentionally non-authoritative ID lets the public validator
        // traverse every structural invariant before its final content-derived ID comparison.
        receipt_id: "0".repeat(64),
        destination_plan_fingerprint: "1".repeat(64),
        materialization_plan_fingerprint: "2".repeat(64),
        approval_id: "3".repeat(64),
        provider: CloudProvider::Icloud,
        account_scope: CloudAccountScope::Personal,
        cloud_root: "/cloud".into(),
        destination_subdirectory: "Recovered".into(),
        executed_at_ms: 1_001,
        fresh_capacity: assess_capacity(snapshot, 10, 10, 0),
        source_file_count: 1,
        unit_count: 1,
        materialized_bytes: 10,
        units: vec![materialized_unit()],
        all_outputs_verified: true,
        provider_sync_confirmed: false,
        source_eviction_authorized: false,
        source_mutation_performed: false,
        production_time_ms: None,
        production_time_source: None,
    }
}

fn assert_rejected(mut mutate: impl FnMut(&mut IncompleteDownloadMaterializationReceipt)) {
    let mut receipt = receipt_fixture();
    mutate(&mut receipt);
    assert!(!incomplete_download_materialization_receipt_integrity_valid(&receipt));
}

#[test]
fn receipt_validator_rejects_fail_closed_metadata_and_authority_drift() {
    let baseline = receipt_fixture();
    assert!(!incomplete_download_materialization_receipt_integrity_valid(&baseline));

    assert_rejected(|receipt| receipt.schema_version = 0);
    assert_rejected(|receipt| receipt.receipt_id = "not-a-digest".into());
    assert_rejected(|receipt| receipt.account_scope = CloudAccountScope::Unknown);
    assert_rejected(|receipt| receipt.cloud_root = "relative/cloud".into());
    assert_rejected(|receipt| receipt.destination_subdirectory = "../escape".into());
    assert_rejected(|receipt| receipt.source_file_count = 0);
    assert_rejected(|receipt| receipt.unit_count = 0);
    assert_rejected(|receipt| receipt.materialized_bytes = 0);
    assert_rejected(|receipt| receipt.all_outputs_verified = false);
    assert_rejected(|receipt| receipt.provider_sync_confirmed = true);
    assert_rejected(|receipt| receipt.source_eviction_authorized = true);
    assert_rejected(|receipt| receipt.source_mutation_performed = true);
    assert_rejected(|receipt| receipt.production_time_ms = Some(1_001));
    assert_rejected(|receipt| receipt.production_time_source = Some("filename".into()));
    assert_rejected(|receipt| receipt.fresh_capacity.can_fit = Some(false));
    assert_rejected(|receipt| receipt.fresh_capacity.blockers.push("blocked".into()));
    assert_rejected(|receipt| receipt.fresh_capacity.snapshot.provider = CloudProvider::Onedrive);
    assert_rejected(|receipt| {
        receipt.fresh_capacity.snapshot.account_scope = Some(CloudAccountScope::Organization)
    });
    assert_rejected(|receipt| receipt.fresh_capacity.snapshot.observed_at_ms = 1_002);
    assert_rejected(|receipt| receipt.fresh_capacity.snapshot.evidence_fingerprint = Some("x".into()));
    assert_rejected(|receipt| receipt.fresh_capacity.requested_bytes = 9);
}

#[test]
fn receipt_validator_rejects_unsafe_or_internally_inconsistent_units() {
    assert_rejected(|receipt| receipt.units[0].materialization_unit_fingerprint = "x".into());
    assert_rejected(|receipt| receipt.units[0].source_relative_path = "/absolute/source".into());
    assert_rejected(|receipt| receipt.units[0].destination_relative_path = "/absolute/output".into());
    assert_rejected(|receipt| receipt.units[0].destination_relative_path = "Other/output.zip".into());
    assert_rejected(|receipt| receipt.units[0].range_end = 0);
    assert_rejected(|receipt| receipt.units[0].output_bytes = 9);
    assert_rejected(|receipt| receipt.units[0].content_digests.blake3 = "x".into());
    assert_rejected(|receipt| receipt.units[0].content_digests.sha256 = "x".into());
    assert_rejected(|receipt| receipt.units[0].content_digests.quick_xor_base64 = "x".into());
    assert_rejected(|receipt| receipt.units[0].source_stable = false);
    assert_rejected(|receipt| receipt.units[0].output_verified = false);
    assert_rejected(|receipt| receipt.units[0].write_performed = false);

    assert_rejected(|receipt| {
        let mut second = receipt.units[0].clone();
        second.source_relative_path = "other.part".into();
        second.destination_relative_path = "Recovered/other.zip".into();
        receipt.units.push(second);
        receipt.source_file_count = 2;
        receipt.unit_count = 2;
        receipt.materialized_bytes = 20;
        receipt.fresh_capacity = assess_capacity(capacity_snapshot(), 20, 10, 0);
    });

    assert_rejected(|receipt| {
        let mut second = receipt.units[0].clone();
        second.materialization_unit_fingerprint = "e".repeat(64);
        second.source_relative_path = "other.part".into();
        receipt.units.push(second);
        receipt.source_file_count = 2;
        receipt.unit_count = 2;
        receipt.materialized_bytes = 20;
        receipt.fresh_capacity = assess_capacity(capacity_snapshot(), 20, 10, 0);
    });

    assert_rejected(|receipt| {
        let mut second = receipt.units[0].clone();
        second.materialization_unit_fingerprint = "e".repeat(64);
        second.destination_relative_path = "Recovered/other.zip".into();
        second.range_start = 5;
        second.range_end = 15;
        receipt.units.push(second);
        receipt.unit_count = 2;
        receipt.materialized_bytes = 20;
        receipt.fresh_capacity = assess_capacity(capacity_snapshot(), 20, 10, 0);
    });
}

#[test]
fn receipt_summary_redacts_path_range_and_digest_details() {
    let receipt = receipt_fixture();
    let summary = summarize_incomplete_download_materialization_receipt(&receipt);
    let encoded = serde_json::to_string(&summary).expect("summary must serialize");

    assert_eq!(summary.output_mode, "redacted-materialization-receipt-summary");
    assert!(summary.paths_names_ranges_and_digests_redacted);
    assert!(summary.filesystem_times_used_only_for_source_stability);
    assert!(!summary.filename_date_used_as_production_time);
    assert!(!summary.production_time_assigned);
    assert!(!encoded.contains(&receipt.cloud_root));
    assert!(!encoded.contains(&receipt.destination_subdirectory));
    assert!(!encoded.contains(&receipt.units[0].source_relative_path));
    assert!(!encoded.contains(&receipt.units[0].destination_relative_path));
    assert!(!encoded.contains(&receipt.units[0].content_digests.sha256));
}
