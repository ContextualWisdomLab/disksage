//! Coverage-visible safety matrix for incomplete-download destination planning.
//!
//! These regressions exercise fail-closed public planning and approval boundaries with local-only
//! temporary directories. They never authorize source eviction and never mutate user data.

use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::content_digest::ContentDigests;
use disksage_lib::incomplete_download_materialization::{
    IncompleteDownloadMaterializationReport, IncompleteDownloadMaterializationUnit,
    MaterializationUnitKind, INCOMPLETE_DOWNLOAD_MATERIALIZATION_VERSION,
};
use disksage_lib::incomplete_download_materialization_destination::{
    approve_incomplete_download_destination, destination_approval_phrase,
    incomplete_download_destination_plan_integrity_valid, plan_incomplete_download_destination,
};
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
        id: "icloud:destination-safety-coverage".into(),
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

#[test]
fn planning_rejects_invalid_capacity_and_bounded_subdirectory_shapes() {
    let source = tempfile::tempdir().unwrap();
    let cloud = tempfile::tempdir().unwrap();
    let materialization = materialization(source.path());
    let cloud_root = root(cloud.path());

    let mut wrong_schema = capacity(1_000);
    wrong_schema.schema_version = 0;
    assert_eq!(
        plan_incomplete_download_destination(
            &materialization,
            &cloud_root,
            "Recovered",
            wrong_schema,
            0,
            1_001,
        )
        .unwrap_err(),
        "materialization-destination-capacity-snapshot-invalid"
    );

    let mut zero_timestamp = capacity(1_000);
    zero_timestamp.observed_at_ms = 0;
    assert_eq!(
        plan_incomplete_download_destination(
            &materialization,
            &cloud_root,
            "Recovered",
            zero_timestamp,
            0,
            1_001,
        )
        .unwrap_err(),
        "materialization-destination-capacity-snapshot-invalid"
    );

    let future = capacity(1_002);
    assert_eq!(
        plan_incomplete_download_destination(
            &materialization,
            &cloud_root,
            "Recovered",
            future,
            0,
            1_001,
        )
        .unwrap_err(),
        "materialization-destination-capacity-snapshot-invalid"
    );

    let too_deep = (0..17).map(|_| "segment").collect::<Vec<_>>().join("/");
    let too_long = "x".repeat(1_025);
    for destination in [too_deep.as_str(), too_long.as_str()] {
        assert_eq!(
            plan_incomplete_download_destination(
                &materialization,
                &cloud_root,
                destination,
                capacity(1_000),
                0,
                1_001,
            )
            .unwrap_err(),
            "materialization-destination-subdirectory-unsafe"
        );
    }
}

#[test]
fn planning_rejects_non_directory_prefix_and_existing_output_blocks_approval() {
    let source = tempfile::tempdir().unwrap();
    let cloud = tempfile::tempdir().unwrap();
    let materialization = materialization(source.path());
    let cloud_root = root(cloud.path());

    std::fs::write(cloud.path().join("Recovered"), b"not a directory").unwrap();
    assert_eq!(
        plan_incomplete_download_destination(
            &materialization,
            &cloud_root,
            "Recovered/Nested",
            capacity(2_000),
            0,
            2_001,
        )
        .unwrap_err(),
        "materialization-destination-parent-not-directory"
    );
    std::fs::remove_file(cloud.path().join("Recovered")).unwrap();

    let recovered = cloud.path().join("Recovered");
    std::fs::create_dir(&recovered).unwrap();
    let existing_output = recovered.join(&materialization.units[0].suggested_filename);
    std::fs::write(&existing_output, b"preexisting user data").unwrap();

    let plan = plan_incomplete_download_destination(
        &materialization,
        &cloud_root,
        "Recovered",
        capacity(2_000),
        0,
        2_001,
    )
    .unwrap();
    assert!(incomplete_download_destination_plan_integrity_valid(&plan));
    assert!(!plan.eligible_after_human_approval);
    assert!(!plan.exact_approval_available);
    assert!(plan
        .blockers
        .contains(&"materialization-destination-output-exists".into()));
    assert_eq!(destination_approval_phrase(&plan), None);
    assert_eq!(
        approve_incomplete_download_destination(
            &plan,
            &plan.destination_plan_fingerprint,
            2_002,
            "human:coverage-reviewer",
            "preexisting destination must block execution",
        )
        .unwrap_err(),
        "materialization-destination-plan-not-eligible"
    );
    assert_eq!(std::fs::read(existing_output).unwrap(), b"preexisting user data");
}

#[cfg(unix)]
#[test]
fn planning_rejects_symlinked_destination_prefix_without_following_it() {
    use std::os::unix::fs::symlink;

    let source = tempfile::tempdir().unwrap();
    let cloud = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    let materialization = materialization(source.path());
    symlink(external.path(), cloud.path().join("Recovered")).unwrap();

    assert_eq!(
        plan_incomplete_download_destination(
            &materialization,
            &root(cloud.path()),
            "Recovered/Nested",
            capacity(3_000),
            0,
            3_001,
        )
        .unwrap_err(),
        "materialization-destination-symlink-component"
    );
    assert!(external.path().read_dir().unwrap().next().is_none());
}
