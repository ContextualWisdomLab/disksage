//! Integration coverage for the durable organization-tenant authorization boundary.
//!
//! These tests exercise the public transfer gate rather than duplicating its predicate. They
//! prove that either canonical organization signal is sufficient to require an explicit human
//! tenant-authority attestation before a cloud copy can proceed.

use disksage_lib::cloud::{
    candidate_review_fingerprint, ArchiveKind, CloudAccountScope, CloudCandidate, CloudProvider,
    CloudRoot, MetadataEvidence, ORGANIZATION_TENANT_AUTHORITY_REVIEW_REASON,
};
use disksage_lib::cloud_review::{create_attributed_decision, CloudReviewDisposition};
use disksage_lib::cloud_transfer::candidate_blockers_with_review;

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

/// Build a cloud root whose account scope exactly matches the candidate under test.
fn cloud_root(account_scope: CloudAccountScope) -> CloudRoot {
    CloudRoot {
        id: format!("icloud:{}", account_scope.as_str()),
        provider: CloudProvider::Icloud,
        account_scope,
        label: "iCloud Drive".into(),
        path: CLOUD_ROOT_PATH.into(),
        readable: true,
        access_issue: None,
    }
}

/// Build a realistic, otherwise eligible candidate with the requested organization signals.
fn candidate(
    destination_account_scope: CloudAccountScope,
    review_reasons: &[&str],
    requires_review: bool,
) -> CloudCandidate {
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
        requires_review,
        review_reasons: review_reasons.iter().map(|reason| (*reason).into()).collect(),
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

/// Create an exact approved review decision that deliberately lacks tenant-authority attestation.
fn unconfirmed_decision(candidate: &CloudCandidate) -> disksage_lib::cloud_review::CloudReviewDecision {
    create_attributed_decision(
        candidate,
        CloudReviewDisposition::Approved,
        100,
        "human:integration-reviewer",
        "Candidate metadata and destination were reviewed without organization tenant authority.",
    )
    .expect("the realistic review decision should be valid")
}

#[test]
fn either_organization_signal_requires_explicit_tenant_authority_attestation() {
    let cases = [
        (
            "organization scope only",
            CloudAccountScope::Organization,
            vec!["embedded-metadata-probe-incomplete"],
            true,
        ),
        (
            "organization reason only",
            CloudAccountScope::Personal,
            vec![ORGANIZATION_TENANT_AUTHORITY_REVIEW_REASON],
            true,
        ),
        (
            "both canonical signals",
            CloudAccountScope::Organization,
            vec![ORGANIZATION_TENANT_AUTHORITY_REVIEW_REASON],
            true,
        ),
        (
            "shared scope with organization reason",
            CloudAccountScope::Shared,
            vec![ORGANIZATION_TENANT_AUTHORITY_REVIEW_REASON],
            true,
        ),
        (
            "unknown scope with organization reason",
            CloudAccountScope::Unknown,
            vec![ORGANIZATION_TENANT_AUTHORITY_REVIEW_REASON],
            true,
        ),
        (
            "neither organization signal",
            CloudAccountScope::Personal,
            vec!["embedded-metadata-probe-incomplete"],
            false,
        ),
    ];

    for (label, scope, reasons, tenant_authority_required) in cases {
        let candidate = candidate(scope, &reasons, true);
        let decision = unconfirmed_decision(&candidate);
        let blockers =
            candidate_blockers_with_review(&candidate, &cloud_root(scope), Some(&decision));
        let blocked = blockers
            .iter()
            .any(|blocker| blocker == "organization-tenant-authority-attestation-required");
        assert_eq!(
            blocked, tenant_authority_required,
            "{label} produced blockers: {blockers:?}"
        );
    }
}

#[test]
fn organization_signals_require_tenant_authority_even_without_ordinary_review() {
    let cases = [
        (
            "organization scope without ordinary review",
            CloudAccountScope::Organization,
            vec!["embedded-metadata-probe-incomplete"],
        ),
        (
            "organization reason without ordinary review",
            CloudAccountScope::Personal,
            vec![ORGANIZATION_TENANT_AUTHORITY_REVIEW_REASON],
        ),
    ];

    for (label, scope, reasons) in cases {
        let candidate = candidate(scope, &reasons, false);
        let blockers = candidate_blockers_with_review(&candidate, &cloud_root(scope), None);
        assert!(
            blockers
                .iter()
                .any(|blocker| blocker == "organization-tenant-authority-attestation-required"),
            "{label} must fail closed without tenant-authority attestation: {blockers:?}"
        );
    }
}
