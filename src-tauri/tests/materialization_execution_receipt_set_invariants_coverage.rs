//! Public-boundary coverage for materialization receipt aggregate invariants.
//!
//! Existing exact-head tests already cover duplicate unit fingerprints, duplicate destinations,
//! overlapping ranges, future-dated capacity evidence, and basic capacity-field mismatches. These
//! regressions deliberately target remaining aggregate and freshness checks: source cardinality,
//! total materialized bytes, recomputed capacity assessment, and provider evidence that has aged
//! beyond the execution admission window. They use only the public receipt validator and
//! deterministic provider-capacity evidence.

use disksage_lib::cloud::{CloudAccountScope, CloudProvider};
use disksage_lib::content_digest::ContentDigests;
use disksage_lib::incomplete_download_materialization::MaterializationUnitKind;
use disksage_lib::incomplete_download_materialization_destination::MAX_CAPACITY_AGE_MS;
use disksage_lib::incomplete_download_materialization_execution::{
    incomplete_download_materialization_receipt_integrity_valid,
    IncompleteDownloadMaterializationReceipt, IncompleteDownloadMaterializedUnit,
    INCOMPLETE_DOWNLOAD_EXECUTION_VERSION,
};
use disksage_lib::provider_capacity::{assess_capacity, parse_icloud_brctl_quota};

const OBSERVED_AT_MS: u64 = 1_000;
const EXECUTED_AT_MS: u64 = 1_001;

fn unit(
    fingerprint: char,
    destination_name: &str,
    source_name: &str,
    range_start: u64,
    range_end: u64,
) -> IncompleteDownloadMaterializedUnit {
    IncompleteDownloadMaterializedUnit {
        materialization_unit_fingerprint: fingerprint.to_string().repeat(64),
        kind: MaterializationUnitKind::FullZipFile,
        source_relative_path: source_name.into(),
        range_start,
        range_end,
        destination_relative_path: format!("DiskSage/Recovered/{destination_name}"),
        output_bytes: range_end - range_start,
        content_digests: ContentDigests {
            blake3: "a".repeat(64),
            sha256: "b".repeat(64),
            quick_xor_base64: "A".repeat(28),
        },
        source_stable: true,
        output_verified: true,
        write_performed: true,
    }
}

fn receipt(
    units: Vec<IncompleteDownloadMaterializedUnit>,
    source_file_count: usize,
    materialized_bytes: u64,
) -> IncompleteDownloadMaterializationReceipt {
    let largest = units.iter().map(|unit| unit.output_bytes).max().unwrap_or_default();
    let snapshot = parse_icloud_brctl_quota(
        "10000000000 bytes of quota remaining in personal account\n",
        OBSERVED_AT_MS,
    )
    .unwrap();
    let fresh_capacity = assess_capacity(snapshot, materialized_bytes, largest, 0);
    assert_eq!(fresh_capacity.can_fit, Some(true));
    assert!(fresh_capacity.blockers.is_empty());

    IncompleteDownloadMaterializationReceipt {
        schema_version: INCOMPLETE_DOWNLOAD_EXECUTION_VERSION,
        // Syntactically valid but intentionally non-authoritative so each aggregate mutation can
        // be rejected before the final content-derived receipt-id equality check.
        receipt_id: "0".repeat(64),
        destination_plan_fingerprint: "1".repeat(64),
        materialization_plan_fingerprint: "2".repeat(64),
        approval_id: "3".repeat(64),
        provider: CloudProvider::Icloud,
        account_scope: CloudAccountScope::Personal,
        cloud_root: "/tmp/disksage-materialization-cloud".into(),
        destination_subdirectory: "DiskSage/Recovered".into(),
        executed_at_ms: EXECUTED_AT_MS,
        fresh_capacity,
        source_file_count,
        unit_count: units.len(),
        materialized_bytes,
        units,
        all_outputs_verified: true,
        provider_sync_confirmed: false,
        source_eviction_authorized: false,
        source_mutation_performed: false,
        production_time_ms: None,
        production_time_source: None,
    }
}

fn two_adjacent_units() -> Vec<IncompleteDownloadMaterializedUnit> {
    vec![
        unit('a', "first.zip", "source.zip.crdownload", 0, 10),
        unit('b', "second.zip", "source.zip.crdownload", 10, 20),
    ]
}

#[test]
fn receipt_integrity_rejects_source_and_byte_aggregate_drift() {
    let source_count_drift = receipt(two_adjacent_units(), 2, 20);
    assert!(!incomplete_download_materialization_receipt_integrity_valid(
        &source_count_drift
    ));

    let byte_count_drift = receipt(two_adjacent_units(), 1, 21);
    assert!(!incomplete_download_materialization_receipt_integrity_valid(
        &byte_count_drift
    ));
}

#[test]
fn receipt_integrity_rejects_recomputed_capacity_assessment_drift() {
    let mut assessment_drift = receipt(two_adjacent_units(), 1, 20);
    assessment_drift.fresh_capacity.largest_candidate_bytes += 1;

    assert!(!incomplete_download_materialization_receipt_integrity_valid(
        &assessment_drift
    ));
}

#[test]
fn receipt_integrity_rejects_capacity_evidence_older_than_execution_window() {
    let mut stale_capacity = receipt(two_adjacent_units(), 1, 20);
    stale_capacity.executed_at_ms = OBSERVED_AT_MS + MAX_CAPACITY_AGE_MS + 1;

    assert!(!incomplete_download_materialization_receipt_integrity_valid(
        &stale_capacity
    ));
}
