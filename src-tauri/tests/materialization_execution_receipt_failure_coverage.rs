//! Real-filesystem coverage for materialization receipt publication failures.
//!
//! A materialization can copy and verify every planned byte and still fail while publishing its
//! immutable receipt. These regressions exercise that late failure boundary through the public API
//! and prove rollback preserves the source and removes newly-created output. The fixtures use a
//! real ZIP, real temporary directories, exact planning lineage, explicit human approval, and fresh
//! provider capacity evidence; no network or provider mutation is involved.

use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::cloud_local_eviction::observe_path_active_use;
use disksage_lib::incomplete_download::{
    collect_incomplete_download_audit, DEFAULT_MAX_ENTRIES, DEFAULT_STALE_AFTER_DAYS,
};
use disksage_lib::incomplete_download_materialization::{
    plan_incomplete_download_materialization, IncompleteDownloadMaterializationReport,
};
use disksage_lib::incomplete_download_materialization_destination::{
    approve_incomplete_download_destination, plan_incomplete_download_destination,
    IncompleteDownloadDestinationApproval, IncompleteDownloadDestinationPlan,
};
use disksage_lib::incomplete_download_materialization_execution::execute_incomplete_download_materialization;
use disksage_lib::incomplete_download_recovery::{
    validate_incomplete_download_recovery, RecoveryValidationLimits,
};
use disksage_lib::provider_capacity::{
    parse_icloud_brctl_quota, CloudCapacitySnapshot, DEFAULT_CAPACITY_RESERVE_BYTES,
};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tempfile::TempDir;

struct Fixture {
    source: TempDir,
    cloud: TempDir,
    materialization: IncompleteDownloadMaterializationReport,
    plan: IncompleteDownloadDestinationPlan,
    approval: IncompleteDownloadDestinationApproval,
    capacity: CloudCapacitySnapshot,
    executed_at_ms: u64,
}

impl Fixture {
    fn source_file(&self) -> PathBuf {
        self.source
            .path()
            .join(&self.materialization.units[0].source_relative_path)
    }

    fn output_file(&self) -> PathBuf {
        self.cloud
            .path()
            .join(&self.plan.units[0].destination_relative_path)
    }

    fn execute(&self, receipt_dir: &Path) -> Result<
        (
            disksage_lib::incomplete_download_materialization_execution::IncompleteDownloadMaterializationReceipt,
            PathBuf,
        ),
        String,
    > {
        execute_incomplete_download_materialization(
            self.source.path(),
            &self.materialization,
            &self.plan,
            &self.approval,
            &self.plan.destination_plan_fingerprint,
            self.capacity.clone(),
            receipt_dir,
            self.executed_at_ms,
        )
    }
}

fn write_zip(path: &Path, payload: &[u8]) {
    let file = File::create(path).unwrap();
    let mut writer = zip::ZipWriter::new(file);
    writer
        .start_file(
            "payload.bin",
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored),
        )
        .unwrap();
    writer.write_all(payload).unwrap();
    writer.finish().unwrap().sync_all().unwrap();
}

fn fixture() -> Fixture {
    let source = tempfile::tempdir().unwrap();
    let cloud = tempfile::tempdir().unwrap();
    let source_file = source.path().join("whole.zip.crdownload");
    write_zip(
        &source_file,
        b"immutable receipt rollback coverage payload",
    );
    wait_for_inactive_source(&source_file);

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
        id: "icloud:receipt-failure-coverage".into(),
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
        "human:receipt-failure-coverage",
        "reviewed exact destination before local materialization",
    )
    .unwrap();

    Fixture {
        source,
        cloud,
        materialization,
        plan,
        approval,
        capacity,
        executed_at_ms: observed_at_ms + 6,
    }
}

fn assert_source_preserved_and_output_rolled_back(fixture: &Fixture) {
    assert!(fixture.source_file().is_file());
    assert!(!fixture.output_file().exists());
}

fn wait_for_inactive_source(path: &Path) {
    for _ in 0..80 {
        let evidence = observe_path_active_use(path);
        if evidence.evidence_complete && !evidence.active {
            return;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    let evidence = observe_path_active_use(path);
    panic!(
        "source active-use evidence did not stabilize: complete={}, active={}, error={:?}",
        evidence.evidence_complete, evidence.active, evidence.error
    );
}

#[test]
fn receipt_directory_rejects_relative_and_data_overlap_with_output_rollback() {
    let fixture = fixture();

    assert_eq!(
        fixture.execute(Path::new("relative-receipts")).unwrap_err(),
        "materialization-execution-receipt-directory-unsafe"
    );
    assert_source_preserved_and_output_rolled_back(&fixture);

    assert_eq!(
        fixture.execute(fixture.source.path()).unwrap_err(),
        "materialization-execution-receipt-directory-overlaps-data"
    );
    assert_source_preserved_and_output_rolled_back(&fixture);

    assert_eq!(
        fixture.execute(fixture.cloud.path()).unwrap_err(),
        "materialization-execution-receipt-directory-overlaps-data"
    );
    assert_source_preserved_and_output_rolled_back(&fixture);
}

#[test]
fn receipt_directory_rejects_non_directory_and_symlink_with_output_rollback() {
    let fixture = fixture();
    let evidence_parent = tempfile::tempdir().unwrap();
    let receipt_file = evidence_parent.path().join("receipt-file");
    std::fs::write(&receipt_file, b"not a directory").unwrap();

    assert_eq!(
        fixture.execute(&receipt_file).unwrap_err(),
        "materialization-execution-receipt-directory-unsafe"
    );
    assert_eq!(std::fs::read(&receipt_file).unwrap(), b"not a directory");
    assert_source_preserved_and_output_rolled_back(&fixture);

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;

        let real_receipts = evidence_parent.path().join("real-receipts");
        let linked_receipts = evidence_parent.path().join("linked-receipts");
        std::fs::create_dir(&real_receipts).unwrap();
        symlink(&real_receipts, &linked_receipts).unwrap();
        assert_eq!(
            fixture.execute(&linked_receipts).unwrap_err(),
            "materialization-execution-receipt-directory-unsafe"
        );
        assert!(std::fs::read_dir(&real_receipts).unwrap().next().is_none());
        assert_source_preserved_and_output_rolled_back(&fixture);
    }
}

#[test]
fn receipt_create_new_collision_rolls_back_recreated_output_and_preserves_first_receipt() {
    let fixture = fixture();
    let evidence_parent = tempfile::tempdir().unwrap();
    let receipt_dir = evidence_parent.path().join("receipts");

    let (first_receipt, first_receipt_path) = fixture.execute(&receipt_dir).unwrap();
    assert!(receipt_dir.is_dir());
    assert!(first_receipt_path.is_file());
    assert!(fixture.output_file().is_file());

    std::fs::remove_file(fixture.output_file()).unwrap();
    wait_for_inactive_source(&fixture.source_file());
    let second_error = fixture.execute(&receipt_dir).unwrap_err();
    assert_eq!(
        second_error,
        "materialization-execution-receipt-create-failed"
    );

    assert!(first_receipt_path.is_file());
    let persisted: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&first_receipt_path).unwrap()).unwrap();
    assert_eq!(
        persisted["receipt_id"].as_str(),
        Some(first_receipt.receipt_id.as_str())
    );
    assert_source_preserved_and_output_rolled_back(&fixture);
}
