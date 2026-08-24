//! Coverage-visible public contracts for incomplete-download destination planning and approval.
//!
//! The source module's internal tests are intentionally excluded from `cfg(coverage)`. These
//! regressions therefore exercise the shipped public boundary with synthetic, local-only evidence
//! while keeping every filesystem mutation inside temporary directories.

use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::content_digest::ContentDigests;
use disksage_lib::incomplete_download_materialization::{
    IncompleteDownloadMaterializationReport, IncompleteDownloadMaterializationUnit,
    MaterializationUnitKind, INCOMPLETE_DOWNLOAD_MATERIALIZATION_VERSION,
};
use disksage_lib::incomplete_download_materialization_destination::{
    approve_incomplete_download_destination, destination_approval_phrase,
    incomplete_download_destination_plan_integrity_valid, plan_incomplete_download_destination,
    summarize_incomplete_download_destination, validate_incomplete_download_destination_approval,
};
use disksage_lib::provider_capacity::parse_icloud_brctl_quota;
use std::path::Path;

fn unit_fingerprint(
    candidate_fingerprint: &str,
    source_relative_path: &str,
    source_logical_bytes: u64,
    source_filesystem_created_ms: u64,
    source_filesystem_modified_ms: u64,
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
        source_filesystem_created_ms,
        source_filesystem_modified_ms,
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

fn valid_materialization(source_root: &Path) -> IncompleteDownloadMaterializationReport {
    let source_scope_fingerprint = "1".repeat(64);
    let audit_fingerprint = "2".repeat(64);
    let validation_fingerprint = "3".repeat(64);
    let candidate_fingerprint = "4".repeat(64);
    let source_relative_path = "downloads/source.zip.crdownload";
    let source_logical_bytes = 10;
    let source_filesystem_created_ms = 10;
    let source_filesystem_modified_ms = 11;
    let range_start = 0;
    let range_end = 10;
    let content_digests = ContentDigests {
        blake3: "5".repeat(64),
        sha256: "6".repeat(64),
        quick_xor_base64: "A".repeat(28),
    };
    let unit_fingerprint = unit_fingerprint(
        &candidate_fingerprint,
        source_relative_path,
        source_logical_bytes,
        source_filesystem_created_ms,
        source_filesystem_modified_ms,
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
        source_filesystem_created_ms,
        source_filesystem_modified_ms,
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

fn icloud_root(path: &Path) -> CloudRoot {
    CloudRoot {
        id: "icloud:coverage".into(),
        provider: CloudProvider::Icloud,
        account_scope: CloudAccountScope::Unknown,
        label: "iCloud".into(),
        path: path.to_string_lossy().into_owned(),
        readable: true,
        access_issue: None,
    }
}

fn capacity(observed_at_ms: u64) -> disksage_lib::provider_capacity::CloudCapacitySnapshot {
    parse_icloud_brctl_quota(
        "10000000000 bytes of quota remaining in personal account\n",
        observed_at_ms,
    )
    .unwrap()
}

#[test]
fn eligible_plan_exposes_only_exact_human_approval_and_redacted_summary() {
    let source = tempfile::tempdir().unwrap();
    let cloud = tempfile::tempdir().unwrap();
    let materialization = valid_materialization(source.path());
    let plan = plan_incomplete_download_destination(
        &materialization,
        &icloud_root(cloud.path()),
        "Recovered",
        capacity(1_000),
        0,
        1_001,
    )
    .unwrap();

    assert!(incomplete_download_destination_plan_integrity_valid(&plan));
    assert!(plan.eligible_after_human_approval);
    assert!(plan.exact_approval_available);
    assert!(!plan.approval_issued);
    assert!(!plan.mutation_performed);
    assert_eq!(plan.provider, CloudProvider::Icloud);
    assert_eq!(plan.account_scope, CloudAccountScope::Personal);
    assert_eq!(plan.unit_count, 1);
    assert!(plan
        .blockers
        .contains(&"human-materialization-destination-approval-required".into()));
    assert_eq!(plan.units[0].write_performed, false);
    assert_eq!(plan.units[0].destination_exists, false);

    let phrase = destination_approval_phrase(&plan).expect("eligible exact plan has phrase");
    assert!(phrase.contains(&plan.destination_plan_fingerprint));
    let summary = summarize_incomplete_download_destination(&plan);
    assert_eq!(summary.output_mode, "incomplete-download-destination-plan-summary");
    assert_eq!(summary.exact_approval_phrase.as_deref(), Some(phrase.as_str()));
    assert!(summary.destination_paths_and_names_redacted);
    let encoded = serde_json::to_string(&summary).unwrap();
    assert!(!encoded.contains(cloud.path().to_string_lossy().as_ref()));
    assert!(!encoded.contains(&materialization.units[0].suggested_filename));
}

#[test]
fn approval_requires_exact_plan_human_attribution_rationale_and_time_order() {
    let source = tempfile::tempdir().unwrap();
    let cloud = tempfile::tempdir().unwrap();
    let materialization = valid_materialization(source.path());
    let plan = plan_incomplete_download_destination(
        &materialization,
        &icloud_root(cloud.path()),
        "Recovered",
        capacity(2_000),
        0,
        2_001,
    )
    .unwrap();

    assert_eq!(
        approve_incomplete_download_destination(&plan, &"f".repeat(64), 2_002, "human:qa", "verified")
            .unwrap_err(),
        "materialization-destination-plan-fingerprint-mismatch"
    );
    assert_eq!(
        approve_incomplete_download_destination(
            &plan,
            &plan.destination_plan_fingerprint,
            2_000,
            "human:qa",
            "verified",
        )
        .unwrap_err(),
        "materialization-destination-approval-predates-plan"
    );
    assert_eq!(
        approve_incomplete_download_destination(
            &plan,
            &plan.destination_plan_fingerprint,
            2_002,
            "automation",
            "verified",
        )
        .unwrap_err(),
        "materialization-destination-human-attribution-required"
    );
    assert_eq!(
        approve_incomplete_download_destination(
            &plan,
            &plan.destination_plan_fingerprint,
            2_002,
            "human:qa",
            "   ",
        )
        .unwrap_err(),
        "materialization-destination-rationale-invalid"
    );

    let approval = approve_incomplete_download_destination(
        &plan,
        &plan.destination_plan_fingerprint,
        2_002,
        "  human:qa  ",
        "  reviewed exact destination and capacity  ",
    )
    .unwrap();
    assert_eq!(approval.approved_by, "human:qa");
    assert_eq!(approval.rationale, "reviewed exact destination and capacity");
    validate_incomplete_download_destination_approval(
        &plan,
        &approval,
        &plan.destination_plan_fingerprint,
    )
    .unwrap();

    let mut tampered = approval.clone();
    tampered.approval_id = "0".repeat(64);
    assert_eq!(
        validate_incomplete_download_destination_approval(
            &plan,
            &tampered,
            &plan.destination_plan_fingerprint,
        )
        .unwrap_err(),
        "materialization-destination-approval-integrity-mismatch"
    );
}

#[test]
fn destination_subdirectory_admission_rejects_unsafe_shapes_without_mutation() {
    let source = tempfile::tempdir().unwrap();
    let cloud = tempfile::tempdir().unwrap();
    let materialization = valid_materialization(source.path());
    let root = icloud_root(cloud.path());

    for destination in ["", "../escape", "/absolute"] {
        assert_eq!(
            plan_incomplete_download_destination(
                &materialization,
                &root,
                destination,
                capacity(3_000),
                0,
                3_001,
            )
            .unwrap_err(),
            "materialization-destination-subdirectory-unsafe",
            "unexpected admission result for {destination:?}"
        );
    }
    assert!(cloud.path().read_dir().unwrap().next().is_none());
}
