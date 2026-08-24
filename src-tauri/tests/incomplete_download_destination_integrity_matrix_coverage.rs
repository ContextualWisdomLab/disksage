//! Coverage-visible integrity and human-approval matrix for incomplete-download destinations.
//!
//! The tests build plans only against temporary local roots. No destination bytes are written by
//! production code and no source mutation authority is exercised.

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
    IncompleteDownloadDestinationApproval, INCOMPLETE_DOWNLOAD_DESTINATION_PLAN_VERSION,
};
use disksage_lib::provider_capacity::{parse_icloud_brctl_quota, CAPACITY_SCHEMA_VERSION};
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
    let digests = ContentDigests {
        blake3: "5".repeat(64),
        sha256: "6".repeat(64),
        quick_xor_base64: "A".repeat(28),
    };
    let unit_fingerprint = unit_fingerprint(
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
        unit_fingerprint: unit_fingerprint.clone(),
        suggested_filename: format!(
            "recovered-{}-{}.zip",
            &digests.sha256[..12],
            &unit_fingerprint[..12]
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
        plan_fingerprint: materialization_plan_fingerprint(
            &source_scope_fingerprint,
            &audit_fingerprint,
            &validation_fingerprint,
            &unit_fingerprint,
        ),
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
        id: "icloud:destination-integrity-coverage".into(),
        provider: CloudProvider::Icloud,
        account_scope: CloudAccountScope::Unknown,
        label: "iCloud".into(),
        path: path.to_string_lossy().into_owned(),
        readable: true,
        access_issue: None,
    }
}

fn approval_id(
    destination_plan_fingerprint: &str,
    approved_at_ms: u64,
    approved_by: &str,
    rationale: &str,
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-incomplete-download-destination-approval-v1\0");
    hasher.update(destination_plan_fingerprint.as_bytes());
    hasher.update(&[0]);
    hasher.update(&approved_at_ms.to_le_bytes());
    hasher.update(approved_by.as_bytes());
    hasher.update(&[0]);
    hasher.update(rationale.as_bytes());
    hasher.finalize().to_hex().to_string()
}

fn valid_plan(
    source_root: &Path,
    cloud_root_path: &Path,
) -> disksage_lib::incomplete_download_materialization_destination::IncompleteDownloadDestinationPlan {
    let materialization = materialization(source_root);
    let capacity = parse_icloud_brctl_quota(
        "10000000000 bytes of quota remaining in personal account\n",
        1_000,
    )
    .unwrap();
    plan_incomplete_download_destination(
        &materialization,
        &cloud_root(cloud_root_path),
        "Recovered",
        capacity,
        0,
        1_001,
    )
    .unwrap()
}

#[test]
fn destination_integrity_rejects_identity_shape_and_state_drift() {
    let source = tempfile::tempdir().unwrap();
    let cloud = tempfile::tempdir().unwrap();
    let plan = valid_plan(source.path(), cloud.path());
    assert!(incomplete_download_destination_plan_integrity_valid(&plan));
    assert!(destination_approval_phrase(&plan).is_some());

    let summary = summarize_incomplete_download_destination(&plan);
    assert_eq!(summary.schema_version, INCOMPLETE_DOWNLOAD_DESTINATION_PLAN_VERSION);
    assert_eq!(summary.exact_approval_phrase, destination_approval_phrase(&plan));
    assert!(summary.destination_paths_and_names_redacted);
    assert!(!summary.approval_issued);
    assert!(!summary.mutation_performed);

    let mut cases = Vec::new();

    let mut mutated = plan.clone();
    mutated.schema_version = 0;
    cases.push(mutated);

    let mut mutated = plan.clone();
    mutated.source_scope_fingerprint = "A".repeat(64);
    cases.push(mutated);

    let mut mutated = plan.clone();
    mutated.account_scope = CloudAccountScope::Unknown;
    cases.push(mutated);

    let mut mutated = plan.clone();
    mutated.cloud_root_id.clear();
    cases.push(mutated);

    let mut mutated = plan.clone();
    mutated.cloud_root = "relative/cloud".into();
    cases.push(mutated);

    let mut mutated = plan.clone();
    mutated.destination_subdirectory = "../Recovered".into();
    cases.push(mutated);

    let mut mutated = plan.clone();
    mutated.source_file_count = 0;
    cases.push(mutated);

    let mut mutated = plan.clone();
    mutated.unit_count += 1;
    cases.push(mutated);

    let mut mutated = plan.clone();
    mutated.units.clear();
    mutated.unit_count = 0;
    cases.push(mutated);

    let mut mutated = plan.clone();
    mutated.planned_output_bytes = 0;
    cases.push(mutated);

    let mut mutated = plan.clone();
    mutated.approval_issued = true;
    cases.push(mutated);

    let mut mutated = plan.clone();
    mutated.mutation_performed = true;
    cases.push(mutated);

    let mut mutated = plan.clone();
    mutated.blockers = vec!["z-last".into(), "a-first".into()];
    cases.push(mutated);

    let mut mutated = plan.clone();
    mutated.notices = vec!["z-last".into(), "a-first".into()];
    cases.push(mutated);

    for mutated in cases {
        assert!(!incomplete_download_destination_plan_integrity_valid(&mutated));
        assert_eq!(destination_approval_phrase(&mutated), None);
    }
}

#[test]
fn destination_integrity_rejects_capacity_unit_and_fingerprint_drift() {
    let source = tempfile::tempdir().unwrap();
    let cloud = tempfile::tempdir().unwrap();
    let plan = valid_plan(source.path(), cloud.path());

    let mut mutated = plan.clone();
    mutated.exact_approval_available = false;
    assert!(!incomplete_download_destination_plan_integrity_valid(&mutated));

    let mut mutated = plan.clone();
    mutated.capacity.can_fit = None;
    assert!(!incomplete_download_destination_plan_integrity_valid(&mutated));

    let mut mutated = plan.clone();
    mutated.capacity.snapshot.evidence_fingerprint = None;
    assert!(!incomplete_download_destination_plan_integrity_valid(&mutated));

    let mut mutated = plan.clone();
    mutated.capacity.snapshot.observed_at_ms = plan.observed_at_ms + 1;
    assert!(!incomplete_download_destination_plan_integrity_valid(&mutated));

    let mut mutated = plan.clone();
    mutated.capacity.snapshot.schema_version = CAPACITY_SCHEMA_VERSION + 1;
    assert!(!incomplete_download_destination_plan_integrity_valid(&mutated));

    let mut mutated = plan.clone();
    mutated.capacity.snapshot.account_scope = Some(CloudAccountScope::Organization);
    assert!(!incomplete_download_destination_plan_integrity_valid(&mutated));

    let mut mutated = plan.clone();
    mutated.capacity.requested_bytes += 1;
    assert!(!incomplete_download_destination_plan_integrity_valid(&mutated));

    let mut mutated = plan.clone();
    mutated.units[0].materialization_unit_fingerprint = "invalid".into();
    assert!(!incomplete_download_destination_plan_integrity_valid(&mutated));

    let mut mutated = plan.clone();
    mutated.units[0].output_bytes = 0;
    assert!(!incomplete_download_destination_plan_integrity_valid(&mutated));

    let mut mutated = plan.clone();
    mutated.units[0].destination_relative_path = "/absolute/output.zip".into();
    assert!(!incomplete_download_destination_plan_integrity_valid(&mutated));

    let mut mutated = plan.clone();
    mutated.units[0].destination_relative_path = "Other/output.zip".into();
    assert!(!incomplete_download_destination_plan_integrity_valid(&mutated));

    let mut mutated = plan.clone();
    mutated.units[0].destination_relative_path = "Recovered/..".into();
    assert!(!incomplete_download_destination_plan_integrity_valid(&mutated));

    let mut mutated = plan.clone();
    mutated.units[0].write_performed = true;
    assert!(!incomplete_download_destination_plan_integrity_valid(&mutated));

    let mut mutated = plan.clone();
    mutated.units[0].output_bytes += 1;
    assert!(!incomplete_download_destination_plan_integrity_valid(&mutated));

    let mut mutated = plan.clone();
    mutated.capacity.largest_candidate_bytes += 1;
    assert!(!incomplete_download_destination_plan_integrity_valid(&mutated));

    let mut mutated = plan.clone();
    mutated.capacity.reserve_bytes += 1;
    assert!(!incomplete_download_destination_plan_integrity_valid(&mutated));

    let mut mutated = plan.clone();
    mutated.destination_plan_fingerprint = "f".repeat(64);
    assert!(!incomplete_download_destination_plan_integrity_valid(&mutated));
}

#[test]
fn destination_approval_rejects_mismatch_ineligible_attribution_and_rationale() {
    let source = tempfile::tempdir().unwrap();
    let cloud = tempfile::tempdir().unwrap();
    let plan = valid_plan(source.path(), cloud.path());

    assert_eq!(
        approve_incomplete_download_destination(
            &plan,
            &"f".repeat(64),
            1_002,
            "human:reviewer",
            "verified destination",
        )
        .unwrap_err(),
        "materialization-destination-plan-fingerprint-mismatch"
    );
    assert_eq!(
        approve_incomplete_download_destination(
            &plan,
            &plan.destination_plan_fingerprint,
            plan.observed_at_ms - 1,
            "human:reviewer",
            "verified destination",
        )
        .unwrap_err(),
        "materialization-destination-approval-predates-plan"
    );

    for reviewer in ["reviewer", "human:"] {
        assert_eq!(
            approve_incomplete_download_destination(
                &plan,
                &plan.destination_plan_fingerprint,
                1_002,
                reviewer,
                "verified destination",
            )
            .unwrap_err(),
            "materialization-destination-human-attribution-required"
        );
    }
    assert_eq!(
        approve_incomplete_download_destination(
            &plan,
            &plan.destination_plan_fingerprint,
            1_002,
            &format!("human:{}", "x".repeat(2_049)),
            "verified destination",
        )
        .unwrap_err(),
        "materialization-destination-human-attribution-required"
    );
    assert_eq!(
        approve_incomplete_download_destination(
            &plan,
            &plan.destination_plan_fingerprint,
            1_002,
            "human:reviewer",
            "   ",
        )
        .unwrap_err(),
        "materialization-destination-rationale-invalid"
    );
    assert_eq!(
        approve_incomplete_download_destination(
            &plan,
            &plan.destination_plan_fingerprint,
            1_002,
            "human:reviewer",
            &"x".repeat(2_049),
        )
        .unwrap_err(),
        "materialization-destination-rationale-invalid"
    );

    let existing = cloud.path().join("Recovered");
    std::fs::create_dir(&existing).unwrap();
    std::fs::write(
        existing.join(&materialization(source.path()).units[0].suggested_filename),
        b"existing user data",
    )
    .unwrap();
    let ineligible = valid_plan(source.path(), cloud.path());
    assert!(incomplete_download_destination_plan_integrity_valid(&ineligible));
    assert!(!ineligible.eligible_after_human_approval);
    assert_eq!(
        approve_incomplete_download_destination(
            &ineligible,
            &ineligible.destination_plan_fingerprint,
            1_002,
            "human:reviewer",
            "verified destination",
        )
        .unwrap_err(),
        "materialization-destination-plan-not-eligible"
    );
}

#[test]
fn destination_approval_validation_distinguishes_integrity_from_semantic_invalidity() {
    let source = tempfile::tempdir().unwrap();
    let cloud = tempfile::tempdir().unwrap();
    let plan = valid_plan(source.path(), cloud.path());
    let approval = approve_incomplete_download_destination(
        &plan,
        &plan.destination_plan_fingerprint,
        1_002,
        "human:reviewer",
        "verified destination",
    )
    .unwrap();
    validate_incomplete_download_destination_approval(
        &plan,
        &approval,
        &plan.destination_plan_fingerprint,
    )
    .unwrap();

    let mut mutated = approval.clone();
    mutated.schema_version = 0;
    assert_eq!(
        validate_incomplete_download_destination_approval(
            &plan,
            &mutated,
            &plan.destination_plan_fingerprint,
        )
        .unwrap_err(),
        "materialization-destination-approval-integrity-mismatch"
    );

    let mut mutated = approval.clone();
    mutated.destination_plan_fingerprint = "f".repeat(64);
    assert_eq!(
        validate_incomplete_download_destination_approval(
            &plan,
            &mutated,
            &plan.destination_plan_fingerprint,
        )
        .unwrap_err(),
        "materialization-destination-approval-integrity-mismatch"
    );

    assert_eq!(
        validate_incomplete_download_destination_approval(&plan, &approval, &"f".repeat(64))
            .unwrap_err(),
        "materialization-destination-approval-integrity-mismatch"
    );

    let mut mutated = approval.clone();
    mutated.approval_id = "0".repeat(64);
    assert_eq!(
        validate_incomplete_download_destination_approval(
            &plan,
            &mutated,
            &plan.destination_plan_fingerprint,
        )
        .unwrap_err(),
        "materialization-destination-approval-integrity-mismatch"
    );

    let invalid_semantics = [
        (plan.observed_at_ms - 1, "human:reviewer", "verified destination"),
        (1_002, "reviewer", "verified destination"),
        (1_002, "human:", "verified destination"),
        (1_002, "human:reviewer", "   "),
    ];
    for (approved_at_ms, approved_by, rationale) in invalid_semantics {
        let mutated = IncompleteDownloadDestinationApproval {
            schema_version: INCOMPLETE_DOWNLOAD_DESTINATION_PLAN_VERSION,
            approval_id: approval_id(
                &plan.destination_plan_fingerprint,
                approved_at_ms,
                approved_by,
                rationale,
            ),
            destination_plan_fingerprint: plan.destination_plan_fingerprint.clone(),
            approved_at_ms,
            approved_by: approved_by.into(),
            rationale: rationale.into(),
        };
        assert_eq!(
            validate_incomplete_download_destination_approval(
                &plan,
                &mutated,
                &plan.destination_plan_fingerprint,
            )
            .unwrap_err(),
            "materialization-destination-approval-invalid"
        );
    }
}
