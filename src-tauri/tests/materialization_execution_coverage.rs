//! Exact-coverage regression for materialization authority and receipt integrity.
//!
//! These tests exercise the shipped public API with a real ZIP fixture and temporary local
//! directories. They deliberately tamper one receipt authority/content invariant at a time so the
//! fail-closed integrity contract is measured through public behavior rather than private helpers.

use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::incomplete_download::{
    collect_incomplete_download_audit, DEFAULT_MAX_ENTRIES, DEFAULT_STALE_AFTER_DAYS,
};
use disksage_lib::incomplete_download_materialization::plan_incomplete_download_materialization;
use disksage_lib::incomplete_download_materialization_destination::{
    approve_incomplete_download_destination, plan_incomplete_download_destination,
};
use disksage_lib::incomplete_download_materialization_execution::{
    execute_incomplete_download_materialization,
    incomplete_download_materialization_receipt_integrity_valid,
    IncompleteDownloadMaterializationReceipt, INCOMPLETE_DOWNLOAD_EXECUTION_VERSION,
};
use disksage_lib::incomplete_download_recovery::{
    validate_incomplete_download_recovery, RecoveryValidationLimits,
};
use disksage_lib::provider_capacity::{
    parse_icloud_brctl_quota, DEFAULT_CAPACITY_RESERVE_BYTES,
};
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

fn write_zip(path: &Path, payload: &[u8]) {
    let bytes = zip_bytes(payload);
    let mut file = File::create(path).unwrap();
    file.write_all(&bytes).unwrap();
    file.sync_all().unwrap();
}

fn valid_receipt() -> IncompleteDownloadMaterializationReceipt {
    let source = tempfile::tempdir().unwrap();
    let cloud = tempfile::tempdir().unwrap();
    let receipts = tempfile::tempdir().unwrap();
    write_zip(
        &source.path().join("whole.zip.crdownload"),
        b"coverage authority payload",
    );

    let created = std::fs::metadata(source.path()).unwrap().modified().unwrap();
    let observed_at_ms = created
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
        id: "icloud:coverage".into(),
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
        "human:coverage",
        "approved exact public coverage materialization",
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
    assert!(receipt_path.is_file());
    receipt
}

#[test]
fn receipt_integrity_rejects_tampered_authority_and_content_fields() {
    let receipt = valid_receipt();
    assert!(incomplete_download_materialization_receipt_integrity_valid(
        &receipt
    ));

    macro_rules! rejects {
        ($label:literal, $mutation:expr) => {{
            let mut tampered = receipt.clone();
            ($mutation)(&mut tampered);
            assert!(
                !incomplete_download_materialization_receipt_integrity_valid(&tampered),
                "tampered receipt must fail closed: {}",
                $label
            );
        }};
    }

    rejects!("schema version", |value: &mut IncompleteDownloadMaterializationReceipt| {
        value.schema_version = INCOMPLETE_DOWNLOAD_EXECUTION_VERSION + 1;
    });
    rejects!("invalid receipt id alphabet", |value: &mut IncompleteDownloadMaterializationReceipt| {
        value.receipt_id = "G".repeat(64);
    });
    rejects!("unknown account scope", |value: &mut IncompleteDownloadMaterializationReceipt| {
        value.account_scope = CloudAccountScope::Unknown;
    });
    rejects!("relative cloud root", |value: &mut IncompleteDownloadMaterializationReceipt| {
        value.cloud_root = "relative-cloud-root".into();
    });
    rejects!("unsafe destination directory", |value: &mut IncompleteDownloadMaterializationReceipt| {
        value.destination_subdirectory = "../Recovered".into();
    });
    rejects!("zero source count", |value: &mut IncompleteDownloadMaterializationReceipt| {
        value.source_file_count = 0;
    });
    rejects!("unit count mismatch", |value: &mut IncompleteDownloadMaterializationReceipt| {
        value.unit_count += 1;
    });
    rejects!("empty units", |value: &mut IncompleteDownloadMaterializationReceipt| {
        value.unit_count = 0;
        value.units.clear();
    });
    rejects!("zero materialized bytes", |value: &mut IncompleteDownloadMaterializationReceipt| {
        value.materialized_bytes = 0;
    });
    rejects!("unverified outputs", |value: &mut IncompleteDownloadMaterializationReceipt| {
        value.all_outputs_verified = false;
    });
    rejects!("false provider sync authority", |value: &mut IncompleteDownloadMaterializationReceipt| {
        value.provider_sync_confirmed = true;
    });
    rejects!("false source eviction authority", |value: &mut IncompleteDownloadMaterializationReceipt| {
        value.source_eviction_authorized = true;
    });
    rejects!("false source mutation evidence", |value: &mut IncompleteDownloadMaterializationReceipt| {
        value.source_mutation_performed = true;
    });
    rejects!("invented production time", |value: &mut IncompleteDownloadMaterializationReceipt| {
        value.production_time_ms = Some(value.executed_at_ms);
    });
    rejects!("invented production time source", |value: &mut IncompleteDownloadMaterializationReceipt| {
        value.production_time_source = Some("filesystem-mtime".into());
    });
    rejects!("capacity not admitted", |value: &mut IncompleteDownloadMaterializationReceipt| {
        value.fresh_capacity.can_fit = Some(false);
    });
    rejects!("capacity blocker present", |value: &mut IncompleteDownloadMaterializationReceipt| {
        value.fresh_capacity.blockers.push("quota-blocked".into());
    });
    rejects!("capacity provider mismatch", |value: &mut IncompleteDownloadMaterializationReceipt| {
        value.fresh_capacity.snapshot.provider = CloudProvider::Onedrive;
    });
    rejects!("capacity account mismatch", |value: &mut IncompleteDownloadMaterializationReceipt| {
        value.fresh_capacity.snapshot.account_scope = Some(CloudAccountScope::Organization);
    });
    rejects!("capacity observed in future", |value: &mut IncompleteDownloadMaterializationReceipt| {
        value.fresh_capacity.snapshot.observed_at_ms = value.executed_at_ms + 1;
    });
    rejects!("invalid capacity fingerprint", |value: &mut IncompleteDownloadMaterializationReceipt| {
        value.fresh_capacity.snapshot.evidence_fingerprint = Some("bad".into());
    });
    rejects!("capacity requested bytes mismatch", |value: &mut IncompleteDownloadMaterializationReceipt| {
        value.fresh_capacity.requested_bytes += 1;
    });

    rejects!("invalid unit fingerprint", |value: &mut IncompleteDownloadMaterializationReceipt| {
        value.units[0].materialization_unit_fingerprint = "x".repeat(64);
    });
    rejects!("unsafe source relative path", |value: &mut IncompleteDownloadMaterializationReceipt| {
        value.units[0].source_relative_path = "../whole.zip.crdownload".into();
    });
    rejects!("unsafe destination relative path", |value: &mut IncompleteDownloadMaterializationReceipt| {
        value.units[0].destination_relative_path = "../whole.zip".into();
    });
    rejects!("wrong destination parent", |value: &mut IncompleteDownloadMaterializationReceipt| {
        value.units[0].destination_relative_path = "Other/whole.zip".into();
    });
    rejects!("invalid range order", |value: &mut IncompleteDownloadMaterializationReceipt| {
        value.units[0].range_end = value.units[0].range_start;
    });
    rejects!("range byte mismatch", |value: &mut IncompleteDownloadMaterializationReceipt| {
        value.units[0].output_bytes += 1;
    });
    rejects!("invalid blake3", |value: &mut IncompleteDownloadMaterializationReceipt| {
        value.units[0].content_digests.blake3 = "z".repeat(64);
    });
    rejects!("invalid sha256", |value: &mut IncompleteDownloadMaterializationReceipt| {
        value.units[0].content_digests.sha256 = "z".repeat(64);
    });
    rejects!("invalid quick xor", |value: &mut IncompleteDownloadMaterializationReceipt| {
        value.units[0].content_digests.quick_xor_base64 = "!".repeat(28);
    });
    rejects!("unstable source", |value: &mut IncompleteDownloadMaterializationReceipt| {
        value.units[0].source_stable = false;
    });
    rejects!("output not verified", |value: &mut IncompleteDownloadMaterializationReceipt| {
        value.units[0].output_verified = false;
    });
    rejects!("write not performed", |value: &mut IncompleteDownloadMaterializationReceipt| {
        value.units[0].write_performed = false;
    });
    rejects!("valid-looking but wrong receipt id", |value: &mut IncompleteDownloadMaterializationReceipt| {
        value.receipt_id = "0".repeat(64);
    });
}
