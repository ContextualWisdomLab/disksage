//! Coverage-visible destination planning blockers not exercised by the baseline happy-path tests.
//!
//! All filesystem objects are temporary. Planning is read-only and the assertions prove exact
//! fail-closed root admission plus explicit non-authorizing blocker evidence.

use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::content_digest::ContentDigests;
use disksage_lib::incomplete_download_materialization::{
    IncompleteDownloadMaterializationReport, IncompleteDownloadMaterializationUnit,
    MaterializationUnitKind, INCOMPLETE_DOWNLOAD_MATERIALIZATION_VERSION,
};
use disksage_lib::incomplete_download_materialization_destination::{
    plan_incomplete_download_destination, MAX_CAPACITY_AGE_MS,
};
use disksage_lib::provider_capacity::{
    parse_icloud_brctl_quota, unavailable_capacity,
};
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

fn root(path: impl Into<String>) -> CloudRoot {
    CloudRoot {
        id: "icloud:destination-blocker-coverage".into(),
        provider: CloudProvider::Icloud,
        account_scope: CloudAccountScope::Unknown,
        label: "iCloud".into(),
        path: path.into(),
        readable: true,
        access_issue: None,
    }
}

fn capacity(remaining_bytes: u64, observed_at_ms: u64) -> disksage_lib::provider_capacity::CloudCapacitySnapshot {
    parse_icloud_brctl_quota(
        &format!("{remaining_bytes} bytes of quota remaining in personal account\n"),
        observed_at_ms,
    )
    .unwrap()
}

#[test]
fn planning_rejects_invalid_source_plan_and_unsafe_or_unavailable_cloud_roots() {
    let source = tempfile::tempdir().unwrap();
    let cloud = tempfile::tempdir().unwrap();
    let valid = materialization(source.path());

    let mut invalid_source = valid.clone();
    invalid_source.schema_version = 0;
    assert_eq!(
        plan_incomplete_download_destination(
            &invalid_source,
            &root(cloud.path().to_string_lossy().into_owned()),
            "Recovered",
            capacity(10_000, 1_000),
            0,
            1_001,
        )
        .unwrap_err(),
        "materialization-destination-source-plan-integrity-invalid"
    );

    for cloud_path in [
        "relative/cloud".to_string(),
        cloud.path().join("..").join("escape").to_string_lossy().into_owned(),
    ] {
        assert_eq!(
            plan_incomplete_download_destination(
                &valid,
                &root(cloud_path),
                "Recovered",
                capacity(10_000, 1_000),
                0,
                1_001,
            )
            .unwrap_err(),
            "materialization-destination-cloud-root-unsafe"
        );
    }

    let missing = cloud.path().join("missing-root");
    assert_eq!(
        plan_incomplete_download_destination(
            &valid,
            &root(missing.to_string_lossy().into_owned()),
            "Recovered",
            capacity(10_000, 1_000),
            0,
            1_001,
        )
        .unwrap_err(),
        "materialization-destination-cloud-root-unavailable"
    );

    let regular_file = cloud.path().join("regular-file-root");
    std::fs::write(&regular_file, b"not a directory").unwrap();
    assert_eq!(
        plan_incomplete_download_destination(
            &valid,
            &root(regular_file.to_string_lossy().into_owned()),
            "Recovered",
            capacity(10_000, 1_000),
            0,
            1_001,
        )
        .unwrap_err(),
        "materialization-destination-cloud-root-unsafe"
    );
}

#[cfg(unix)]
#[test]
fn planning_rejects_symlink_cloud_root_without_following_it() {
    use std::os::unix::fs::symlink;

    let source = tempfile::tempdir().unwrap();
    let cloud_parent = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    let cloud_link = cloud_parent.path().join("cloud-link");
    symlink(external.path(), &cloud_link).unwrap();

    assert_eq!(
        plan_incomplete_download_destination(
            &materialization(source.path()),
            &root(cloud_link.to_string_lossy().into_owned()),
            "Recovered",
            capacity(10_000, 2_000),
            0,
            2_001,
        )
        .unwrap_err(),
        "materialization-destination-cloud-root-unsafe"
    );
    assert!(external.path().read_dir().unwrap().next().is_none());
}

#[test]
fn planning_keeps_untrusted_capacity_and_root_quality_as_explicit_blockers() {
    let source = tempfile::tempdir().unwrap();
    let cloud = tempfile::tempdir().unwrap();
    let materialization = materialization(source.path());

    let mut stale_root = root(cloud.path().to_string_lossy().into_owned());
    stale_root.readable = false;
    stale_root.access_issue = Some("discovery-access-denied".into());
    let stale_plan = plan_incomplete_download_destination(
        &materialization,
        &stale_root,
        "Recovered",
        capacity(10_000, 3_000),
        0,
        3_000 + MAX_CAPACITY_AGE_MS + 1,
    )
    .unwrap();
    assert!(!stale_plan.eligible_after_human_approval);
    for blocker in [
        "materialization-destination-cloud-root-not-readable",
        "materialization-destination-cloud-root-access-issue",
        "materialization-destination-capacity-stale",
    ] {
        assert!(stale_plan.blockers.contains(&blocker.into()));
    }

    let mut missing_fingerprint = capacity(10_000, 4_000);
    missing_fingerprint.evidence_fingerprint = None;
    let missing_fingerprint_plan = plan_incomplete_download_destination(
        &materialization,
        &root(cloud.path().to_string_lossy().into_owned()),
        "Recovered",
        missing_fingerprint,
        0,
        4_001,
    )
    .unwrap();
    assert!(missing_fingerprint_plan
        .blockers
        .contains(&"materialization-destination-capacity-fingerprint-missing".into()));

    let insufficient = plan_incomplete_download_destination(
        &materialization,
        &root(cloud.path().to_string_lossy().into_owned()),
        "Recovered",
        capacity(1, 5_000),
        0,
        5_001,
    )
    .unwrap();
    assert!(!insufficient.eligible_after_human_approval);
    assert!(insufficient
        .blockers
        .contains(&"cloud-capacity-insufficient-with-reserve".into()));

    let unavailable = plan_incomplete_download_destination(
        &materialization,
        &root(cloud.path().to_string_lossy().into_owned()),
        "Recovered",
        unavailable_capacity(CloudProvider::Icloud, 6_000, "icloud-native-quota-unavailable"),
        0,
        6_001,
    )
    .unwrap();
    assert!(!unavailable.eligible_after_human_approval);
    for blocker in [
        "materialization-destination-account-scope-unverified",
        "materialization-destination-capacity-fingerprint-missing",
        "icloud-native-quota-unavailable",
    ] {
        assert!(unavailable.blockers.contains(&blocker.into()));
    }
}
