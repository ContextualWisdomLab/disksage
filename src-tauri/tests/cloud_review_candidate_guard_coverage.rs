//! Public-contract coverage for cloud-review candidate admission guards.
//!
//! Candidate decisions must not be created for review-free, malformed, or stale evidence. These
//! tests stop before persistence and exercise the same production admission boundary used by both
//! legacy and attributed decisions.

use disksage_lib::cloud::{
    ArchiveKind, CloudAccountScope, CloudCandidate, CloudProvider,
};
use disksage_lib::cloud_review::{
    create_attributed_decision, create_decision, CloudReviewDisposition,
};

fn candidate() -> CloudCandidate {
    CloudCandidate {
        metadata_fingerprint: "a".repeat(64),
        review_fingerprint: "b".repeat(64),
        src: "/source/report.pdf".into(),
        dst: "/cloud/report.pdf".into(),
        provider: CloudProvider::Onedrive,
        destination_account_scope: CloudAccountScope::Personal,
        kind: ArchiveKind::Document,
        bytes: 42,
        age_days: 120,
        created_ms: 10,
        modified_ms: 20,
        production_time_ms: 10,
        production_time_source: "filesystem-created".into(),
        production_time_confidence: "medium".into(),
        source_root: "/source".into(),
        relative_path: "report.pdf".into(),
        source_context: "documents".into(),
        requires_review: true,
        review_reasons: vec!["metadata-needs-review".into()],
        content_title: Some("Report".into()),
        content_authors: vec!["Author".into()],
        content_context: vec!["Quarterly".into()],
        duration_ms: None,
        dataset_profile: None,
        metadata_evidence: Vec::new(),
        blocked_reason: None,
    }
}

#[test]
fn review_free_candidates_cannot_receive_legacy_or_attributed_decisions() {
    let mut candidate = candidate();
    candidate.requires_review = false;

    assert_eq!(
        create_decision(&candidate, CloudReviewDisposition::Approved, 100).unwrap_err(),
        "cloud-review-not-required"
    );
    assert_eq!(
        create_attributed_decision(
            &candidate,
            CloudReviewDisposition::Approved,
            100,
            "human:reviewer",
            "Reviewed evidence 123.",
        )
        .unwrap_err(),
        "cloud-review-not-required"
    );
}

#[test]
fn malformed_candidate_fingerprints_fail_closed_before_decision_creation() {
    let mut candidate = candidate();
    candidate.metadata_fingerprint = "not-a-valid-fingerprint".into();

    assert_eq!(
        create_decision(&candidate, CloudReviewDisposition::Held, 101).unwrap_err(),
        "cloud-review-candidate-fingerprint-invalid"
    );
    assert_eq!(
        create_attributed_decision(
            &candidate,
            CloudReviewDisposition::Held,
            101,
            "human:reviewer",
            "Held pending evidence 456.",
        )
        .unwrap_err(),
        "cloud-review-candidate-fingerprint-invalid"
    );
}

#[test]
fn stale_review_fingerprint_cannot_be_reused_for_either_decision_version() {
    let candidate = candidate();

    assert_eq!(
        create_decision(&candidate, CloudReviewDisposition::Approved, 102).unwrap_err(),
        "cloud-review-fingerprint-mismatch"
    );
    assert_eq!(
        create_attributed_decision(
            &candidate,
            CloudReviewDisposition::Approved,
            102,
            "human:reviewer",
            "Reviewed fresh metadata evidence 789.",
        )
        .unwrap_err(),
        "cloud-review-fingerprint-mismatch"
    );
}
