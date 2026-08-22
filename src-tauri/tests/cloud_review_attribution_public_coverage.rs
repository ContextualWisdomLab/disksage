//! Public-contract coverage for cloud-review attribution and organization-tenant authority.
//!
//! These tests exercise the production validation boundary without reading or writing review
//! records. They keep agent identities out of the `human:` namespace and reject invisible or
//! punctuation-only rationales before any cloud plan can consume a decision.

use disksage_lib::cloud_review::{
    organization_tenant_authority_attested, validate_review_attribution, CloudReviewDecision,
    CloudReviewDisposition, DECISION_VERSION, ORGANIZATION_TENANT_AUTHORITY_ATTESTATION,
};

fn decision(disposition: CloudReviewDisposition, rationale: &str) -> CloudReviewDecision {
    CloudReviewDecision {
        version: DECISION_VERSION,
        decision_id: "a".repeat(64),
        candidate_fingerprint: "b".repeat(64),
        review_fingerprint: "c".repeat(64),
        disposition,
        reviewed_at_ms: 42,
        reviewed_by: "human:security.reviewer@example.com".into(),
        rationale: rationale.into(),
    }
}

#[test]
fn attribution_accepts_bounded_human_identity_and_meaningful_rationale() {
    assert_eq!(
        validate_review_attribution(
            "human:security.reviewer@example.com",
            "Reviewed the tenant authority and supporting evidence.",
        ),
        Ok(())
    );
}

#[test]
fn attribution_rejects_non_human_unbounded_or_ambiguous_identity() {
    let too_long = format!("human:{}", "a".repeat(129));
    for reviewed_by in [
        "",
        "human:",
        "agent:reviewer",
        " human:reviewer",
        "human:reviewer ",
        "human:review er",
        "human:reviewer\n",
        too_long.as_str(),
    ] {
        assert_eq!(
            validate_review_attribution(reviewed_by, "Reviewed evidence 123.").unwrap_err(),
            "cloud-review-decision-attribution-invalid",
            "unexpectedly accepted reviewer identity {reviewed_by:?}",
        );
    }
}

#[test]
fn attribution_rejects_empty_invisible_format_only_or_unbounded_rationale() {
    let too_long = "a".repeat(1_001);
    for rationale in [
        "",
        "   ",
        "... -- ...",
        " leading rationale",
        "trailing rationale ",
        "contains\ncontrol",
        "abc\u{202e}def",
        "\u{200b}hidden-edge",
        too_long.as_str(),
    ] {
        assert_eq!(
            validate_review_attribution("human:reviewer", rationale).unwrap_err(),
            "cloud-review-decision-attribution-invalid",
            "unexpectedly accepted rationale {rationale:?}",
        );
    }
}

#[test]
fn organization_authority_requires_approved_prefixed_meaningful_attestation() {
    let approved = decision(
        CloudReviewDisposition::Approved,
        &format!(
            "{ORGANIZATION_TENANT_AUTHORITY_ATTESTATION} Tenant owner confirmed account scope 7."
        ),
    );
    assert!(organization_tenant_authority_attested(&approved));

    let held = decision(CloudReviewDisposition::Held, &approved.rationale);
    assert!(!organization_tenant_authority_attested(&held));

    let missing_space = decision(
        CloudReviewDisposition::Approved,
        &format!("{ORGANIZATION_TENANT_AUTHORITY_ATTESTATION}confirmed"),
    );
    assert!(!organization_tenant_authority_attested(&missing_space));

    let punctuation_only = decision(
        CloudReviewDisposition::Approved,
        &format!("{ORGANIZATION_TENANT_AUTHORITY_ATTESTATION} ..."),
    );
    assert!(!organization_tenant_authority_attested(&punctuation_only));

    let unrelated = decision(CloudReviewDisposition::Approved, "Tenant owner confirmed scope.");
    assert!(!organization_tenant_authority_attested(&unrelated));
}
