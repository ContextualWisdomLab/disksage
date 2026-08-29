#![cfg(unix)]

use disksage_lib::cloud_review::{
    write_immutable_decision, CloudReviewDecision, CloudReviewDisposition, DECISION_VERSION,
};
use std::os::unix::fs::PermissionsExt;

fn valid_decision() -> CloudReviewDecision {
    let candidate_fingerprint = "a".repeat(64);
    let review_fingerprint = "b".repeat(64);
    let reviewed_at_ms = 1_786_490_000_000u64;
    let reviewed_by = "human:runtime-test";
    let rationale = "Verified exact review evidence before durable publication.";

    let mut hasher = blake3::Hasher::new();
    hasher.update(&DECISION_VERSION.to_le_bytes());
    hasher.update(candidate_fingerprint.as_bytes());
    hasher.update(&[0]);
    hasher.update(review_fingerprint.as_bytes());
    hasher.update(&[0]);
    hasher.update(b"approved");
    hasher.update(&reviewed_at_ms.to_le_bytes());
    hasher.update(reviewed_by.as_bytes());
    hasher.update(&[0]);
    hasher.update(rationale.as_bytes());

    CloudReviewDecision {
        version: DECISION_VERSION,
        decision_id: hasher.finalize().to_hex().to_string(),
        candidate_fingerprint,
        review_fingerprint,
        disposition: CloudReviewDisposition::Approved,
        reviewed_at_ms,
        reviewed_by: reviewed_by.into(),
        rationale: rationale.into(),
    }
}

#[test]
fn immutable_review_decision_is_owner_read_only_and_create_once_at_runtime() {
    let directory = tempfile::tempdir().unwrap();
    let decision = valid_decision();

    let path = write_immutable_decision(directory.path(), &decision).unwrap();
    let metadata = std::fs::symlink_metadata(&path).unwrap();
    assert!(metadata.is_file());
    assert_eq!(metadata.permissions().mode() & 0o777, 0o400);
    assert!(metadata.permissions().readonly());

    let stored: CloudReviewDecision =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(stored, decision);

    assert_eq!(
        write_immutable_decision(directory.path(), &decision).unwrap_err(),
        "cloud-review-decision-create-failed"
    );
}
