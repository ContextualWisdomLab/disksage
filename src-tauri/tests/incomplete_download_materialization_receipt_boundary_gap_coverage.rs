//! Public-boundary gap coverage for materialization-receipt integrity.
//!
//! These regressions exercise acceptance and fail-closed branches that remain
//! externally observable through the receipt validator. They add no filesystem,
//! provider, network, cloud-write, or source-eviction authority.

use disksage_lib::cloud::{CloudAccountScope, CloudProvider};
use disksage_lib::content_digest::ContentDigests;
use disksage_lib::incomplete_download_materialization::MaterializationUnitKind;
use disksage_lib::incomplete_download_materialization_execution::{
    incomplete_download_materialization_receipt_integrity_valid,
    IncompleteDownloadMaterializationReceipt, IncompleteDownloadMaterializedUnit,
    INCOMPLETE_DOWNLOAD_EXECUTION_VERSION,
};
use disksage_lib::provider_capacity::{
    assess_capacity, CapacityEvidenceKind, CloudCapacitySnapshot, CloudCapacityState,
    CAPACITY_SCHEMA_VERSION,
};

fn capacity_snapshot() -> CloudCapacitySnapshot {
    CloudCapacitySnapshot {
        schema_version: CAPACITY_SCHEMA_VERSION,
        provider: CloudProvider::Onedrive,
        account_scope: Some(CloudAccountScope::Personal),
        evidence_kind: CapacityEvidenceKind::ProviderApi,
        observed_at_ms: 100,
        total_bytes: Some(10_000),
        used_bytes: Some(0),
        remaining_bytes: Some(10_000),
        trashed_bytes: Some(0),
        max_upload_size_bytes: Some(10_000),
        state: CloudCapacityState::Normal,
        evidence_fingerprint: Some("a".repeat(64)),
        unavailable_reason: None,
    }
}

fn unit(
    fingerprint: char,
    source: &str,
    destination: &str,
    range_start: u64,
) -> IncompleteDownloadMaterializedUnit {
    IncompleteDownloadMaterializedUnit {
        materialization_unit_fingerprint: fingerprint.to_string().repeat(64),
        kind: MaterializationUnitKind::FullZipFile,
        source_relative_path: source.into(),
        range_start,
        range_end: range_start + 10,
        destination_relative_path: destination.into(),
        output_bytes: 10,
        content_digests: ContentDigests {
            blake3: "b".repeat(64),
            sha256: "c".repeat(64),
            quick_xor_base64: "A".repeat(28),
        },
        source_stable: true,
        output_verified: true,
        write_performed: true,
    }
}

fn sign(receipt: &mut IncompleteDownloadMaterializationReceipt) {
    let mut unsigned = receipt.clone();
    unsigned.receipt_id.clear();
    let encoded = serde_json::to_vec(&unsigned).expect("receipt fixture must serialize");
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-incomplete-download-materialization-receipt-v1\0");
    hasher.update(&encoded);
    receipt.receipt_id = hasher.finalize().to_hex().to_string();
}

fn valid_receipt() -> IncompleteDownloadMaterializationReceipt {
    let units = vec![unit(
        'd',
        "downloads/source.crdownload",
        "DiskSage/materialized.zip",
        0,
    )];
    let mut receipt = IncompleteDownloadMaterializationReceipt {
        schema_version: INCOMPLETE_DOWNLOAD_EXECUTION_VERSION,
        receipt_id: String::new(),
        destination_plan_fingerprint: "e".repeat(64),
        materialization_plan_fingerprint: "f".repeat(64),
        approval_id: "1".repeat(64),
        provider: CloudProvider::Onedrive,
        account_scope: CloudAccountScope::Personal,
        #[cfg(windows)]
        cloud_root: r"C:\Cloud".into(),
        #[cfg(not(windows))]
        cloud_root: "/Cloud".into(),
        destination_subdirectory: "DiskSage".into(),
        executed_at_ms: 101,
        fresh_capacity: assess_capacity(capacity_snapshot(), 10, 10, 0),
        source_file_count: 1,
        unit_count: 1,
        materialized_bytes: 10,
        units,
        all_outputs_verified: true,
        provider_sync_confirmed: false,
        source_eviction_authorized: false,
        source_mutation_performed: false,
        production_time_ms: None,
        production_time_source: None,
    };
    sign(&mut receipt);
    receipt
}

fn resign_and_reject(
    mut receipt: IncompleteDownloadMaterializationReceipt,
) -> IncompleteDownloadMaterializationReceipt {
    sign(&mut receipt);
    assert!(!incomplete_download_materialization_receipt_integrity_valid(&receipt));
    receipt
}

#[test]
fn receipt_integrity_accepts_adjacent_ranges_for_one_source() {
    let mut receipt = valid_receipt();
    receipt.units.push(unit(
        '2',
        "downloads/source.crdownload",
        "DiskSage/materialized-2.zip",
        10,
    ));
    receipt.unit_count = 2;
    receipt.materialized_bytes = 20;
    receipt.fresh_capacity = assess_capacity(capacity_snapshot(), 20, 10, 0);
    sign(&mut receipt);

    assert!(incomplete_download_materialization_receipt_integrity_valid(&receipt));
}

#[test]
fn receipt_integrity_accepts_two_distinct_sources_when_cardinality_matches() {
    let mut receipt = valid_receipt();
    receipt.units.push(unit(
        '2',
        "downloads/other.crdownload",
        "DiskSage/materialized-2.zip",
        0,
    ));
    receipt.source_file_count = 2;
    receipt.unit_count = 2;
    receipt.materialized_bytes = 20;
    receipt.fresh_capacity = assess_capacity(capacity_snapshot(), 20, 10, 0);
    sign(&mut receipt);

    assert!(incomplete_download_materialization_receipt_integrity_valid(&receipt));
}

#[test]
fn receipt_integrity_rejects_empty_sets_and_empty_relative_paths() {
    let mut empty_units = valid_receipt();
    empty_units.units.clear();
    empty_units.unit_count = 0;
    let _ = resign_and_reject(empty_units);

    let mut empty_subdirectory = valid_receipt();
    empty_subdirectory.destination_subdirectory.clear();
    let _ = resign_and_reject(empty_subdirectory);

    let mut empty_source = valid_receipt();
    empty_source.units[0].source_relative_path.clear();
    let _ = resign_and_reject(empty_source);

    let mut empty_destination = valid_receipt();
    empty_destination.units[0].destination_relative_path.clear();
    let _ = resign_and_reject(empty_destination);
}

#[test]
fn receipt_integrity_rejects_absolute_subdirectory_and_bad_quickxor_alphabet() {
    let mut absolute_subdirectory = valid_receipt();
    #[cfg(windows)]
    {
        absolute_subdirectory.destination_subdirectory = r"C:\DiskSage".into();
    }
    #[cfg(not(windows))]
    {
        absolute_subdirectory.destination_subdirectory = "/DiskSage".into();
    }
    let _ = resign_and_reject(absolute_subdirectory);

    let mut invalid_quickxor = valid_receipt();
    invalid_quickxor.units[0].content_digests.quick_xor_base64 = "!".repeat(28);
    let _ = resign_and_reject(invalid_quickxor);
}
