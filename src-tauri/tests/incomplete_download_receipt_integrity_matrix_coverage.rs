//! Coverage-visible receipt-integrity matrix for approved incomplete-download materialization.
//!
//! The fixture crosses the real audit -> recovery -> materialization -> destination -> approval ->
//! execution path using only temporary directories. Production writes are restricted to the temp
//! destination and temp immutable-receipt directory; the source is verified unchanged.

use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::incomplete_download::{
    collect_incomplete_download_audit, DEFAULT_MAX_ENTRIES, DEFAULT_STALE_AFTER_DAYS,
};
use disksage_lib::incomplete_download_materialization::plan_incomplete_download_materialization;
use disksage_lib::incomplete_download_materialization_destination::{
    approve_incomplete_download_destination, plan_incomplete_download_destination,
    MAX_CAPACITY_AGE_MS,
};
use disksage_lib::incomplete_download_materialization_execution::{
    execute_incomplete_download_materialization,
    incomplete_download_materialization_receipt_integrity_valid,
    summarize_incomplete_download_materialization_receipt,
};
use disksage_lib::incomplete_download_recovery::{
    validate_incomplete_download_recovery, RecoveryValidationLimits,
};
use disksage_lib::provider_capacity::{parse_icloud_brctl_quota, DEFAULT_CAPACITY_RESERVE_BYTES};
use std::fs::File;
use std::io::Write;
use std::path::Path;

fn zip_bytes(payload: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let cursor = std::io::Cursor::new(&mut bytes);
        let mut writer = zip::ZipWriter::new(cursor);
        writer
            .start_file(
                "payload.bin",
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(payload).unwrap();
        writer.finish().unwrap();
    }
    bytes
}

fn write_zip(path: &Path, payload: &[u8]) -> Vec<u8> {
    let bytes = zip_bytes(payload);
    let mut file = File::create(path).unwrap();
    file.write_all(&bytes).unwrap();
    file.sync_all().unwrap();
    drop(file);
    bytes
}

fn successful_execution() -> (
    tempfile::TempDir,
    tempfile::TempDir,
    tempfile::TempDir,
    Vec<u8>,
    disksage_lib::incomplete_download_materialization_execution::IncompleteDownloadMaterializationReceipt,
    std::path::PathBuf,
) {
    let source = tempfile::tempdir().unwrap();
    let cloud = tempfile::tempdir().unwrap();
    let receipts = tempfile::tempdir().unwrap();
    let source_path = source.path().join("whole.zip.crdownload");
    let original = write_zip(&source_path, b"coverage receipt payload");

    let modified = std::fs::metadata(source.path()).unwrap().modified().unwrap();
    let observed_at_ms = modified
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
        + 31 * 86_400_000;
    let audit = collect_incomplete_download_audit(
        source.path(),
        observed_at_ms,
        DEFAULT_MAX_ENTRIES,
        DEFAULT_STALE_AFTER_DAYS,
    )
    .unwrap();
    let recovery = validate_incomplete_download_recovery(
        source.path(),
        &audit,
        observed_at_ms + 1,
        RecoveryValidationLimits::default(),
    )
    .unwrap();
    let materialization = plan_incomplete_download_materialization(
        source.path(),
        &audit,
        &recovery,
        observed_at_ms + 2,
    )
    .unwrap();
    let capacity = parse_icloud_brctl_quota(
        "10000000000 bytes of quota remaining in personal account\n",
        observed_at_ms + 3,
    )
    .unwrap();
    let root = CloudRoot {
        id: "icloud:receipt-integrity-coverage".into(),
        provider: CloudProvider::Icloud,
        account_scope: CloudAccountScope::Unknown,
        label: "iCloud".into(),
        path: cloud.path().to_string_lossy().into_owned(),
        readable: true,
        access_issue: None,
    };
    let plan = plan_incomplete_download_destination(
        &materialization,
        &root,
        "DiskSage/Recovered",
        capacity.clone(),
        DEFAULT_CAPACITY_RESERVE_BYTES,
        observed_at_ms + 4,
    )
    .unwrap();
    let approval = approve_incomplete_download_destination(
        &plan,
        &plan.destination_plan_fingerprint,
        observed_at_ms + 5,
        "human:coverage-reviewer",
        "verified exact temporary destination and capacity evidence",
    )
    .unwrap();
    let (receipt, receipt_path) = execute_incomplete_download_materialization(
        source.path(),
        &materialization,
        &plan,
        &approval,
        &plan.destination_plan_fingerprint,
        capacity,
        receipts.path(),
        approval.approved_at_ms + 1,
    )
    .unwrap();

    assert_eq!(std::fs::read(&source_path).unwrap(), original);
    (source, cloud, receipts, original, receipt, receipt_path)
}

#[test]
fn real_execution_publishes_integrity_checked_redacted_receipt() {
    let (_source, cloud, _receipts, original, receipt, receipt_path) = successful_execution();
    assert!(incomplete_download_materialization_receipt_integrity_valid(
        &receipt
    ));
    assert!(receipt_path.is_file());
    assert_eq!(receipt.unit_count, 1);
    assert_eq!(receipt.materialized_bytes, original.len() as u64);
    assert_eq!(
        std::fs::read(
            Path::new(&receipt.cloud_root).join(&receipt.units[0].destination_relative_path)
        )
        .unwrap(),
        original
    );

    let summary = summarize_incomplete_download_materialization_receipt(&receipt);
    let encoded = serde_json::to_string(&summary).unwrap();
    assert!(summary.paths_names_ranges_and_digests_redacted);
    assert!(!summary.production_time_assigned);
    assert!(!summary.filename_date_used_as_production_time);
    assert!(!encoded.contains("whole.zip.crdownload"));
    assert!(!encoded.contains(cloud.path().to_string_lossy().as_ref()));
    assert!(!encoded.contains(&receipt.units[0].content_digests.sha256));
}

#[test]
fn receipt_integrity_rejects_top_level_authority_capacity_and_state_drift() {
    let (_source, _cloud, _receipts, _original, receipt, _receipt_path) = successful_execution();
    let mut cases = Vec::new();

    let mut mutated = receipt.clone();
    mutated.schema_version = 0;
    cases.push(mutated);

    let mut mutated = receipt.clone();
    mutated.receipt_id = "invalid".into();
    cases.push(mutated);

    let mut mutated = receipt.clone();
    mutated.destination_plan_fingerprint = "invalid".into();
    cases.push(mutated);

    let mut mutated = receipt.clone();
    mutated.account_scope = CloudAccountScope::Unknown;
    cases.push(mutated);

    let mut mutated = receipt.clone();
    mutated.cloud_root = "relative/cloud".into();
    cases.push(mutated);

    let mut mutated = receipt.clone();
    mutated.destination_subdirectory = "../Recovered".into();
    cases.push(mutated);

    let mut mutated = receipt.clone();
    mutated.source_file_count = 0;
    cases.push(mutated);

    let mut mutated = receipt.clone();
    mutated.unit_count += 1;
    cases.push(mutated);

    let mut mutated = receipt.clone();
    mutated.units.clear();
    mutated.unit_count = 0;
    cases.push(mutated);

    let mut mutated = receipt.clone();
    mutated.materialized_bytes = 0;
    cases.push(mutated);

    let mut mutated = receipt.clone();
    mutated.all_outputs_verified = false;
    cases.push(mutated);

    let mut mutated = receipt.clone();
    mutated.provider_sync_confirmed = true;
    cases.push(mutated);

    let mut mutated = receipt.clone();
    mutated.source_eviction_authorized = true;
    cases.push(mutated);

    let mut mutated = receipt.clone();
    mutated.source_mutation_performed = true;
    cases.push(mutated);

    let mut mutated = receipt.clone();
    mutated.production_time_ms = Some(receipt.executed_at_ms);
    cases.push(mutated);

    let mut mutated = receipt.clone();
    mutated.production_time_source = Some("filename".into());
    cases.push(mutated);

    let mut mutated = receipt.clone();
    mutated.fresh_capacity.can_fit = None;
    cases.push(mutated);

    let mut mutated = receipt.clone();
    mutated.fresh_capacity.blockers.push("coverage-blocker".into());
    cases.push(mutated);

    let mut mutated = receipt.clone();
    mutated.fresh_capacity.snapshot.provider = CloudProvider::GoogleDrive;
    cases.push(mutated);

    let mut mutated = receipt.clone();
    mutated.fresh_capacity.snapshot.account_scope = Some(CloudAccountScope::Organization);
    cases.push(mutated);

    let mut mutated = receipt.clone();
    mutated.fresh_capacity.snapshot.observed_at_ms = receipt.executed_at_ms + 1;
    cases.push(mutated);

    let mut mutated = receipt.clone();
    mutated.executed_at_ms = receipt
        .fresh_capacity
        .snapshot
        .observed_at_ms
        .saturating_add(MAX_CAPACITY_AGE_MS + 1);
    cases.push(mutated);

    let mut mutated = receipt.clone();
    mutated.fresh_capacity.snapshot.evidence_fingerprint = None;
    cases.push(mutated);

    let mut mutated = receipt.clone();
    mutated.fresh_capacity.requested_bytes += 1;
    cases.push(mutated);

    for mutated in cases {
        assert!(!incomplete_download_materialization_receipt_integrity_valid(
            &mutated
        ));
    }
}

#[test]
fn receipt_integrity_rejects_unit_range_digest_and_capacity_recomputation_drift() {
    let (_source, _cloud, _receipts, _original, receipt, _receipt_path) = successful_execution();

    let mut mutated = receipt.clone();
    mutated.units[0].materialization_unit_fingerprint = "invalid".into();
    assert!(!incomplete_download_materialization_receipt_integrity_valid(&mutated));

    let mut mutated = receipt.clone();
    mutated.units[0].source_relative_path = "../source.zip".into();
    assert!(!incomplete_download_materialization_receipt_integrity_valid(&mutated));

    let mut mutated = receipt.clone();
    mutated.units[0].destination_relative_path = "/absolute/output.zip".into();
    assert!(!incomplete_download_materialization_receipt_integrity_valid(&mutated));

    let mut mutated = receipt.clone();
    mutated.units[0].destination_relative_path = "Other/output.zip".into();
    assert!(!incomplete_download_materialization_receipt_integrity_valid(&mutated));

    let mut mutated = receipt.clone();
    mutated.units[0].range_end = mutated.units[0].range_start;
    assert!(!incomplete_download_materialization_receipt_integrity_valid(&mutated));

    let mut mutated = receipt.clone();
    mutated.units[0].output_bytes += 1;
    assert!(!incomplete_download_materialization_receipt_integrity_valid(&mutated));

    let mut mutated = receipt.clone();
    mutated.units[0].content_digests.blake3 = "invalid".into();
    assert!(!incomplete_download_materialization_receipt_integrity_valid(&mutated));

    let mut mutated = receipt.clone();
    mutated.units[0].content_digests.sha256 = "invalid".into();
    assert!(!incomplete_download_materialization_receipt_integrity_valid(&mutated));

    let mut mutated = receipt.clone();
    mutated.units[0].content_digests.quick_xor_base64 = "invalid".into();
    assert!(!incomplete_download_materialization_receipt_integrity_valid(&mutated));

    let mut mutated = receipt.clone();
    mutated.units[0].source_stable = false;
    assert!(!incomplete_download_materialization_receipt_integrity_valid(&mutated));

    let mut mutated = receipt.clone();
    mutated.units[0].output_verified = false;
    assert!(!incomplete_download_materialization_receipt_integrity_valid(&mutated));

    let mut mutated = receipt.clone();
    mutated.units[0].write_performed = false;
    assert!(!incomplete_download_materialization_receipt_integrity_valid(&mutated));

    let mut mutated = receipt.clone();
    mutated.fresh_capacity.largest_candidate_bytes += 1;
    assert!(!incomplete_download_materialization_receipt_integrity_valid(&mutated));

    let mut mutated = receipt.clone();
    mutated.fresh_capacity.reserve_bytes += 1;
    assert!(!incomplete_download_materialization_receipt_integrity_valid(&mutated));

    let mut mutated = receipt.clone();
    mutated.receipt_id = "f".repeat(64);
    assert!(!incomplete_download_materialization_receipt_integrity_valid(&mutated));
}
