//! Integrity-guard coverage for exact-duplicate evidence.
//!
//! These regressions start from a real, valid duplicate audit and then tamper one contract field at
//! a time. The validator must fail closed for every authority-bearing total, cluster invariant, and
//! member-evidence invariant rather than accepting a report whose outer JSON shape still parses.

use crate::duplicate_audit::{
    collect_exact_duplicate_audit, exact_duplicate_audit_integrity_valid, ExactDuplicateAuditReport,
    MAX_ENTRIES,
};

fn valid_report() -> ExactDuplicateAuditReport {
    let root = tempfile::tempdir().expect("temporary duplicate-audit root");
    std::fs::write(root.path().join("a.bin"), b"same exact bytes").expect("write first duplicate");
    std::fs::write(root.path().join("b.bin"), b"same exact bytes").expect("write second duplicate");
    let report = collect_exact_duplicate_audit(root.path(), 50_001, 1, 100)
        .expect("valid duplicate audit fixture");
    assert_eq!(report.cluster_count, 1);
    assert_eq!(report.clusters[0].members.len(), 2);
    assert!(exact_duplicate_audit_integrity_valid(&report));
    report
}

#[test]
fn validator_rejects_each_top_level_authority_and_total_tamper() {
    let report = valid_report();

    let mut tampered = report.clone();
    tampered.schema_version += 1;
    assert!(!exact_duplicate_audit_integrity_valid(&tampered));

    let mut tampered = report.clone();
    tampered.min_bytes = 0;
    assert!(!exact_duplicate_audit_integrity_valid(&tampered));

    let mut tampered = report.clone();
    tampered.max_entries = MAX_ENTRIES + 1;
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
    tampered.automatic_delete_allowed = true;
    assert!(!exact_duplicate_audit_integrity_valid(&tampered));

    let mut tampered = report.clone();
    tampered.mutation_performed = true;
    assert!(!exact_duplicate_audit_integrity_valid(&tampered));

    let mut tampered = report.clone();
    tampered.cluster_count += 1;
    assert!(!exact_duplicate_audit_integrity_valid(&tampered));

    let mut tampered = report.clone();
    tampered.duplicate_file_count += 1;
    assert!(!exact_duplicate_audit_integrity_valid(&tampered));

    let mut tampered = report.clone();
    tampered.logical_duplicate_bytes += 1;
    assert!(!exact_duplicate_audit_integrity_valid(&tampered));

    let mut tampered = report.clone();
    tampered.logical_redundant_bytes += 1;
    assert!(!exact_duplicate_audit_integrity_valid(&tampered));

    let mut tampered = report.clone();
    tampered.metadata_evidence_complete = !tampered.metadata_evidence_complete;
    assert!(!exact_duplicate_audit_integrity_valid(&tampered));

    let mut tampered = report.clone();
    tampered.production_time_source_counts.clear();
    assert!(!exact_duplicate_audit_integrity_valid(&tampered));

    let mut tampered = report.clone();
    tampered.source_scope_fingerprint = "0".repeat(64);
    assert!(!exact_duplicate_audit_integrity_valid(&tampered));

    let mut tampered = report;
    tampered.audit_fingerprint = "f".repeat(64);
    assert!(!exact_duplicate_audit_integrity_valid(&tampered));
}

#[test]
fn validator_rejects_each_cluster_and_member_authority_tamper() {
    let report = valid_report();

    let mut tampered = report.clone();
    tampered.clusters[0].file_count = 1;
    assert!(!exact_duplicate_audit_integrity_valid(&tampered));

    let mut tampered = report.clone();
    tampered.clusters[0].file_count = 3;
    assert!(!exact_duplicate_audit_integrity_valid(&tampered));

    let mut tampered = report.clone();
    tampered.clusters[0].logical_duplicate_bytes += 1;
    assert!(!exact_duplicate_audit_integrity_valid(&tampered));

    let mut tampered = report.clone();
    tampered.clusters[0].logical_redundant_bytes += 1;
    assert!(!exact_duplicate_audit_integrity_valid(&tampered));

    let mut tampered = report.clone();
    tampered.clusters[0].physical_reclaimable_bytes = Some(1);
    assert!(!exact_duplicate_audit_integrity_valid(&tampered));

    let mut tampered = report.clone();
    tampered.clusters[0].distinct_storage_identity_count = Some(99);
    assert!(!exact_duplicate_audit_integrity_valid(&tampered));

    let mut tampered = report.clone();
    tampered.clusters[0].requires_human_canonical_selection = false;
    assert!(!exact_duplicate_audit_integrity_valid(&tampered));

    let mut tampered = report.clone();
    tampered.clusters[0].automatic_delete_allowed = true;
    assert!(!exact_duplicate_audit_integrity_valid(&tampered));

    let mut tampered = report.clone();
    tampered.clusters[0].members[0].source_stable = false;
    assert!(!exact_duplicate_audit_integrity_valid(&tampered));

    let mut tampered = report.clone();
    tampered.clusters[0].members[0].write_performed = true;
    assert!(!exact_duplicate_audit_integrity_valid(&tampered));

    let mut tampered = report.clone();
    tampered.clusters[0].members[0].logical_bytes += 1;
    assert!(!exact_duplicate_audit_integrity_valid(&tampered));

    let mut tampered = report.clone();
    tampered.clusters[0].members[0].metadata_fingerprint = "0".repeat(64);
    assert!(!exact_duplicate_audit_integrity_valid(&tampered));

    let mut tampered = report.clone();
    tampered.clusters[0].members[0].member_fingerprint = "0".repeat(64);
    assert!(!exact_duplicate_audit_integrity_valid(&tampered));

    let mut tampered = report;
    tampered.clusters[0].cluster_fingerprint = "0".repeat(64);
    assert!(!exact_duplicate_audit_integrity_valid(&tampered));
}
