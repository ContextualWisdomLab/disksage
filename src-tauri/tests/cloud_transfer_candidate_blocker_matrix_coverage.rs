//! Credential-free coverage for cloud-transfer candidate blocker composition.
//!
//! The tests mutate only in-memory planner evidence. They perform no provider call, filesystem
//! copy, deletion, credential access, or source mutation.

use disksage_lib::cloud::{
    candidate_review_fingerprint, ArchiveKind, CloudAccountScope, CloudCandidate, CloudProvider,
    CloudRoot,
};
use disksage_lib::cloud_transfer::{
    candidate_blockers_with_review, existing_copy_candidate_blockers_with_review,
};

#[cfg(windows)]
const CLOUD_ROOT_PATH: &str = r"C:\cloud";
#[cfg(not(windows))]
const CLOUD_ROOT_PATH: &str = "/cloud";
#[cfg(windows)]
const SOURCE_PATH: &str = r"C:\source\report.pdf";
#[cfg(not(windows))]
const SOURCE_PATH: &str = "/source/report.pdf";
#[cfg(windows)]
const DESTINATION_PATH: &str = r"C:\cloud\report.pdf";
#[cfg(not(windows))]
const DESTINATION_PATH: &str = "/cloud/report.pdf";
#[cfg(windows)]
const OUTSIDE_PATH: &str = r"C:\outside\report.pdf";
#[cfg(not(windows))]
const OUTSIDE_PATH: &str = "/outside/report.pdf";

fn root() -> CloudRoot {
    CloudRoot {
        id: "onedrive:personal:coverage".into(),
        provider: CloudProvider::Onedrive,
        account_scope: CloudAccountScope::Personal,
        label: "OneDrive coverage".into(),
        path: CLOUD_ROOT_PATH.into(),
        readable: true,
        access_issue: None,
    }
}

fn candidate() -> CloudCandidate {
    let mut candidate = CloudCandidate {
        metadata_fingerprint: "a".repeat(64),
        review_fingerprint: String::new(),
        src: SOURCE_PATH.into(),
        dst: DESTINATION_PATH.into(),
        provider: CloudProvider::Onedrive,
        destination_account_scope: CloudAccountScope::Personal,
        kind: ArchiveKind::Document,
        bytes: 1024,
        age_days: 90,
        created_ms: 1,
        modified_ms: 2,
        production_time_ms: 3,
        production_time_source: "embedded:exiftool:CreateDate".into(),
        production_time_confidence: "high".into(),
        source_root: "/source".into(),
        relative_path: "report.pdf".into(),
        source_context: "source".into(),
        requires_review: false,
        review_reasons: Vec::new(),
        content_title: Some("Report".into()),
        content_authors: Vec::new(),
        content_context: Vec::new(),
        duration_ms: None,
        dataset_profile: None,
        metadata_evidence: Vec::new(),
        blocked_reason: None,
    };
    candidate.review_fingerprint = candidate_review_fingerprint(&candidate);
    candidate
}

fn assert_blocked(candidate: &CloudCandidate, cloud_root: &CloudRoot, expected: &str) {
    let blockers = candidate_blockers_with_review(candidate, cloud_root, None);
    assert!(
        blockers.iter().any(|blocker| blocker == expected),
        "expected {expected:?} in blockers {blockers:?}"
    );
}

#[test]
fn candidate_blockers_report_fingerprint_review_and_planner_failures() {
    let cloud_root = root();

    let mut invalid_review_fingerprint = candidate();
    invalid_review_fingerprint.review_fingerprint = "z".repeat(64);
    assert_blocked(
        &invalid_review_fingerprint,
        &cloud_root,
        "review-fingerprint-invalid",
    );

    let mut mismatched_review_fingerprint = candidate();
    mismatched_review_fingerprint.review_fingerprint = "0".repeat(64);
    assert_blocked(
        &mismatched_review_fingerprint,
        &cloud_root,
        "review-fingerprint-mismatch",
    );

    let mut review_required = candidate();
    review_required.requires_review = true;
    review_required.review_reasons = vec!["operator-review-required".into()];
    review_required.review_fingerprint = candidate_review_fingerprint(&review_required);
    assert_blocked(&review_required, &cloud_root, "review-required");

    let mut planner_blocked = candidate();
    planner_blocked.blocked_reason = Some("source-read-failed".into());
    planner_blocked.review_fingerprint = candidate_review_fingerprint(&planner_blocked);
    assert_blocked(&planner_blocked, &cloud_root, "planner-blocked");
}

#[test]
fn candidate_blockers_report_metadata_provider_and_scope_mismatches() {
    let cloud_root = root();

    let mut missing_metadata = candidate();
    missing_metadata.metadata_fingerprint.clear();
    missing_metadata.review_fingerprint = candidate_review_fingerprint(&missing_metadata);
    assert_blocked(&missing_metadata, &cloud_root, "metadata-fingerprint-missing");

    let mut invalid_metadata = candidate();
    invalid_metadata.metadata_fingerprint = "z".repeat(64);
    invalid_metadata.review_fingerprint = candidate_review_fingerprint(&invalid_metadata);
    assert_blocked(&invalid_metadata, &cloud_root, "metadata-fingerprint-invalid");

    let mut provider_mismatch = candidate();
    provider_mismatch.provider = CloudProvider::GoogleDrive;
    provider_mismatch.review_fingerprint = candidate_review_fingerprint(&provider_mismatch);
    assert_blocked(&provider_mismatch, &cloud_root, "provider-mismatch");

    let mut scope_mismatch = candidate();
    scope_mismatch.destination_account_scope = CloudAccountScope::Shared;
    scope_mismatch.review_fingerprint = candidate_review_fingerprint(&scope_mismatch);
    assert_blocked(
        &scope_mismatch,
        &cloud_root,
        "destination-account-scope-mismatch",
    );
}

#[test]
fn candidate_blockers_report_path_authority_failures() {
    let cloud_root = root();

    let mut relative_source = candidate();
    relative_source.src = "relative/report.pdf".into();
    relative_source.review_fingerprint = candidate_review_fingerprint(&relative_source);
    assert_blocked(&relative_source, &cloud_root, "source-path-not-safe-absolute");

    let mut relative_destination = candidate();
    relative_destination.dst = "relative/report.pdf".into();
    relative_destination.review_fingerprint = candidate_review_fingerprint(&relative_destination);
    assert_blocked(
        &relative_destination,
        &cloud_root,
        "destination-path-not-safe-absolute",
    );

    let mut unsafe_root = cloud_root.clone();
    unsafe_root.path = "relative-cloud".into();
    assert_blocked(&candidate(), &unsafe_root, "cloud-root-not-safe-absolute");

    let mut same_path = candidate();
    same_path.dst = same_path.src.clone();
    same_path.review_fingerprint = candidate_review_fingerprint(&same_path);
    assert_blocked(&same_path, &cloud_root, "source-equals-destination");

    let mut source_inside_cloud = candidate();
    source_inside_cloud.src = DESTINATION_PATH.into();
    source_inside_cloud.review_fingerprint = candidate_review_fingerprint(&source_inside_cloud);
    assert_blocked(
        &source_inside_cloud,
        &cloud_root,
        "source-already-in-cloud-root",
    );

    let mut destination_outside_cloud = candidate();
    destination_outside_cloud.dst = OUTSIDE_PATH.into();
    destination_outside_cloud.review_fingerprint = candidate_review_fingerprint(&destination_outside_cloud);
    assert_blocked(
        &destination_outside_cloud,
        &cloud_root,
        "destination-outside-cloud-root",
    );
}

#[test]
fn existing_copy_admission_allows_only_the_exact_destination_exists_planner_state() {
    let cloud_root = root();

    let mut existing_destination = candidate();
    existing_destination.blocked_reason = Some("destination-exists".into());
    existing_destination.review_fingerprint = candidate_review_fingerprint(&existing_destination);
    let blockers = existing_copy_candidate_blockers_with_review(
        &existing_destination,
        &cloud_root,
        None,
    );
    assert!(
        !blockers.iter().any(|blocker| blocker == "planner-blocked"),
        "adoption may clear only the exact destination-exists planner blocker: {blockers:?}"
    );
    assert!(
        !blockers
            .iter()
            .any(|blocker| blocker == "existing-destination-plan-required"),
        "the existing destination plan must be accepted for adoption: {blockers:?}"
    );

    let ordinary_candidate = candidate();
    let blockers = existing_copy_candidate_blockers_with_review(
        &ordinary_candidate,
        &cloud_root,
        None,
    );
    assert!(
        blockers
            .iter()
            .any(|blocker| blocker == "existing-destination-plan-required"),
        "adoption must reject a candidate that was not planned for an existing destination: {blockers:?}"
    );

    let mut other_planner_failure = candidate();
    other_planner_failure.blocked_reason = Some("source-read-failed".into());
    other_planner_failure.review_fingerprint = candidate_review_fingerprint(&other_planner_failure);
    let blockers = existing_copy_candidate_blockers_with_review(
        &other_planner_failure,
        &cloud_root,
        None,
    );
    assert!(
        blockers.iter().any(|blocker| blocker == "planner-blocked"),
        "adoption must preserve unrelated planner blockers: {blockers:?}"
    );
    assert!(
        blockers
            .iter()
            .any(|blocker| blocker == "existing-destination-plan-required"),
        "adoption must also require the exact destination-exists planner state: {blockers:?}"
    );
}
