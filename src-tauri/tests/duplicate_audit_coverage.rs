//! Coverage-focused integration regressions for exact duplicate audit fail-closed boundaries.
//!
//! These tests exercise public production behavior that exact-head coverage diagnostics showed was
//! still unobserved. They deliberately add no mutation authority and keep filesystem fixtures local
//! to temporary directories.

#![cfg(not(coverage))]

use disksage_lib::duplicate_audit::{
    collect_exact_duplicate_audit, exact_duplicate_audit_integrity_valid,
    summarize_exact_duplicate_audit, MAX_ENTRIES,
};
use std::path::Path;

#[test]
fn rejects_invalid_roots_and_limits_before_scanning() {
    assert_eq!(
        collect_exact_duplicate_audit(Path::new("relative"), 1, 1, 10).unwrap_err(),
        "duplicate-audit-root-must-be-absolute"
    );

    let root = tempfile::tempdir().unwrap();
    assert_eq!(
        collect_exact_duplicate_audit(root.path(), 1, 0, 10).unwrap_err(),
        "duplicate-audit-min-bytes-out-of-range"
    );
    assert_eq!(
        collect_exact_duplicate_audit(root.path(), 1, 1, 0).unwrap_err(),
        "duplicate-audit-max-entries-out-of-range"
    );
    assert_eq!(
        collect_exact_duplicate_audit(root.path(), 1, 1, MAX_ENTRIES + 1).unwrap_err(),
        "duplicate-audit-max-entries-out-of-range"
    );

    let missing = root.path().join("missing");
    assert_eq!(
        collect_exact_duplicate_audit(&missing, 1, 1, 10).unwrap_err(),
        "duplicate-audit-root-unavailable"
    );

    let file_root = root.path().join("not-a-directory.bin");
    std::fs::write(&file_root, b"content").unwrap();
    assert_eq!(
        collect_exact_duplicate_audit(&file_root, 1, 1, 10).unwrap_err(),
        "duplicate-audit-root-unsafe"
    );
}

#[test]
fn below_threshold_and_unique_sizes_do_not_trigger_hash_or_delete_authority() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("tiny.bin"), b"x").unwrap();
    std::fs::write(root.path().join("unique-a.bin"), b"1234").unwrap();
    std::fs::write(root.path().join("unique-b.bin"), b"123456").unwrap();

    let report = collect_exact_duplicate_audit(root.path(), 42, 2, 100).unwrap();
    assert!(report.evidence_complete);
    assert_eq!(report.file_count, 3);
    assert_eq!(report.size_collision_candidate_count, 0);
    assert_eq!(report.content_hashed_file_count, 0);
    assert_eq!(report.cluster_count, 0);
    assert_eq!(report.duplicate_file_count, 0);
    assert_eq!(report.logical_duplicate_bytes, 0);
    assert_eq!(report.logical_redundant_bytes, 0);
    assert!(exact_duplicate_audit_integrity_valid(&report));

    let summary = summarize_exact_duplicate_audit(&report);
    assert!(!summary.requires_human_canonical_selection);
    assert!(!summary.automatic_delete_allowed);
    assert!(!summary.mutation_performed);
}

#[test]
fn directory_depth_limit_marks_evidence_incomplete_without_descending() {
    let root = tempfile::tempdir().unwrap();
    let mut cursor = root.path().to_path_buf();
    for index in 0..=64 {
        cursor = cursor.join(format!("depth-{index}"));
        std::fs::create_dir(&cursor).unwrap();
    }
    std::fs::write(cursor.join("hidden.bin"), b"not-observed").unwrap();

    let report = collect_exact_duplicate_audit(root.path(), 42, 1, 1_000).unwrap();
    assert!(!report.evidence_complete);
    assert_eq!(
        report.issue_counts.get("duplicate-audit-depth-limit-reached"),
        Some(&1)
    );
    assert_eq!(report.file_count, 0);
    assert_eq!(report.cluster_count, 0);
    assert!(exact_duplicate_audit_integrity_valid(&report));
}

#[cfg(unix)]
#[test]
fn non_unicode_entry_is_reported_without_content_or_delete_authority() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let root = tempfile::tempdir().unwrap();
    let opaque_name = OsString::from_vec(vec![b'o', b'p', b'a', b'q', b'u', b'e', 0x80]);
    std::fs::write(root.path().join(opaque_name), b"same").unwrap();

    let report = collect_exact_duplicate_audit(root.path(), 42, 1, 100).unwrap();
    assert!(!report.evidence_complete);
    assert_eq!(report.file_count, 1);
    assert_eq!(report.content_hashed_file_count, 0);
    assert_eq!(report.cluster_count, 0);
    assert_eq!(
        report
            .issue_counts
            .get("duplicate-audit-relative-path-non-unicode"),
        Some(&1)
    );
    assert!(!report.automatic_delete_allowed);
    assert!(!report.mutation_performed);
    assert!(exact_duplicate_audit_integrity_valid(&report));
}

#[test]
fn integrity_rejects_public_report_authority_and_count_tampering() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("a.bin"), b"same").unwrap();
    std::fs::write(root.path().join("b.bin"), b"same").unwrap();
    let report = collect_exact_duplicate_audit(root.path(), 42, 1, 100).unwrap();
    assert!(exact_duplicate_audit_integrity_valid(&report));

    let mut tampered = report.clone();
    tampered.schema_version += 1;
    assert!(!exact_duplicate_audit_integrity_valid(&tampered));

    let mut tampered = report.clone();
    tampered.min_bytes = 0;
    assert!(!exact_duplicate_audit_integrity_valid(&tampered));

    let mut tampered = report.clone();
    tampered.max_entries = 0;
    assert!(!exact_duplicate_audit_integrity_valid(&tampered));

    let mut tampered = report.clone();
    tampered.production_metadata_evaluated = false;
    assert!(!exact_duplicate_audit_integrity_valid(&tampered));

    let mut tampered = report.clone();
    tampered.production_date_policy = "filesystem-only".into();
    assert!(!exact_duplicate_audit_integrity_valid(&tampered));

    let mut tampered = report.clone();
    tampered.physical_reclaimable_bytes = Some(1);
    assert!(!exact_duplicate_audit_integrity_valid(&tampered));

    let mut tampered = report.clone();
    tampered.exact_content_match_is_delete_approval = true;
    assert!(!exact_duplicate_audit_integrity_valid(&tampered));

    let mut tampered = report.clone();
    tampered.mutation_performed = true;
    assert!(!exact_duplicate_audit_integrity_valid(&tampered));

    let mut tampered = report;
    tampered.cluster_count += 1;
    assert!(!exact_duplicate_audit_integrity_valid(&tampered));
}
