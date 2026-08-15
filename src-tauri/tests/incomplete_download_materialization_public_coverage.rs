//! Integration coverage for public incomplete-download materialization admission boundaries.
//!
//! The fixtures are empty local temporary directories and in-memory reports. Planning stays
//! destination-independent and performs no extraction, rename, discard, or cloud write.

use disksage_lib::incomplete_download::{
    collect_incomplete_download_audit, DEFAULT_STALE_AFTER_DAYS,
};
use disksage_lib::incomplete_download_materialization::plan_incomplete_download_materialization;
use disksage_lib::incomplete_download_recovery::{
    validate_incomplete_download_recovery, RecoveryValidationLimits,
};
use std::path::Path;

fn lineage(
    root: &Path,
) -> (
    disksage_lib::incomplete_download::IncompleteDownloadAuditReport,
    disksage_lib::incomplete_download_recovery::IncompleteDownloadRecoveryReport,
) {
    let audit = collect_incomplete_download_audit(root, 41, 100, DEFAULT_STALE_AFTER_DAYS)
        .expect("empty read-only incomplete-download audit");
    let recovery = validate_incomplete_download_recovery(
        root,
        &audit,
        42,
        RecoveryValidationLimits::default(),
    )
    .expect("empty read-only recovery validation");
    (audit, recovery)
}

#[test]
fn planning_rejects_unsafe_roots_and_lineage_mismatch() {
    let root = tempfile::tempdir().expect("temporary materialization root");
    let (audit, recovery) = lineage(root.path());

    assert_eq!(
        plan_incomplete_download_materialization(Path::new("relative-root"), &audit, &recovery, 43)
            .unwrap_err(),
        "materialization-root-must-be-absolute"
    );
    assert_eq!(
        plan_incomplete_download_materialization(
            &root.path().join("missing"),
            &audit,
            &recovery,
            43,
        )
        .unwrap_err(),
        "materialization-root-unavailable"
    );

    let regular_file = root.path().join("not-a-directory.bin");
    std::fs::write(&regular_file, b"ordinary file").expect("regular-file root fixture");
    assert_eq!(
        plan_incomplete_download_materialization(&regular_file, &audit, &recovery, 43).unwrap_err(),
        "materialization-root-unsafe"
    );

    let mut wrong_root = audit.clone();
    wrong_root.source_root = "/different-canonical-root".into();
    assert_eq!(
        plan_incomplete_download_materialization(root.path(), &wrong_root, &recovery, 43)
            .unwrap_err(),
        "materialization-audit-root-mismatch"
    );
}

#[test]
fn planning_rejects_tampered_audit_and_recovery_integrity() {
    let root = tempfile::tempdir().expect("temporary materialization root");
    let (audit, recovery) = lineage(root.path());

    let mut tampered_audit = audit.clone();
    tampered_audit.mutation_performed = true;
    assert_eq!(
        plan_incomplete_download_materialization(root.path(), &tampered_audit, &recovery, 43)
            .unwrap_err(),
        "materialization-audit-integrity-invalid"
    );

    let mut tampered_recovery = recovery.clone();
    tampered_recovery.validation_fingerprint = "0".repeat(64);
    assert_eq!(
        plan_incomplete_download_materialization(root.path(), &audit, &tampered_recovery, 43)
            .unwrap_err(),
        "materialization-recovery-integrity-invalid"
    );
}

#[test]
fn planning_refuses_to_invent_materialization_units_for_empty_recovery() {
    let root = tempfile::tempdir().expect("temporary materialization root");
    let (audit, recovery) = lineage(root.path());

    assert!(audit.evidence_complete);
    assert!(recovery.evidence_complete);
    assert_eq!(audit.recovery_candidate_count, 0);
    assert_eq!(recovery.candidate_count, 0);
    assert_eq!(
        plan_incomplete_download_materialization(root.path(), &audit, &recovery, 43).unwrap_err(),
        "materialization-unit-set-empty-or-duplicate"
    );
}
