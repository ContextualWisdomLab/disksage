//! Generate a synthetic, path-free v1 materialization-lineage envelope for contract testing.
//!
//! All source, destination, and receipt writes stay inside temporary directories. The JSON emitted
//! to stdout contains only the redacted Naruon contract.

use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::incomplete_download::{
    collect_incomplete_download_audit, DEFAULT_MAX_ENTRIES, DEFAULT_STALE_AFTER_DAYS,
};
use disksage_lib::incomplete_download_materialization::plan_incomplete_download_materialization;
use disksage_lib::incomplete_download_materialization_destination::{
    approve_incomplete_download_destination, plan_incomplete_download_destination,
};
use disksage_lib::incomplete_download_materialization_execution::execute_incomplete_download_materialization;
use disksage_lib::incomplete_download_recovery::{
    validate_incomplete_download_recovery, RecoveryValidationLimits,
};
use disksage_lib::naruon_incomplete_download_lineage::export_naruon_incomplete_download_materialization_lineage;
use disksage_lib::provider_capacity::{parse_icloud_brctl_quota, DEFAULT_CAPACITY_RESERVE_BYTES};
use std::io::Write;

fn main() -> Result<(), String> {
    let source = tempfile::tempdir().map_err(|error| error.to_string())?;
    let cloud = tempfile::tempdir().map_err(|error| error.to_string())?;
    let receipts = tempfile::tempdir().map_err(|error| error.to_string())?;
    let source_path = source.path().join("synthetic.zip.crdownload");
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
            .map_err(|error| error.to_string())?;
        writer
            .write_all(b"synthetic Naruon lineage fixture")
            .map_err(|error| error.to_string())?;
        writer.finish().map_err(|error| error.to_string())?;
    }
    let mut source_file = std::fs::File::create(&source_path).map_err(|error| error.to_string())?;
    source_file
        .write_all(&bytes)
        .and_then(|_| source_file.sync_all())
        .map_err(|error| error.to_string())?;
    drop(source_file);

    let modified = std::fs::metadata(source.path())
        .and_then(|metadata| metadata.modified())
        .and_then(|value| {
            value
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(std::io::Error::other)
        })
        .map_err(|error| error.to_string())?
        .as_millis() as u64;
    let observed_at_ms = modified + 31 * 86_400_000;
    let audit = collect_incomplete_download_audit(
        source.path(),
        observed_at_ms,
        DEFAULT_MAX_ENTRIES,
        DEFAULT_STALE_AFTER_DAYS,
    )?;
    let recovery = validate_incomplete_download_recovery(
        source.path(),
        &audit,
        observed_at_ms + 1,
        RecoveryValidationLimits::default(),
    )?;
    let materialization = plan_incomplete_download_materialization(
        source.path(),
        &audit,
        &recovery,
        observed_at_ms + 2,
    )?;
    let capacity = parse_icloud_brctl_quota(
        "10000000000 bytes of quota remaining in personal account\n",
        observed_at_ms + 3,
    )?;
    let root = CloudRoot {
        id: "icloud:synthetic-contract-fixture".into(),
        provider: CloudProvider::Icloud,
        account_scope: CloudAccountScope::Unknown,
        label: "Synthetic iCloud".into(),
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
    )?;
    let approval = approve_incomplete_download_destination(
        &plan,
        &plan.destination_plan_fingerprint,
        observed_at_ms + 5,
        "human:synthetic-contract-fixture",
        "approved synthetic contract fixture",
    )?;
    let (receipt, _) = execute_incomplete_download_materialization(
        source.path(),
        &materialization,
        &plan,
        &approval,
        &plan.destination_plan_fingerprint,
        capacity,
        receipts.path(),
        observed_at_ms + 6,
    )?;
    let envelope = export_naruon_incomplete_download_materialization_lineage(
        &receipt,
        &materialization,
        &plan,
        &approval,
    )?;
    println!(
        "{}",
        serde_json::to_string_pretty(&envelope).map_err(|error| error.to_string())?
    );
    Ok(())
}
