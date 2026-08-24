//! Integration coverage for exact-duplicate audit admission and integrity boundaries.
//!
//! These tests exercise shipped public APIs with real temporary filesystem fixtures. They do not
//! weaken the exact-coverage threshold, mutate user data, or rely on source-text assertions.

#![cfg(not(coverage))]

use disksage_lib::duplicate_audit::{
    collect_exact_duplicate_audit, exact_duplicate_audit_integrity_valid,
    summarize_exact_duplicate_audit, ExactDuplicateAuditReport, MAX_ENTRIES,
};
use std::path::Path;

fn duplicate_report() -> ExactDuplicateAuditReport {
    let root = tempfile::tempdir().expect("temporary duplicate-audit root");
    std::fs::write(root.path().join("a.bin"), b"same exact bytes").expect("first duplicate");
    std::fs::write(root.path().join("b.bin"), b"same exact bytes").expect("second duplicate");
    let report = collect_exact_duplicate_audit(root.path(), 42, 1, 100)
        .expect("valid duplicate audit report");
    assert!(exact_duplicate_audit_integrity_valid(&report));
    assert_eq!(report.cluster_count, 1);
    report
}

fn expect_integrity_rejection(
    report: &ExactDuplicateAuditReport,
    mutate: impl FnOnce(&mut ExactDuplicateAuditReport),
) {
    let mut tampered = report.clone();
    mutate(&mut tampered);
    assert!(
        !exact_duplicate_audit_integrity_valid(&tampered),
        "tampered duplicate-audit evidence must fail closed"
    );
}

#[test]
fn collection_rejects_invalid_public_contract_inputs() {
    let root = tempfile::tempdir().expect("temporary duplicate-audit root");

    assert_eq!(
        collect_exact_duplicate_audit(Path::new("relative-root"), 42, 1, 100).unwrap_err(),
        "duplicate-audit-root-must-be-absolute"
    );
    assert_eq!(
        collect_exact_duplicate_audit(root.path(), 42, 0, 100).unwrap_err(),
        "duplicate-audit-min-bytes-out-of-range"
    );
    assert_eq!(
        collect_exact_duplicate_audit(root.path(), 42, 1, 0).unwrap_err(),
        "duplicate-audit-max-entries-out-of-range"
    );
    assert_eq!(
        collect_exact_duplicate_audit(root.path(), 42, 1, MAX_ENTRIES + 1).unwrap_err(),
        "duplicate-audit-max-entries-out-of-range"
    );
    assert_eq!(
        collect_exact_duplicate_audit(&root.path().join("missing"), 42, 1, 100).unwrap_err(),
        "duplicate-audit-root-unavailable"
    );

    let regular_file = root.path().join("not-a-directory.bin");
    std::fs::write(&regular_file, b"not a directory").expect("regular-file root fixture");
    assert_eq!(
        collect_exact_duplicate_audit(&regular_file, 42, 1, 100).unwrap_err(),
        "duplicate-audit-root-unsafe"
    );
}

#[test]
fn integrity_rejects_top_level_authority_and_schema_drift() {
    let report = duplicate_report();

    expect_integrity_rejection(&report, |value| value.schema_version += 1);
    expect_integrity_rejection(&report, |value| value.min_bytes = 0);
    expect_integrity_rejection(&report, |value| value.max_entries = 0);
    expect_integrity_rejection(&report, |value| value.production_metadata_evaluated = false);
    expect_integrity_rejection(&report, |value| {
        value.production_date_policy = "untrusted-policy".into()
    });
    expect_integrity_rejection(&report, |value| value.physical_reclaimable_bytes = Some(1));
    expect_integrity_rejection(&report, |value| {
        value.exact_content_match_is_delete_approval = true
    });
    expect_integrity_rejection(&report, |value| value.automatic_delete_allowed = true);
    expect_integrity_rejection(&report, |value| value.mutation_performed = true);
    expect_integrity_rejection(&report, |value| value.cluster_count += 1);
}

#[test]
fn integrity_rejects_cluster_and_member_drift() {
    let report = duplicate_report();

    expect_integrity_rejection(&report, |value| value.clusters[0].file_count = 1);
    expect_integrity_rejection(&report, |value| {
        value.clusters[0].logical_duplicate_bytes += 1
    });
    expect_integrity_rejection(&report, |value| {
        value.clusters[0].logical_redundant_bytes += 1
    });
    expect_integrity_rejection(&report, |value| {
        value.clusters[0].physical_reclaimable_bytes = Some(1)
    });
    expect_integrity_rejection(&report, |value| {
        value.clusters[0].distinct_storage_identity_count = Some(999)
    });
    expect_integrity_rejection(&report, |value| {
        value.clusters[0].requires_human_canonical_selection = false
    });
    expect_integrity_rejection(&report, |value| {
        value.clusters[0].automatic_delete_allowed = true
    });
    expect_integrity_rejection(&report, |value| {
        value.clusters[0].members[0].source_stable = false
    });
    expect_integrity_rejection(&report, |value| {
        value.clusters[0].members[0].write_performed = true
    });
    expect_integrity_rejection(&report, |value| {
        value.clusters[0].members[0].logical_bytes += 1
    });
    expect_integrity_rejection(&report, |value| {
        value.clusters[0].members[0].metadata_fingerprint = "forged".into()
    });
    expect_integrity_rejection(&report, |value| {
        value.clusters[0].members[0].member_fingerprint = "forged".into()
    });
    expect_integrity_rejection(&report, |value| {
        value.clusters[0].cluster_fingerprint = "forged".into()
    });
}

#[test]
fn integrity_rejects_aggregate_scope_and_fingerprint_drift() {
    let report = duplicate_report();

    expect_integrity_rejection(&report, |value| value.duplicate_file_count += 1);
    expect_integrity_rejection(&report, |value| value.logical_duplicate_bytes += 1);
    expect_integrity_rejection(&report, |value| value.logical_redundant_bytes += 1);
    expect_integrity_rejection(&report, |value| {
        value.metadata_evidence_complete = !value.metadata_evidence_complete
    });
    expect_integrity_rejection(&report, |value| {
        value.production_time_source_counts.clear()
    });
    expect_integrity_rejection(&report, |value| {
        value.source_scope_fingerprint = "0".repeat(64)
    });
    expect_integrity_rejection(&report, |value| value.audit_fingerprint = "0".repeat(64));
}

#[test]
fn summary_redacts_private_evidence_and_handles_an_empty_audit() {
    let report = duplicate_report();
    let summary = summarize_exact_duplicate_audit(&report);
    assert_eq!(
        summary.content_digest_algorithms,
        vec!["blake3", "sha256", "quickxor"]
    );
    assert!(!summary.local_paths_included);
    assert!(!summary.content_digests_included);
    assert!(summary.requires_human_canonical_selection);
    assert_eq!(summary.physical_reclaimable_bytes, None);
    assert!(!summary.exact_content_match_is_delete_approval);
    assert!(!summary.automatic_delete_allowed);
    assert!(!summary.mutation_performed);
    assert_eq!(summary.notices.len(), 7);
    assert!(summary
        .notices
        .contains(&"no-delete-approval-created".to_string()));

    let empty_root = tempfile::tempdir().expect("temporary empty duplicate-audit root");
    let empty = collect_exact_duplicate_audit(empty_root.path(), 43, 1, 100)
        .expect("empty read-only audit");
    assert!(exact_duplicate_audit_integrity_valid(&empty));
    assert_eq!(empty.cluster_count, 0);
    assert!(!summarize_exact_duplicate_audit(&empty).requires_human_canonical_selection);
}
