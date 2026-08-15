//! Integration coverage for public incomplete-download recovery admission and summary boundaries.
//!
//! Fixtures use empty local temporary directories only. Validation remains read-only and the tests
//! do not extract, rename, discard, or grant mutation authority to any candidate.

use disksage_lib::incomplete_download::{
    collect_incomplete_download_audit, DEFAULT_STALE_AFTER_DAYS,
};
use disksage_lib::incomplete_download_recovery::{
    summarize_incomplete_download_recovery, validate_incomplete_download_recovery,
    RecoveryValidationLimits, MAX_PNG_OUTPUT_BYTES, MAX_ZIP_ENTRIES,
    MAX_ZIP_SINGLE_UNCOMPRESSED_BYTES, MAX_ZIP_TOTAL_UNCOMPRESSED_BYTES,
};
use std::path::Path;

fn empty_audit(root: &Path) -> disksage_lib::incomplete_download::IncompleteDownloadAuditReport {
    collect_incomplete_download_audit(root, 41, 100, DEFAULT_STALE_AFTER_DAYS)
        .expect("empty read-only incomplete-download audit")
}

#[test]
fn validation_rejects_every_out_of_range_public_limit() {
    let root = tempfile::tempdir().expect("temporary recovery root");
    let audit = empty_audit(root.path());
    let defaults = RecoveryValidationLimits::default();

    for (limits, expected) in [
        (
            RecoveryValidationLimits {
                max_zip_entries: 0,
                ..defaults
            },
            "recovery-validation-zip-entry-limit-out-of-range",
        ),
        (
            RecoveryValidationLimits {
                max_zip_entries: MAX_ZIP_ENTRIES + 1,
                ..defaults
            },
            "recovery-validation-zip-entry-limit-out-of-range",
        ),
        (
            RecoveryValidationLimits {
                max_zip_total_uncompressed_bytes: 0,
                ..defaults
            },
            "recovery-validation-zip-total-limit-out-of-range",
        ),
        (
            RecoveryValidationLimits {
                max_zip_total_uncompressed_bytes: MAX_ZIP_TOTAL_UNCOMPRESSED_BYTES + 1,
                ..defaults
            },
            "recovery-validation-zip-total-limit-out-of-range",
        ),
        (
            RecoveryValidationLimits {
                max_zip_single_uncompressed_bytes: 0,
                ..defaults
            },
            "recovery-validation-zip-single-limit-out-of-range",
        ),
        (
            RecoveryValidationLimits {
                max_zip_single_uncompressed_bytes: MAX_ZIP_SINGLE_UNCOMPRESSED_BYTES + 1,
                ..defaults
            },
            "recovery-validation-zip-single-limit-out-of-range",
        ),
        (
            RecoveryValidationLimits {
                max_zip_total_uncompressed_bytes: 1,
                max_zip_single_uncompressed_bytes: 2,
                ..defaults
            },
            "recovery-validation-zip-single-limit-out-of-range",
        ),
        (
            RecoveryValidationLimits {
                max_png_output_bytes: 0,
                ..defaults
            },
            "recovery-validation-png-output-limit-out-of-range",
        ),
        (
            RecoveryValidationLimits {
                max_png_output_bytes: MAX_PNG_OUTPUT_BYTES + 1,
                ..defaults
            },
            "recovery-validation-png-output-limit-out-of-range",
        ),
    ] {
        assert_eq!(
            validate_incomplete_download_recovery(root.path(), &audit, 42, limits).unwrap_err(),
            expected
        );
    }
}

#[test]
fn validation_rejects_unsafe_roots_and_stale_audit_authority() {
    let root = tempfile::tempdir().expect("temporary recovery root");
    let audit = empty_audit(root.path());
    let limits = RecoveryValidationLimits::default();

    assert_eq!(
        validate_incomplete_download_recovery(Path::new("relative-root"), &audit, 42, limits)
            .unwrap_err(),
        "recovery-validation-root-must-be-absolute"
    );
    assert_eq!(
        validate_incomplete_download_recovery(&root.path().join("missing"), &audit, 42, limits)
            .unwrap_err(),
        "recovery-validation-root-unavailable"
    );

    let regular_file = root.path().join("not-a-directory.bin");
    std::fs::write(&regular_file, b"ordinary file").expect("regular-file root fixture");
    assert_eq!(
        validate_incomplete_download_recovery(&regular_file, &audit, 42, limits).unwrap_err(),
        "recovery-validation-root-unsafe"
    );

    let mut wrong_root = audit.clone();
    wrong_root.source_root = "/different-canonical-root".into();
    assert_eq!(
        validate_incomplete_download_recovery(root.path(), &wrong_root, 42, limits).unwrap_err(),
        "recovery-validation-audit-root-mismatch"
    );

    let mut mutating_audit = audit.clone();
    mutating_audit.mutation_performed = true;
    assert_eq!(
        validate_incomplete_download_recovery(root.path(), &mutating_audit, 42, limits)
            .unwrap_err(),
        "recovery-validation-rejects-mutating-audit"
    );
}

#[test]
fn empty_recovery_is_complete_read_only_and_privacy_redacted() {
    let root = tempfile::tempdir().expect("temporary recovery root");
    let audit = empty_audit(root.path());
    let report = validate_incomplete_download_recovery(
        root.path(),
        &audit,
        42,
        RecoveryValidationLimits::default(),
    )
    .expect("empty recovery validation");

    assert!(report.audit_evidence_complete);
    assert!(report.evidence_complete);
    assert_eq!(report.observed_at_ms, 42);
    assert_eq!(report.candidate_count, 0);
    assert_eq!(report.candidate_bytes, 0);
    assert_eq!(report.validated_recoverable_bytes, 0);
    assert!(report.issue_counts.is_empty());
    assert!(!report.mutation_performed);

    let summary = summarize_incomplete_download_recovery(&report);
    assert!(!summary.human_recovery_action_required);
    assert!(!summary.automatic_rename_allowed);
    assert!(!summary.automatic_discard_allowed);
    assert!(!summary.mutation_performed);
    assert_eq!(summary.notices.len(), 8);
    for redacted in [
        "absolute-source-root",
        "relative-file-path",
        "zip-range-offsets",
        "entry-paths",
        "active-process-identifiers",
    ] {
        assert!(summary.redacted_from_summary.contains(&redacted.to_string()));
    }
    let encoded = serde_json::to_string(&summary).expect("summary JSON");
    assert!(!encoded.contains(root.path().to_string_lossy().as_ref()));
}
