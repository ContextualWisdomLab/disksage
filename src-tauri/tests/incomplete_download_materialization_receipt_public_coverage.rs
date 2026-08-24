use disksage_lib::cloud::{CloudAccountScope, CloudProvider};
use disksage_lib::content_digest::ContentDigests;
use disksage_lib::incomplete_download_materialization::MaterializationUnitKind;
use disksage_lib::incomplete_download_materialization_execution::{
    incomplete_download_materialization_receipt_integrity_valid,
    summarize_incomplete_download_materialization_receipt,
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

fn unit(fingerprint: char, destination: &str, range_start: u64) -> IncompleteDownloadMaterializedUnit {
    IncompleteDownloadMaterializedUnit {
        materialization_unit_fingerprint: fingerprint.to_string().repeat(64),
        kind: MaterializationUnitKind::FullZipFile,
        source_relative_path: "downloads/source.crdownload".into(),
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
    let encoded = serde_json::to_vec(&unsigned).unwrap();
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-incomplete-download-materialization-receipt-v1\0");
    hasher.update(&encoded);
    receipt.receipt_id = hasher.finalize().to_hex().to_string();
}

fn valid_receipt() -> IncompleteDownloadMaterializationReceipt {
    let snapshot = capacity_snapshot();
    let units = vec![unit('d', "DiskSage/materialized.zip", 0)];
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
        fresh_capacity: assess_capacity(snapshot, 10, 10, 0),
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

macro_rules! assert_signed_rejected {
    ($case:literal, $mutation:expr) => {{
        let mut receipt = valid_receipt();
        ($mutation)(&mut receipt);
        sign(&mut receipt);
        assert!(
            !incomplete_download_materialization_receipt_integrity_valid(&receipt),
            "tampered receipt was accepted: {}",
            $case
        );
    }};
}

#[test]
fn valid_public_receipt_round_trips_integrity_and_redacted_summary() {
    let receipt = valid_receipt();
    assert!(incomplete_download_materialization_receipt_integrity_valid(&receipt));

    let summary = summarize_incomplete_download_materialization_receipt(&receipt);
    assert_eq!(summary.schema_version, INCOMPLETE_DOWNLOAD_EXECUTION_VERSION);
    assert_eq!(summary.output_mode, "redacted-materialization-receipt-summary");
    assert_eq!(summary.materialized_bytes, 10);
    assert!(summary.all_outputs_verified);
    assert!(!summary.provider_sync_confirmed);
    assert!(!summary.source_eviction_authorized);
    assert!(!summary.source_mutation_performed);
    assert!(!summary.production_time_assigned);
    assert!(!summary.filename_date_used_as_production_time);
    assert!(summary.filesystem_times_used_only_for_source_stability);
    assert!(summary.paths_names_ranges_and_digests_redacted);

    let encoded = serde_json::to_string(&summary).unwrap();
    for secret in [
        "downloads/source.crdownload",
        "DiskSage/materialized.zip",
        receipt.units[0].content_digests.sha256.as_str(),
    ] {
        assert!(!encoded.contains(secret));
    }
}

#[test]
fn receipt_integrity_rejects_overlap_duplicates_and_capacity_forgery() {
    let mut overlapping = valid_receipt();
    overlapping.units.push(unit('2', "DiskSage/materialized-2.zip", 5));
    overlapping.unit_count = overlapping.units.len();
    overlapping.materialized_bytes = 20;
    overlapping.fresh_capacity = assess_capacity(capacity_snapshot(), 20, 10, 0);
    sign(&mut overlapping);
    assert!(!incomplete_download_materialization_receipt_integrity_valid(&overlapping));

    let mut duplicate_destination = valid_receipt();
    duplicate_destination
        .units
        .push(unit('2', "DiskSage/materialized.zip", 10));
    duplicate_destination.unit_count = duplicate_destination.units.len();
    duplicate_destination.materialized_bytes = 20;
    duplicate_destination.fresh_capacity = assess_capacity(capacity_snapshot(), 20, 10, 0);
    sign(&mut duplicate_destination);
    assert!(!incomplete_download_materialization_receipt_integrity_valid(
        &duplicate_destination
    ));

    let mut malformed_digest = valid_receipt();
    malformed_digest.units[0].content_digests.quick_xor_base64 = "not-base64".into();
    sign(&mut malformed_digest);
    assert!(!incomplete_download_materialization_receipt_integrity_valid(
        &malformed_digest
    ));

    let mut forged_capacity = valid_receipt();
    forged_capacity.fresh_capacity.requested_bytes = 9;
    sign(&mut forged_capacity);
    assert!(!incomplete_download_materialization_receipt_integrity_valid(
        &forged_capacity
    ));

    let mut forged_identity = valid_receipt();
    forged_identity.receipt_id = "0".repeat(64);
    assert!(!incomplete_download_materialization_receipt_integrity_valid(
        &forged_identity
    ));
}

#[test]
fn receipt_integrity_rejects_authority_temporal_and_capacity_tampering() {
    assert_signed_rejected!("schema-version", |receipt: &mut IncompleteDownloadMaterializationReceipt| {
        receipt.schema_version += 1;
    });
    assert_signed_rejected!("destination-plan-fingerprint", |receipt: &mut IncompleteDownloadMaterializationReceipt| {
        receipt.destination_plan_fingerprint.clear();
    });
    assert_signed_rejected!("materialization-plan-fingerprint", |receipt: &mut IncompleteDownloadMaterializationReceipt| {
        receipt.materialization_plan_fingerprint = "G".repeat(64);
    });
    assert_signed_rejected!("approval-id", |receipt: &mut IncompleteDownloadMaterializationReceipt| {
        receipt.approval_id = "short".into();
    });
    assert_signed_rejected!("unknown-account-scope", |receipt: &mut IncompleteDownloadMaterializationReceipt| {
        receipt.account_scope = CloudAccountScope::Unknown;
    });
    assert_signed_rejected!("relative-cloud-root", |receipt: &mut IncompleteDownloadMaterializationReceipt| {
        receipt.cloud_root = "relative/cloud".into();
    });
    assert_signed_rejected!("unsafe-destination-subdirectory", |receipt: &mut IncompleteDownloadMaterializationReceipt| {
        receipt.destination_subdirectory = "../escape".into();
    });
    assert_signed_rejected!("zero-source-count", |receipt: &mut IncompleteDownloadMaterializationReceipt| {
        receipt.source_file_count = 0;
    });
    assert_signed_rejected!("unit-count-mismatch", |receipt: &mut IncompleteDownloadMaterializationReceipt| {
        receipt.unit_count = 2;
    });
    assert_signed_rejected!("zero-materialized-bytes", |receipt: &mut IncompleteDownloadMaterializationReceipt| {
        receipt.materialized_bytes = 0;
    });
    assert_signed_rejected!("unverified-outputs", |receipt: &mut IncompleteDownloadMaterializationReceipt| {
        receipt.all_outputs_verified = false;
    });
    assert_signed_rejected!("provider-sync-authority", |receipt: &mut IncompleteDownloadMaterializationReceipt| {
        receipt.provider_sync_confirmed = true;
    });
    assert_signed_rejected!("source-eviction-authority", |receipt: &mut IncompleteDownloadMaterializationReceipt| {
        receipt.source_eviction_authorized = true;
    });
    assert_signed_rejected!("source-mutation", |receipt: &mut IncompleteDownloadMaterializationReceipt| {
        receipt.source_mutation_performed = true;
    });
    assert_signed_rejected!("production-time", |receipt: &mut IncompleteDownloadMaterializationReceipt| {
        receipt.production_time_ms = Some(100);
    });
    assert_signed_rejected!("production-time-source", |receipt: &mut IncompleteDownloadMaterializationReceipt| {
        receipt.production_time_source = Some("filename".into());
    });
    assert_signed_rejected!("capacity-not-fit", |receipt: &mut IncompleteDownloadMaterializationReceipt| {
        receipt.fresh_capacity.can_fit = Some(false);
    });
    assert_signed_rejected!("capacity-blocker", |receipt: &mut IncompleteDownloadMaterializationReceipt| {
        receipt.fresh_capacity.blockers.push("tampered".into());
    });
    assert_signed_rejected!("capacity-provider", |receipt: &mut IncompleteDownloadMaterializationReceipt| {
        receipt.fresh_capacity.snapshot.provider = CloudProvider::GoogleDrive;
    });
    assert_signed_rejected!("capacity-account-scope", |receipt: &mut IncompleteDownloadMaterializationReceipt| {
        receipt.fresh_capacity.snapshot.account_scope = Some(CloudAccountScope::Organization);
    });
    assert_signed_rejected!("capacity-observed-in-future", |receipt: &mut IncompleteDownloadMaterializationReceipt| {
        receipt.fresh_capacity.snapshot.observed_at_ms = receipt.executed_at_ms + 1;
    });
    assert_signed_rejected!("capacity-stale", |receipt: &mut IncompleteDownloadMaterializationReceipt| {
        receipt.executed_at_ms = u64::MAX;
    });
    assert_signed_rejected!("capacity-fingerprint", |receipt: &mut IncompleteDownloadMaterializationReceipt| {
        receipt.fresh_capacity.snapshot.evidence_fingerprint = None;
    });
}

#[test]
fn receipt_integrity_rejects_unit_shape_observation_and_aggregate_tampering() {
    assert_signed_rejected!("unit-fingerprint", |receipt: &mut IncompleteDownloadMaterializationReceipt| {
        receipt.units[0].materialization_unit_fingerprint = "z".repeat(64);
    });
    assert_signed_rejected!("unsafe-source-path", |receipt: &mut IncompleteDownloadMaterializationReceipt| {
        receipt.units[0].source_relative_path = "../outside".into();
    });
    assert_signed_rejected!("unsafe-destination-path", |receipt: &mut IncompleteDownloadMaterializationReceipt| {
        receipt.units[0].destination_relative_path = "DiskSage/../outside".into();
    });
    assert_signed_rejected!("destination-parent", |receipt: &mut IncompleteDownloadMaterializationReceipt| {
        receipt.units[0].destination_relative_path = "Other/materialized.zip".into();
    });
    assert_signed_rejected!("empty-range", |receipt: &mut IncompleteDownloadMaterializationReceipt| {
        receipt.units[0].range_end = receipt.units[0].range_start;
    });
    assert_signed_rejected!("output-byte-count", |receipt: &mut IncompleteDownloadMaterializationReceipt| {
        receipt.units[0].output_bytes = 9;
    });
    assert_signed_rejected!("blake3-digest", |receipt: &mut IncompleteDownloadMaterializationReceipt| {
        receipt.units[0].content_digests.blake3 = "0".repeat(63);
    });
    assert_signed_rejected!("sha256-digest", |receipt: &mut IncompleteDownloadMaterializationReceipt| {
        receipt.units[0].content_digests.sha256 = "X".repeat(64);
    });
    assert_signed_rejected!("source-stability", |receipt: &mut IncompleteDownloadMaterializationReceipt| {
        receipt.units[0].source_stable = false;
    });
    assert_signed_rejected!("output-verification", |receipt: &mut IncompleteDownloadMaterializationReceipt| {
        receipt.units[0].output_verified = false;
    });
    assert_signed_rejected!("write-observation", |receipt: &mut IncompleteDownloadMaterializationReceipt| {
        receipt.units[0].write_performed = false;
    });

    let mut duplicate_fingerprint = valid_receipt();
    duplicate_fingerprint
        .units
        .push(unit('d', "DiskSage/materialized-2.zip", 10));
    duplicate_fingerprint.unit_count = 2;
    duplicate_fingerprint.materialized_bytes = 20;
    duplicate_fingerprint.fresh_capacity = assess_capacity(capacity_snapshot(), 20, 10, 0);
    sign(&mut duplicate_fingerprint);
    assert!(!incomplete_download_materialization_receipt_integrity_valid(
        &duplicate_fingerprint
    ));

    let mut source_count_mismatch = valid_receipt();
    source_count_mismatch.source_file_count = 2;
    sign(&mut source_count_mismatch);
    assert!(!incomplete_download_materialization_receipt_integrity_valid(
        &source_count_mismatch
    ));

    let mut total_mismatch = valid_receipt();
    total_mismatch.materialized_bytes = 11;
    total_mismatch.fresh_capacity = assess_capacity(capacity_snapshot(), 11, 10, 0);
    sign(&mut total_mismatch);
    assert!(!incomplete_download_materialization_receipt_integrity_valid(
        &total_mismatch
    ));
}
