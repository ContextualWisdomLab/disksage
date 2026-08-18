//! Integrity-edge coverage for exact duplicate audit evidence.
//!
//! Fixtures are temporary local files only. These regressions exercise the public fail-closed
//! integrity validator and never authorize or perform deletion.

use disksage_lib::duplicate_audit::{
    collect_exact_duplicate_audit, exact_duplicate_audit_integrity_valid, ExactDuplicateAuditReport,
    MAX_ENTRIES,
};

fn duplicate_report() -> ExactDuplicateAuditReport {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("a.bin"), b"same").unwrap();
    std::fs::write(root.path().join("b.bin"), b"same").unwrap();
    collect_exact_duplicate_audit(root.path(), 42, 1, 100).unwrap()
}

#[test]
fn cluster_and_member_contract_tampering_fail_closed_independently() {
    let report = duplicate_report();
    assert!(exact_duplicate_audit_integrity_valid(&report));

    let mut tampered = report.clone();
    tampered.clusters[0].file_count = 1;
    assert!(!exact_duplicate_audit_integrity_valid(&tampered));

    let mut tampered = report.clone();
    tampered.clusters[0].file_count = tampered.clusters[0].members.len() + 1;
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

#[test]
fn derived_report_contract_tampering_fail_closed_independently() {
    let report = duplicate_report();
    assert!(exact_duplicate_audit_integrity_valid(&report));

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

    let mut tampered = report;
    tampered.source_scope_fingerprint = "0".repeat(64);
    assert!(!exact_duplicate_audit_integrity_valid(&tampered));
}

#[test]
fn upper_entry_bound_tampering_fails_closed() {
    let mut report = duplicate_report();
    assert!(exact_duplicate_audit_integrity_valid(&report));
    report.max_entries = MAX_ENTRIES + 1;
    assert!(!exact_duplicate_audit_integrity_valid(&report));
}

#[cfg(unix)]
#[test]
fn storage_identity_count_tampering_fails_closed() {
    let mut report = duplicate_report();
    assert!(exact_duplicate_audit_integrity_valid(&report));
    report.clusters[0].distinct_storage_identity_count = Some(0);
    assert!(!exact_duplicate_audit_integrity_valid(&report));
}