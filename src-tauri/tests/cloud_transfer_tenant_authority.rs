//! Regression coverage for organization-tenant authority at the durable Rust transfer gate.
//!
//! The frontend projection and the Rust executor must apply the same fail-closed
//! truth table. Either an organization destination scope or the canonical
//! organization-sensitive review reason requires an explicit tenant-authority
//! attestation before a reviewed candidate can become copy-eligible.

use disksage_lib::cloud::{
    candidate_review_fingerprint, ArchiveKind, CloudAccountScope, CloudCandidate, CloudProvider,
    CloudRoot, MetadataEvidence,
};
use disksage_lib::cloud_review::{create_attributed_decision, CloudReviewDisposition};
use disksage_lib::cloud_transfer::candidate_blockers_with_review;

const ORGANIZATION_TENANT_AUTHORITY_REVIEW_REASON: &str =
    "organization-cloud-sensitive-context-needs-explicit-tenant-approval";
const ATTESTATION_BLOCKER: &str = "organization-tenant-authority-attestation-required";

#[cfg(windows)]
const CLOUD_ROOT_PATH: &str = r"C:\cloud";
#[cfg(not(windows))]
const CLOUD_ROOT_PATH: &str = "/cloud";
#[cfg(windows)]
const SOURCE_PATH: &str = r"C:\source\report.pdf";
#[cfg(not(windows))]
const SOURCE_PATH: &str = "/source/report.pdf";
#[cfg(windows)]
const DESTINATION_PATH: &str = r"C:\cloud\DiskSage Archive\report.pdf";
#[cfg(not(windows))]
const DESTINATION_PATH: &str = "/cloud/DiskSage Archive/report.pdf";

/// Builds a safe candidate whose organization signals can be varied independently.
fn candidate(
    destination_account_scope: CloudAccountScope,
    organization_review_reason_present: bool,
) -> CloudCandidate {
    let mut review_reasons = vec!["embedded-metadata-probe-incomplete".into()];
    if organization_review_reason_present {
        review_reasons.push(ORGANIZATION_TENANT_AUTHORITY_REVIEW_REASON.into());
    }
    let mut candidate = CloudCandidate {
        metadata_fingerprint: "a".repeat(64),
        review_fingerprint: String::new(),
        src: SOURCE_PATH.into(),
        dst: DESTINATION_PATH.into(),
        provider: CloudProvider::Icloud,
        destination_account_scope,
        kind: ArchiveKind::Document,
        bytes: 12,
        age_days: 90,
        created_ms: 1,
        modified_ms: 2,
        production_time_ms: 3,
        production_time_source: "embedded:exiftool:CreateDate".into(),
        production_time_confidence: "high".into(),
        source_root: SOURCE_PATH.into(),
        relative_path: "report.pdf".into(),
        source_context: "source".into(),
        requires_review: true,
        review_reasons,
        content_title: Some("Report".into()),
        content_authors: vec!["Author".into()],
        content_context: vec!["Context".into()],
        duration_ms: None,
        dataset_profile: None,
        metadata_evidence: vec![MetadataEvidence {
            field: "production_time".into(),
            value: "2026-01-01".into(),
            source: "exiftool:CreateDate".into(),
            confidence: "high".into(),
        }],
        blocked_reason: None,
    };
    candidate.review_fingerprint = candidate_review_fingerprint(&candidate);
    candidate
}

/// Builds a cloud root whose account scope matches the candidate under test.
fn cloud_root(account_scope: CloudAccountScope) -> CloudRoot {
    CloudRoot {
        id: "icloud:test".into(),
        provider: CloudProvider::Icloud,
        account_scope,
        label: "iCloud Drive".into(),
        path: CLOUD_ROOT_PATH.into(),
        readable: true,
        access_issue: None,
    }
}

/// Returns whether a non-attested approval remains blocked by tenant authority.
fn unconfirmed_approval_is_blocked(
    destination_account_scope: CloudAccountScope,
    organization_review_reason_present: bool,
) -> bool {
    let candidate = candidate(
        destination_account_scope,
        organization_review_reason_present,
    );
    let decision = create_attributed_decision(
        &candidate,
        CloudReviewDisposition::Approved,
        10,
        "human:local:reviewer",
        "Metadata, account scope, and destination reviewed.",
    )
    .expect("the test decision must be structurally valid");
    candidate_blockers_with_review(
        &candidate,
        &cloud_root(destination_account_scope),
        Some(&decision),
    )
    .iter()
    .any(|blocker| blocker == ATTESTATION_BLOCKER)
}

#[test]
fn rust_transfer_gate_requires_authority_when_either_organization_signal_is_present() {
    let cases = [
        ("scope-only", CloudAccountScope::Organization, false, true),
        ("reason-only", CloudAccountScope::Personal, true, true),
        ("both", CloudAccountScope::Organization, true, true),
        ("unknown-with-reason", CloudAccountScope::Unknown, true, true),
        ("neither", CloudAccountScope::Personal, false, false),
    ];

    for (label, account_scope, organization_reason_present, expected_blocked) in cases {
        assert_eq!(
            unconfirmed_approval_is_blocked(account_scope, organization_reason_present),
            expected_blocked,
            "unexpected tenant-authority result for {label}",
        );
    }
}
