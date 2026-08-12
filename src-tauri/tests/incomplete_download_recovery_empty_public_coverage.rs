//! Credential-free coverage for incomplete-download recovery admission and empty-report behavior.
//!
//! These tests never recover, rename, extract, or discard data. They exercise fail-closed public
//! validation boundaries and the deterministic no-candidate aggregation path.

use disksage_lib::incomplete_download::{
    IncompleteDownloadAuditReport, DEFAULT_STALE_AFTER_DAYS, INCOMPLETE_DOWNLOAD_AUDIT_VERSION,
};
use disksage_lib::incomplete_download_recovery::{
    summarize_incomplete_download_recovery, validate_incomplete_download_recovery,
    RecoveryValidationLimits, DEFAULT_MAX_PNG_OUTPUT_BYTES, DEFAULT_MAX_ZIP_ENTRIES,
    DEFAULT_MAX_ZIP_SINGLE_UNCOMPRESSED_BYTES, DEFAULT_MAX_ZIP_TOTAL_UNCOMPRESSED_BYTES,
    MAX_PNG_OUTPUT_BYTES, MAX_ZIP_ENTRIES, MAX_ZIP_SINGLE_UNCOMPRESSED_BYTES,
    MAX_ZIP_TOTAL_UNCOMPRESSED_BYTES,
};
use std::collections::BTreeMap;
use std::path::Path;

fn empty_audit(source_root: String, evidence_complete: bool) -> IncompleteDownloadAuditReport {
    IncompleteDownloadAuditReport {
        schema_version: INCOMPLETE_DOWNLOAD_AUDIT_VERSION,
        observed_at_ms: 1,
        source_root,
        source_scope_fingerprint: "a".repeat(64),
        stale_after_days: DEFAULT_STALE_AFTER_DAYS,
        evidence_complete,
        entries_seen: 0,
        issue_counts: BTreeMap::new(),
        file_count: 0,
        logical_bytes: 0,
        allocated_bytes: 0,
        active_count: 0,
        active_bytes: 0,
        evidence_incomplete_count: 0,
        evidence_incomplete_bytes: 0,
        recent_idle_count: 0,
        recent_idle_bytes: 0,
        stale_idle_count: 0,
        stale_idle_bytes: 0,
        recovery_candidate_count: 0,
        recovery_candidate_bytes: 0,
        structural_zip_candidate_item_count: 0,
        structural_zip_recoverable_bytes: 0,
        whole_file_structurally_complete_zip_count: 0,
        whole_file_structurally_complete_zip_bytes: 0,
        detected_type_count: 0,
        acquisition_date_evidence_count: 0,
        production_time_evidence_count: 0,
        final_sibling_count: 0,
        discard_review_bytes: 0,
        audit_fingerprint: "b".repeat(64),
        mutation_performed: false,
        items: Vec::new(),
    }
}

#[test]
fn recovery_limits_reject_zero_oversized_and_internally_inconsistent_bounds_first() {
    let audit = empty_audit("unused".into(), true);
    let root = Path::new("relative-root-is-not-reached");

    let cases = [
        (
            RecoveryValidationLimits {
                max_zip_entries: 0,
                ..RecoveryValidationLimits::default()
            },
            "recovery-validation-zip-entry-limit-out-of-range",
        ),
        (
            RecoveryValidationLimits {
                max_zip_entries: MAX_ZIP_ENTRIES + 1,
                ..RecoveryValidationLimits::default()
            },
            "recovery-validation-zip-entry-limit-out-of-range",
        ),
        (
            RecoveryValidationLimits {
                max_zip_total_uncompressed_bytes: 0,
                ..RecoveryValidationLimits::default()
            },
            "recovery-validation-zip-total-limit-out-of-range",
        ),
        (
            RecoveryValidationLimits {
                max_zip_total_uncompressed_bytes: MAX_ZIP_TOTAL_UNCOMPRESSED_BYTES + 1,
                ..RecoveryValidationLimits::default()
            },
            "recovery-validation-zip-total-limit-out-of-range",
        ),
        (
            RecoveryValidationLimits {
                max_zip_single_uncompressed_bytes: 0,
                ..RecoveryValidationLimits::default()
            },
            "recovery-validation-zip-single-limit-out-of-range",
        ),
        (
            RecoveryValidationLimits {
                max_zip_single_uncompressed_bytes: MAX_ZIP_SINGLE_UNCOMPRESSED_BYTES + 1,
                max_zip_total_uncompressed_bytes: MAX_ZIP_TOTAL_UNCOMPRESSED_BYTES,
                ..RecoveryValidationLimits::default()
            },
            "recovery-validation-zip-single-limit-out-of-range",
        ),
        (
            RecoveryValidationLimits {
                max_zip_total_uncompressed_bytes: 1024,
                max_zip_single_uncompressed_bytes: 1025,
                ..RecoveryValidationLimits::default()
            },
            "recovery-validation-zip-single-limit-out-of-range",
        ),
        (
            RecoveryValidationLimits {
                max_png_output_bytes: 0,
                ..RecoveryValidationLimits::default()
            },
            "recovery-validation-png-output-limit-out-of-range",
        ),
        (
            RecoveryValidationLimits {
                max_png_output_bytes: MAX_PNG_OUTPUT_BYTES + 1,
                ..RecoveryValidationLimits::default()
            },
            "recovery-validation-png-output-limit-out-of-range",
        ),
    ];

    for (limits, expected) in cases {
        assert_eq!(
            validate_incomplete_download_recovery(root, &audit, 2, limits).unwrap_err(),
            expected
        );
    }
}

#[test]
fn recovery_root_and_audit_authority_fail_closed_before_candidate_validation() {
    let defaults = RecoveryValidationLimits::default();
    assert_eq!(defaults.max_zip_entries, DEFAULT_MAX_ZIP_ENTRIES);
    assert_eq!(
        defaults.max_zip_total_uncompressed_bytes,
        DEFAULT_MAX_ZIP_TOTAL_UNCOMPRESSED_BYTES
    );
    assert_eq!(
        defaults.max_zip_single_uncompressed_bytes,
        DEFAULT_MAX_ZIP_SINGLE_UNCOMPRESSED_BYTES
    );
    assert_eq!(defaults.max_png_output_bytes, DEFAULT_MAX_PNG_OUTPUT_BYTES);

    let audit = empty_audit("unused".into(), true);
    assert_eq!(
        validate_incomplete_download_recovery(Path::new("relative"), &audit, 2, defaults)
            .unwrap_err(),
        "recovery-validation-root-must-be-absolute"
    );

    let temp = tempfile::tempdir().unwrap();
    let missing = temp.path().join("missing");
    assert_eq!(
        validate_incomplete_download_recovery(&missing, &audit, 2, defaults).unwrap_err(),
        "recovery-validation-root-unavailable"
    );

    let regular_file = temp.path().join("regular-file");
    std::fs::write(&regular_file, b"fixture").unwrap();
    let file_audit = empty_audit(regular_file.to_string_lossy().into_owned(), true);
    assert_eq!(
        validate_incomplete_download_recovery(&regular_file, &file_audit, 2, defaults).unwrap_err(),
        "recovery-validation-root-unsafe"
    );

    let canonical = std::fs::canonicalize(temp.path()).unwrap();
    let mismatched = empty_audit("/definitely/not/the/canonical/root".into(), true);
    assert_eq!(
        validate_incomplete_download_recovery(temp.path(), &mismatched, 2, defaults).unwrap_err(),
        "recovery-validation-audit-root-mismatch"
    );

    let mut mutating = empty_audit(canonical.to_string_lossy().into_owned(), true);
    mutating.mutation_performed = true;
    assert_eq!(
        validate_incomplete_download_recovery(temp.path(), &mutating, 2, defaults).unwrap_err(),
        "recovery-validation-rejects-mutating-audit"
    );
}

#[test]
fn empty_recovery_report_is_deterministic_redacted_and_never_grants_mutation_authority() {
    let temp = tempfile::tempdir().unwrap();
    let canonical = std::fs::canonicalize(temp.path()).unwrap();
    let audit = empty_audit(canonical.to_string_lossy().into_owned(), true);

    let report = validate_incomplete_download_recovery(
        temp.path(),
        &audit,
        42,
        RecoveryValidationLimits::default(),
    )
    .unwrap();

    assert!(report.evidence_complete);
    assert_eq!(report.candidate_count, 0);
    assert_eq!(report.candidate_bytes, 0);
    assert_eq!(report.fully_validated_file_count, 0);
    assert_eq!(report.partially_validated_file_count, 0);
    assert_eq!(report.validated_recoverable_bytes, 0);
    assert_eq!(report.invalid_count, 0);
    assert_eq!(report.limit_exceeded_count, 0);
    assert_eq!(report.unsupported_count, 0);
    assert_eq!(report.skipped_count, 0);
    assert!(report.issue_counts.is_empty());
    assert!(report.items.is_empty());
    assert_eq!(report.validation_fingerprint.len(), 64);
    assert!(!report.mutation_performed);

    let summary = summarize_incomplete_download_recovery(&report);
    assert_eq!(
        summary.output_mode,
        "incomplete-download-recovery-validation-summary"
    );
    assert!(!summary.human_recovery_action_required);
    assert!(!summary.automatic_rename_allowed);
    assert!(!summary.automatic_discard_allowed);
    assert!(!summary.mutation_performed);
    assert!(summary.items.is_empty());
    assert!(summary.redacted_from_summary.contains(&"absolute-source-root".into()));
    let encoded = serde_json::to_string(&summary).unwrap();
    assert!(!encoded.contains(&audit.source_root));
}

#[test]
fn incomplete_audit_keeps_empty_recovery_evidence_incomplete() {
    let temp = tempfile::tempdir().unwrap();
    let canonical = std::fs::canonicalize(temp.path()).unwrap();
    let audit = empty_audit(canonical.to_string_lossy().into_owned(), false);

    let report = validate_incomplete_download_recovery(
        temp.path(),
        &audit,
        43,
        RecoveryValidationLimits::default(),
    )
    .unwrap();

    assert!(!report.audit_evidence_complete);
    assert!(!report.evidence_complete);
    assert_eq!(report.candidate_count, 0);
    assert_eq!(report.validation_fingerprint.len(), 64);
}