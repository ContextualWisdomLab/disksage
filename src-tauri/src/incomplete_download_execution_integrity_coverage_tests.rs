//! Fail-closed integrity coverage for incomplete-download materialization receipts.
//!
//! The production validator protects a receipt that can later be shown to an operator as proof of
//! what bytes were materialized. These tests start from one internally consistent receipt and then
//! tamper authority-bearing fields one family at a time. They exercise the shipped integrity
//! boundary without performing cloud writes or source mutation.

use crate::cloud::{CloudAccountScope, CloudProvider};
use crate::content_digest::ContentDigests;
use crate::incomplete_download_materialization::MaterializationUnitKind;
use crate::incomplete_download_materialization_execution::{
    incomplete_download_materialization_receipt_integrity_valid,
    IncompleteDownloadMaterializationReceipt, IncompleteDownloadMaterializedUnit,
    INCOMPLETE_DOWNLOAD_EXECUTION_VERSION,
};
use crate::provider_capacity::{assess_capacity, parse_icloud_brctl_quota};

fn hex(byte: char) -> String {
    std::iter::repeat_n(byte, 64).collect()
}

fn quick_xor() -> String {
    "AAAAAAAAAAAAAAAAAAAAAAAAAAA=".into()
}

fn receipt_id_for_test(receipt: &IncompleteDownloadMaterializationReceipt) -> String {
    let mut unsigned = receipt.clone();
    unsigned.receipt_id.clear();
    let encoded = serde_json::to_vec(&unsigned).expect("receipt fixture must serialize");
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-incomplete-download-materialization-receipt-v1\0");
    hasher.update(&encoded);
    hasher.finalize().to_hex().to_string()
}

fn valid_receipt() -> IncompleteDownloadMaterializationReceipt {
    let output_bytes = 10;
    let snapshot = parse_icloud_brctl_quota(
        "10000000000 bytes of quota remaining in personal account\n",
        900,
    )
    .expect("bounded native capacity fixture");
    let fresh_capacity = assess_capacity(snapshot, output_bytes, output_bytes, 0);
    assert_eq!(fresh_capacity.can_fit, Some(true));
    assert!(fresh_capacity.blockers.is_empty());

    let unit = IncompleteDownloadMaterializedUnit {
        materialization_unit_fingerprint: hex('d'),
        kind: MaterializationUnitKind::FullZipFile,
        source_relative_path: "incoming/download.crdownload".into(),
        range_start: 0,
        range_end: output_bytes,
        destination_relative_path: "Recovered/output.zip".into(),
        output_bytes,
        content_digests: ContentDigests {
            blake3: hex('a'),
            sha256: hex('b'),
            quick_xor_base64: quick_xor(),
        },
        source_stable: true,
        output_verified: true,
        write_performed: true,
    };
    let mut receipt = IncompleteDownloadMaterializationReceipt {
        schema_version: INCOMPLETE_DOWNLOAD_EXECUTION_VERSION,
        receipt_id: String::new(),
        destination_plan_fingerprint: hex('1'),
        materialization_plan_fingerprint: hex('2'),
        approval_id: hex('3'),
        provider: CloudProvider::Icloud,
        account_scope: CloudAccountScope::Personal,
        cloud_root: "/tmp/disksage-cloud-root".into(),
        destination_subdirectory: "Recovered".into(),
        executed_at_ms: 1_000,
        fresh_capacity,
        source_file_count: 1,
        unit_count: 1,
        materialized_bytes: output_bytes,
        units: vec![unit],
        all_outputs_verified: true,
        provider_sync_confirmed: false,
        source_eviction_authorized: false,
        source_mutation_performed: false,
        production_time_ms: None,
        production_time_source: None,
    };
    receipt.receipt_id = receipt_id_for_test(&receipt);
    assert!(incomplete_download_materialization_receipt_integrity_valid(&receipt));
    receipt
}

#[test]
fn receipt_validator_rejects_top_level_authority_tampering() {
    let receipt = valid_receipt();

    let mut tampered = receipt.clone();
    tampered.schema_version += 1;
    assert!(!incomplete_download_materialization_receipt_integrity_valid(&tampered));

    for field in 0..4 {
        let mut tampered = receipt.clone();
        match field {
            0 => tampered.receipt_id = "not-a-fingerprint".into(),
            1 => tampered.destination_plan_fingerprint = "0".into(),
            2 => tampered.materialization_plan_fingerprint = "g".repeat(64),
            _ => tampered.approval_id.clear(),
        }
        assert!(!incomplete_download_materialization_receipt_integrity_valid(&tampered));
    }

    let mut tampered = receipt.clone();
    tampered.account_scope = CloudAccountScope::Unknown;
    assert!(!incomplete_download_materialization_receipt_integrity_valid(&tampered));

    let mut tampered = receipt.clone();
    tampered.cloud_root = "relative/cloud".into();
    assert!(!incomplete_download_materialization_receipt_integrity_valid(&tampered));

    let mut tampered = receipt.clone();
    tampered.destination_subdirectory = "../escape".into();
    assert!(!incomplete_download_materialization_receipt_integrity_valid(&tampered));

    let mut tampered = receipt.clone();
    tampered.source_file_count = 0;
    assert!(!incomplete_download_materialization_receipt_integrity_valid(&tampered));

    let mut tampered = receipt.clone();
    tampered.unit_count = 2;
    assert!(!incomplete_download_materialization_receipt_integrity_valid(&tampered));

    let mut tampered = receipt.clone();
    tampered.units.clear();
    tampered.unit_count = 0;
    assert!(!incomplete_download_materialization_receipt_integrity_valid(&tampered));

    let mut tampered = receipt.clone();
    tampered.materialized_bytes = 0;
    assert!(!incomplete_download_materialization_receipt_integrity_valid(&tampered));

    for flag in 0..6 {
        let mut tampered = receipt.clone();
        match flag {
            0 => tampered.all_outputs_verified = false,
            1 => tampered.provider_sync_confirmed = true,
            2 => tampered.source_eviction_authorized = true,
            3 => tampered.source_mutation_performed = true,
            4 => tampered.production_time_ms = Some(1),
            _ => tampered.production_time_source = Some("filesystem:modified".into()),
        }
        assert!(!incomplete_download_materialization_receipt_integrity_valid(&tampered));
    }
}

#[test]
fn receipt_validator_rejects_capacity_authority_tampering() {
    let receipt = valid_receipt();

    let mut tampered = receipt.clone();
    tampered.fresh_capacity.can_fit = Some(false);
    assert!(!incomplete_download_materialization_receipt_integrity_valid(&tampered));

    let mut tampered = receipt.clone();
    tampered.fresh_capacity.blockers.push("forged-blocker".into());
    assert!(!incomplete_download_materialization_receipt_integrity_valid(&tampered));

    let mut tampered = receipt.clone();
    tampered.fresh_capacity.snapshot.provider = CloudProvider::GoogleDrive;
    assert!(!incomplete_download_materialization_receipt_integrity_valid(&tampered));

    let mut tampered = receipt.clone();
    tampered.fresh_capacity.snapshot.account_scope = Some(CloudAccountScope::Unknown);
    assert!(!incomplete_download_materialization_receipt_integrity_valid(&tampered));

    let mut tampered = receipt.clone();
    tampered.fresh_capacity.snapshot.observed_at_ms = tampered.executed_at_ms + 1;
    assert!(!incomplete_download_materialization_receipt_integrity_valid(&tampered));

    let mut tampered = receipt.clone();
    tampered.fresh_capacity.snapshot.evidence_fingerprint = Some("not-hex".into());
    assert!(!incomplete_download_materialization_receipt_integrity_valid(&tampered));

    let mut tampered = receipt.clone();
    tampered.fresh_capacity.requested_bytes += 1;
    assert!(!incomplete_download_materialization_receipt_integrity_valid(&tampered));

    let mut tampered = receipt;
    tampered.fresh_capacity.reserve_bytes += 1;
    assert!(!incomplete_download_materialization_receipt_integrity_valid(&tampered));
}

#[test]
fn receipt_validator_rejects_unit_and_range_tampering() {
    let receipt = valid_receipt();

    let mut tampered = receipt.clone();
    tampered.units[0].materialization_unit_fingerprint = "x".repeat(64);
    assert!(!incomplete_download_materialization_receipt_integrity_valid(&tampered));

    let mut tampered = receipt.clone();
    tampered.units[0].source_relative_path = "/absolute/source".into();
    assert!(!incomplete_download_materialization_receipt_integrity_valid(&tampered));

    let mut tampered = receipt.clone();
    tampered.units[0].destination_relative_path = "Other/output.zip".into();
    assert!(!incomplete_download_materialization_receipt_integrity_valid(&tampered));

    let mut tampered = receipt.clone();
    tampered.units[0].range_end = tampered.units[0].range_start;
    assert!(!incomplete_download_materialization_receipt_integrity_valid(&tampered));

    let mut tampered = receipt.clone();
    tampered.units[0].output_bytes += 1;
    assert!(!incomplete_download_materialization_receipt_integrity_valid(&tampered));

    for digest in 0..3 {
        let mut tampered = receipt.clone();
        match digest {
            0 => tampered.units[0].content_digests.blake3 = "f".repeat(63),
            1 => tampered.units[0].content_digests.sha256 = "Z".repeat(64),
            _ => tampered.units[0].content_digests.quick_xor_base64 = "short".into(),
        }
        assert!(!incomplete_download_materialization_receipt_integrity_valid(&tampered));
    }

    for flag in 0..3 {
        let mut tampered = receipt.clone();
        match flag {
            0 => tampered.units[0].source_stable = false,
            1 => tampered.units[0].output_verified = false,
            _ => tampered.units[0].write_performed = false,
        }
        assert!(!incomplete_download_materialization_receipt_integrity_valid(&tampered));
    }

    let mut tampered = receipt.clone();
    let duplicate = tampered.units[0].clone();
    tampered.units.push(duplicate);
    tampered.unit_count = 2;
    tampered.source_file_count = 1;
    tampered.materialized_bytes *= 2;
    tampered.fresh_capacity = assess_capacity(
        tampered.fresh_capacity.snapshot.clone(),
        tampered.materialized_bytes,
        tampered.units[0].output_bytes,
        0,
    );
    tampered.receipt_id = receipt_id_for_test(&tampered);
    assert!(!incomplete_download_materialization_receipt_integrity_valid(&tampered));

    let mut tampered = receipt;
    let mut overlapping = tampered.units[0].clone();
    overlapping.materialization_unit_fingerprint = hex('e');
    overlapping.destination_relative_path = "Recovered/output-2.zip".into();
    overlapping.range_start = 5;
    overlapping.range_end = 15;
    tampered.units.push(overlapping);
    tampered.unit_count = 2;
    tampered.materialized_bytes = 20;
    tampered.fresh_capacity = assess_capacity(
        tampered.fresh_capacity.snapshot.clone(),
        tampered.materialized_bytes,
        10,
        0,
    );
    tampered.receipt_id = receipt_id_for_test(&tampered);
    assert!(!incomplete_download_materialization_receipt_integrity_valid(&tampered));
}
