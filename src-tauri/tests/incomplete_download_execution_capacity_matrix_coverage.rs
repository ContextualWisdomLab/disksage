//! Coverage-visible execution preflight matrix for incomplete-download materialization.
//!
//! Every case fails before source bytes are copied. The tests use only temporary roots and prove
//! that stale or mismatched capacity evidence and destination-prefix replacement remain fail closed.

use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::content_digest::ContentDigests;
use disksage_lib::incomplete_download_materialization::{
    IncompleteDownloadMaterializationReport, IncompleteDownloadMaterializationUnit,
    MaterializationUnitKind, INCOMPLETE_DOWNLOAD_MATERIALIZATION_VERSION,
};
use disksage_lib::incomplete_download_materialization_destination::{
    approve_incomplete_download_destination, plan_incomplete_download_destination,
    MAX_CAPACITY_AGE_MS,
};
use disksage_lib::incomplete_download_materialization_execution::execute_incomplete_download_materialization;
use disksage_lib::provider_capacity::{parse_icloud_brctl_quota, CloudCapacitySnapshot};
use std::path::Path;

fn unit_fingerprint(
    candidate_fingerprint: &str,
    source_relative_path: &str,
    source_logical_bytes: u64,
    created_ms: u64,
    modified_ms: u64,
    range_start: u64,
    range_end: u64,
    digests: &ContentDigests,
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-incomplete-download-materialization-unit-v1\0");
    for value in [
        candidate_fingerprint,
        source_relative_path,
        "full-zip-file",
        digests.blake3.as_str(),
        digests.sha256.as_str(),
        digests.quick_xor_base64.as_str(),
    ] {
        hasher.update(value.as_bytes());
        hasher.update(&[0]);
    }
    for value in [
        source_logical_bytes,
        created_ms,
        modified_ms,
        range_start,
        range_end,
    ] {
        hasher.update(&value.to_le_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

fn plan_fingerprint(
    source_scope_fingerprint: &str,
    audit_fingerprint: &str,
    validation_fingerprint: &str,
    unit_fingerprint: &str,
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-incomplete-download-materialization-plan-v1\0");
    for value in [
        source_scope_fingerprint,
        audit_fingerprint,
        validation_fingerprint,
    ] {
        hasher.update(value.as_bytes());
        hasher.update(&[0]);
    }
    hasher.update(&[1]);
    hasher.update(unit_fingerprint.as_bytes());
    hasher.update(&[0]);
    hasher.finalize().to_hex().to_string()
}

fn materialization(source_root: &Path) -> IncompleteDownloadMaterializationReport {
    let source_scope_fingerprint = "1".repeat(64);
    let audit_fingerprint = "2".repeat(64);
    let validation_fingerprint = "3".repeat(64);
    let candidate_fingerprint = "4".repeat(64);
    let source_relative_path = "downloads/source.zip.crdownload";
    let digests = ContentDigests {
        blake3: "5".repeat(64),
        sha256: "6".repeat(64),
        quick_xor_base64: "A".repeat(28),
    };
    let fingerprint = unit_fingerprint(
        &candidate_fingerprint,
        source_relative_path,
        10,
        10,
        11,
        0,
        10,
        &digests,
    );
    let unit = IncompleteDownloadMaterializationUnit {
        candidate_fingerprint,
        source_relative_path: source_relative_path.into(),
        source_logical_bytes: 10,
        source_filesystem_created_ms: 10,
        source_filesystem_modified_ms: 11,
        kind: MaterializationUnitKind::FullZipFile,
        range_start: 0,
        range_end: 10,
        output_bytes: 10,
        output_mime_type: "application/zip".into(),
        output_extension: "zip".into(),
        content_digests: digests.clone(),
        unit_fingerprint: fingerprint.clone(),
        suggested_filename: format!(
            "recovered-{}-{}.zip",
            &digests.sha256[..12],
            &fingerprint[..12]
        ),
        active_use_evidence_complete: true,
        source_active: false,
        source_stable: true,
        destination_selected: false,
        requires_human_destination_review: true,
        approval_issued: false,
        write_performed: false,
    };
    IncompleteDownloadMaterializationReport {
        schema_version: INCOMPLETE_DOWNLOAD_MATERIALIZATION_VERSION,
        observed_at_ms: 900,
        source_root: source_root.to_string_lossy().into_owned(),
        source_scope_fingerprint: source_scope_fingerprint.clone(),
        audit_fingerprint: audit_fingerprint.clone(),
        validation_fingerprint: validation_fingerprint.clone(),
        evidence_complete: true,
        source_file_count: 1,
        unit_count: 1,
        full_file_unit_count: 1,
        embedded_zip_range_unit_count: 0,
        planned_output_bytes: 10,
        plan_fingerprint: plan_fingerprint(
            &source_scope_fingerprint,
            &audit_fingerprint,
            &validation_fingerprint,
            &fingerprint,
        ),
        destination_selected: false,
        requires_human_destination_review: true,
        exact_materialization_approval_available: false,
        approval_issued: false,
        mutation_performed: false,
        units: vec![unit],
    }
}

fn root(path: &Path) -> CloudRoot {
    CloudRoot {
        id: "icloud:execution-capacity-coverage".into(),
        provider: CloudProvider::Icloud,
        account_scope: CloudAccountScope::Unknown,
        label: "iCloud".into(),
        path: path.to_string_lossy().into_owned(),
        readable: true,
        access_issue: None,
    }
}

fn capacity(observed_at_ms: u64) -> CloudCapacitySnapshot {
    parse_icloud_brctl_quota(
        "10000000000 bytes of quota remaining in personal account\n",
        observed_at_ms,
    )
    .unwrap()
}

fn context(
    source_root: &Path,
    cloud_root: &Path,
    observed_at_ms: u64,
) -> (
    IncompleteDownloadMaterializationReport,
    disksage_lib::incomplete_download_materialization_destination::IncompleteDownloadDestinationPlan,
    disksage_lib::incomplete_download_materialization_destination::IncompleteDownloadDestinationApproval,
) {
    let materialization = materialization(source_root);
    let plan = plan_incomplete_download_destination(
        &materialization,
        &root(cloud_root),
        "Recovered",
        capacity(observed_at_ms - 1),
        0,
        observed_at_ms,
    )
    .unwrap();
    let approval = approve_incomplete_download_destination(
        &plan,
        &plan.destination_plan_fingerprint,
        observed_at_ms + 1,
        "human:coverage-reviewer",
        "verified exact destination and capacity",
    )
    .unwrap();
    (materialization, plan, approval)
}

#[test]
fn execution_rejects_capacity_schema_scope_time_and_size_drift_before_source_access() {
    let source = tempfile::tempdir().unwrap();
    let cloud = tempfile::tempdir().unwrap();
    let receipts = tempfile::tempdir().unwrap();
    let (materialization, plan, approval) = context(source.path(), cloud.path(), 10_000);
    let receipt_dir = receipts.path().join("receipts");

    let mut wrong_schema = capacity(10_001);
    wrong_schema.schema_version = 0;
    let mut wrong_scope = capacity(10_001);
    wrong_scope.account_scope = Some(CloudAccountScope::Organization);
    let mut zero_observed = capacity(10_001);
    zero_observed.observed_at_ms = 0;
    let future_observed = capacity(10_003);

    for snapshot in [wrong_schema, wrong_scope, zero_observed, future_observed] {
        assert_eq!(
            execute_incomplete_download_materialization(
                source.path(),
                &materialization,
                &plan,
                &approval,
                &plan.destination_plan_fingerprint,
                snapshot,
                &receipt_dir,
                10_002,
            )
            .unwrap_err(),
            "materialization-execution-capacity-evidence-invalid"
        );
    }

    let stale_observed_at = 10_002u64.saturating_sub(MAX_CAPACITY_AGE_MS + 1);
    assert_eq!(
        execute_incomplete_download_materialization(
            source.path(),
            &materialization,
            &plan,
            &approval,
            &plan.destination_plan_fingerprint,
            capacity(stale_observed_at),
            &receipt_dir,
            10_002,
        )
        .unwrap_err(),
        "materialization-execution-capacity-evidence-invalid"
    );

    let mut insufficient = capacity(10_001);
    insufficient.remaining_bytes = Some(1);
    assert_eq!(
        execute_incomplete_download_materialization(
            source.path(),
            &materialization,
            &plan,
            &approval,
            &plan.destination_plan_fingerprint,
            insufficient,
            &receipt_dir,
            10_002,
        )
        .unwrap_err(),
        "materialization-execution-capacity-insufficient"
    );

    assert!(!receipt_dir.exists());
    assert!(!cloud.path().join("Recovered").exists());
}

#[test]
fn execution_rechecks_destination_prefix_type_after_human_approval() {
    let source = tempfile::tempdir().unwrap();
    let cloud = tempfile::tempdir().unwrap();
    let receipts = tempfile::tempdir().unwrap();
    let (materialization, plan, approval) = context(source.path(), cloud.path(), 20_000);
    let receipt_dir = receipts.path().join("receipts");

    std::fs::write(cloud.path().join("Recovered"), b"replacement file").unwrap();
    assert_eq!(
        execute_incomplete_download_materialization(
            source.path(),
            &materialization,
            &plan,
            &approval,
            &plan.destination_plan_fingerprint,
            capacity(20_001),
            &receipt_dir,
            20_002,
        )
        .unwrap_err(),
        "materialization-execution-destination-parent-not-directory"
    );
    assert_eq!(
        std::fs::read(cloud.path().join("Recovered")).unwrap(),
        b"replacement file"
    );
    assert!(!receipt_dir.exists());
}

#[cfg(unix)]
#[test]
fn execution_rechecks_destination_prefix_symlinks_after_human_approval() {
    use std::os::unix::fs::symlink;

    let source = tempfile::tempdir().unwrap();
    let cloud = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    let receipts = tempfile::tempdir().unwrap();
    let (materialization, plan, approval) = context(source.path(), cloud.path(), 30_000);
    let receipt_dir = receipts.path().join("receipts");

    symlink(external.path(), cloud.path().join("Recovered")).unwrap();
    assert_eq!(
        execute_incomplete_download_materialization(
            source.path(),
            &materialization,
            &plan,
            &approval,
            &plan.destination_plan_fingerprint,
            capacity(30_001),
            &receipt_dir,
            30_002,
        )
        .unwrap_err(),
        "materialization-execution-destination-symlink-component"
    );
    assert!(external.path().read_dir().unwrap().next().is_none());
    assert!(!receipt_dir.exists());
}
