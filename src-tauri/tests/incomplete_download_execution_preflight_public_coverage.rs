//! Coverage-visible fail-closed preflight contracts for incomplete-download execution.
//!
//! These regressions construct valid local-only planning evidence, then stop execution at explicit
//! preflight boundaries before any source or destination content is copied. They therefore exercise
//! shipped execution authority without granting mutation or touching user data.

use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::content_digest::ContentDigests;
use disksage_lib::incomplete_download_materialization::{
    IncompleteDownloadMaterializationReport, IncompleteDownloadMaterializationUnit,
    MaterializationUnitKind, INCOMPLETE_DOWNLOAD_MATERIALIZATION_VERSION,
};
use disksage_lib::incomplete_download_materialization_destination::{
    approve_incomplete_download_destination, plan_incomplete_download_destination,
};
use disksage_lib::incomplete_download_materialization_execution::execute_incomplete_download_materialization;
use disksage_lib::provider_capacity::{parse_icloud_brctl_quota, CloudCapacitySnapshot};
use std::path::Path;

fn materialization_unit_fingerprint(
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

fn materialization_plan_fingerprint(
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
    let source_logical_bytes = 10;
    let created_ms = 10;
    let modified_ms = 11;
    let range_start = 0;
    let range_end = 10;
    let content_digests = ContentDigests {
        blake3: "5".repeat(64),
        sha256: "6".repeat(64),
        quick_xor_base64: "A".repeat(28),
    };
    let unit_fingerprint = materialization_unit_fingerprint(
        &candidate_fingerprint,
        source_relative_path,
        source_logical_bytes,
        created_ms,
        modified_ms,
        range_start,
        range_end,
        &content_digests,
    );
    let suggested_filename = format!(
        "recovered-{}-{}.zip",
        &content_digests.sha256[..12],
        &unit_fingerprint[..12]
    );
    let unit = IncompleteDownloadMaterializationUnit {
        candidate_fingerprint,
        source_relative_path: source_relative_path.into(),
        source_logical_bytes,
        source_filesystem_created_ms: created_ms,
        source_filesystem_modified_ms: modified_ms,
        kind: MaterializationUnitKind::FullZipFile,
        range_start,
        range_end,
        output_bytes: 10,
        output_mime_type: "application/zip".into(),
        output_extension: "zip".into(),
        content_digests,
        unit_fingerprint: unit_fingerprint.clone(),
        suggested_filename,
        active_use_evidence_complete: true,
        source_active: false,
        source_stable: true,
        destination_selected: false,
        requires_human_destination_review: true,
        approval_issued: false,
        write_performed: false,
    };
    let plan_fingerprint = materialization_plan_fingerprint(
        &source_scope_fingerprint,
        &audit_fingerprint,
        &validation_fingerprint,
        &unit_fingerprint,
    );
    IncompleteDownloadMaterializationReport {
        schema_version: INCOMPLETE_DOWNLOAD_MATERIALIZATION_VERSION,
        observed_at_ms: 900,
        source_root: source_root.to_string_lossy().into_owned(),
        source_scope_fingerprint,
        audit_fingerprint,
        validation_fingerprint,
        evidence_complete: true,
        source_file_count: 1,
        unit_count: 1,
        full_file_unit_count: 1,
        embedded_zip_range_unit_count: 0,
        planned_output_bytes: 10,
        plan_fingerprint,
        destination_selected: false,
        requires_human_destination_review: true,
        exact_materialization_approval_available: false,
        approval_issued: false,
        mutation_performed: false,
        units: vec![unit],
    }
}

fn cloud_root(path: &Path) -> CloudRoot {
    CloudRoot {
        id: "icloud:execution-coverage".into(),
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

fn planning_context(
    source_root: &Path,
    cloud_path: &Path,
    plan_observed_at_ms: u64,
) -> (
    IncompleteDownloadMaterializationReport,
    disksage_lib::incomplete_download_materialization_destination::IncompleteDownloadDestinationPlan,
    disksage_lib::incomplete_download_materialization_destination::IncompleteDownloadDestinationApproval,
) {
    let materialization = materialization(source_root);
    let plan = plan_incomplete_download_destination(
        &materialization,
        &cloud_root(cloud_path),
        "Recovered",
        capacity(plan_observed_at_ms - 1),
        0,
        plan_observed_at_ms,
    )
    .unwrap();
    let approval = approve_incomplete_download_destination(
        &plan,
        &plan.destination_plan_fingerprint,
        plan_observed_at_ms + 1,
        "human:coverage-reviewer",
        "verified exact local destination and fresh capacity",
    )
    .unwrap();
    (materialization, plan, approval)
}

#[test]
fn execution_rejects_time_order_and_invalid_capacity_before_source_access() {
    let source = tempfile::tempdir().unwrap();
    let cloud = tempfile::tempdir().unwrap();
    let receipts = tempfile::tempdir().unwrap();
    let (materialization, plan, approval) = planning_context(source.path(), cloud.path(), 5_000);
    let missing_source = source.path().join("missing-source");
    let receipt_dir = receipts.path().join("receipts");

    assert_eq!(
        execute_incomplete_download_materialization(
            &missing_source,
            &materialization,
            &plan,
            &approval,
            &plan.destination_plan_fingerprint,
            capacity(5_001),
            &receipt_dir,
            approval.approved_at_ms - 1,
        )
        .unwrap_err(),
        "materialization-execution-predates-approval"
    );

    let mut wrong_provider = capacity(5_002);
    wrong_provider.provider = CloudProvider::GoogleDrive;
    assert_eq!(
        execute_incomplete_download_materialization(
            &missing_source,
            &materialization,
            &plan,
            &approval,
            &plan.destination_plan_fingerprint,
            wrong_provider,
            &receipt_dir,
            5_003,
        )
        .unwrap_err(),
        "materialization-execution-capacity-evidence-invalid"
    );

    let mut missing_fingerprint = capacity(5_002);
    missing_fingerprint.evidence_fingerprint = None;
    assert_eq!(
        execute_incomplete_download_materialization(
            &missing_source,
            &materialization,
            &plan,
            &approval,
            &plan.destination_plan_fingerprint,
            missing_fingerprint,
            &receipt_dir,
            5_003,
        )
        .unwrap_err(),
        "materialization-execution-capacity-evidence-invalid"
    );

    assert!(!receipt_dir.exists());
    assert!(cloud.path().join("Recovered").read_dir().is_err());
}

#[test]
fn execution_rejects_missing_and_non_directory_source_roots_without_output_mutation() {
    let source = tempfile::tempdir().unwrap();
    let cloud = tempfile::tempdir().unwrap();
    let receipts = tempfile::tempdir().unwrap();
    let (materialization, plan, approval) = planning_context(source.path(), cloud.path(), 6_000);
    let receipt_dir = receipts.path().join("receipts");
    let missing_source = source.path().join("missing-source");

    assert_eq!(
        execute_incomplete_download_materialization(
            &missing_source,
            &materialization,
            &plan,
            &approval,
            &plan.destination_plan_fingerprint,
            capacity(6_002),
            &receipt_dir,
            6_003,
        )
        .unwrap_err(),
        "materialization-execution-source-root-unavailable"
    );

    let regular_file = source.path().join("regular-file-source");
    std::fs::write(&regular_file, b"not a directory").unwrap();
    assert_eq!(
        execute_incomplete_download_materialization(
            &regular_file,
            &materialization,
            &plan,
            &approval,
            &plan.destination_plan_fingerprint,
            capacity(6_002),
            &receipt_dir,
            6_003,
        )
        .unwrap_err(),
        "materialization-execution-source-root-unsafe"
    );

    assert_eq!(std::fs::read(&regular_file).unwrap(), b"not a directory");
    assert!(!receipt_dir.exists());
    assert!(cloud.path().join("Recovered").read_dir().is_err());
}

#[cfg(unix)]
#[test]
fn execution_rejects_symlink_source_root_without_output_mutation() {
    use std::os::unix::fs::symlink;

    let source = tempfile::tempdir().unwrap();
    let cloud = tempfile::tempdir().unwrap();
    let receipts = tempfile::tempdir().unwrap();
    let (materialization, plan, approval) = planning_context(source.path(), cloud.path(), 7_000);
    let linked_source = receipts.path().join("linked-source-root");
    let receipt_dir = receipts.path().join("receipts");
    symlink(source.path(), &linked_source).unwrap();

    assert_eq!(
        execute_incomplete_download_materialization(
            &linked_source,
            &materialization,
            &plan,
            &approval,
            &plan.destination_plan_fingerprint,
            capacity(7_002),
            &receipt_dir,
            7_003,
        )
        .unwrap_err(),
        "materialization-execution-source-root-unsafe"
    );

    assert!(
        std::fs::symlink_metadata(&linked_source)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(!receipt_dir.exists());
    assert!(cloud.path().join("Recovered").read_dir().is_err());
}

#[cfg(unix)]
#[test]
fn execution_rejects_cloud_root_replacement_before_output_mutation() {
    use std::os::unix::fs::symlink;

    let source = tempfile::tempdir().unwrap();
    let cloud_parent = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    let receipts = tempfile::tempdir().unwrap();
    let cloud_path = cloud_parent.path().join("cloud-root");
    let moved_cloud_path = cloud_parent.path().join("authorized-cloud-root-moved");
    let receipt_dir = receipts.path().join("receipts");
    std::fs::create_dir(&cloud_path).unwrap();
    let (materialization, plan, approval) = planning_context(source.path(), &cloud_path, 8_000);

    std::fs::rename(&cloud_path, &moved_cloud_path).unwrap();
    symlink(external.path(), &cloud_path).unwrap();

    assert_eq!(
        execute_incomplete_download_materialization(
            source.path(),
            &materialization,
            &plan,
            &approval,
            &plan.destination_plan_fingerprint,
            capacity(8_002),
            &receipt_dir,
            8_003,
        )
        .unwrap_err(),
        "materialization-execution-cloud-root-unsafe"
    );

    assert!(
        std::fs::symlink_metadata(&cloud_path)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(moved_cloud_path.is_dir());
    assert!(!receipt_dir.exists());
    assert!(external.path().join("Recovered").read_dir().is_err());
    assert!(moved_cloud_path.join("Recovered").read_dir().is_err());
}
