//! Security regressions for caller-supplied root-object admission.
//!
//! These tests use only temporary local directories and symbolic links. They prove that public
//! read-only audit, recovery, and materialization entry points reject a symlink root before
//! canonicalization can erase the caller-supplied filesystem object's identity. No test mutates,
//! renames, recovers, extracts, uploads, or discards user data.

#![cfg(unix)]

use disksage_lib::duplicate_audit::collect_exact_duplicate_audit;
use disksage_lib::incomplete_download::{
    collect_incomplete_download_audit, DEFAULT_STALE_AFTER_DAYS,
};
use disksage_lib::incomplete_download_materialization::plan_incomplete_download_materialization;
use disksage_lib::incomplete_download_recovery::{
    validate_incomplete_download_recovery, RecoveryValidationLimits,
};
use disksage_lib::multipart_archive::collect_multipart_archive_audit;
use std::os::unix::fs::symlink;

#[test]
fn public_read_only_roots_reject_symlink_objects_before_canonicalization() {
    let fixture = tempfile::tempdir().expect("temporary symlink-admission fixture");
    let real_root = fixture.path().join("real-root");
    let symlink_root = fixture.path().join("symlink-root");
    std::fs::create_dir(&real_root).expect("real root");
    symlink(&real_root, &symlink_root).expect("symlink root");

    assert_eq!(
        collect_exact_duplicate_audit(&symlink_root, 1, 1, 100).unwrap_err(),
        "duplicate-audit-root-unsafe"
    );
    assert_eq!(
        collect_multipart_archive_audit(&symlink_root, 1, 100).unwrap_err(),
        "multipart-audit-root-unsafe"
    );
    assert_eq!(
        collect_incomplete_download_audit(
            &symlink_root,
            1,
            100,
            DEFAULT_STALE_AFTER_DAYS,
        )
        .unwrap_err(),
        "incomplete-download-audit-root-unsafe"
    );

    // Build valid lineage from the real directory so the symlink-root assertions exercise root
    // admission rather than an unrelated audit/recovery integrity failure.
    let audit = collect_incomplete_download_audit(
        &real_root,
        2,
        100,
        DEFAULT_STALE_AFTER_DAYS,
    )
    .expect("read-only audit lineage");
    assert_eq!(
        validate_incomplete_download_recovery(
            &symlink_root,
            &audit,
            3,
            RecoveryValidationLimits::default(),
        )
        .unwrap_err(),
        "recovery-validation-root-unsafe"
    );

    let recovery = validate_incomplete_download_recovery(
        &real_root,
        &audit,
        4,
        RecoveryValidationLimits::default(),
    )
    .expect("read-only recovery lineage");
    assert_eq!(
        plan_incomplete_download_materialization(&symlink_root, &audit, &recovery, 5).unwrap_err(),
        "materialization-root-unsafe"
    );
}
