//! Integrity-guard and collector-boundary coverage for exact-duplicate evidence.
//!
//! These regressions start from a real, valid duplicate audit and then tamper one contract field at
//! a time. The validator must fail closed for every authority-bearing total, cluster invariant, and
//! member-evidence invariant rather than accepting a report whose outer JSON shape still parses.
//! Collector tests exercise real filesystem boundaries while remaining read-only with respect to
//! the audited source tree.

use crate::cloud::MetadataEvidence;
use crate::duplicate_audit::{
    collect_exact_duplicate_audit, exact_duplicate_audit_integrity_valid, ExactDuplicateAuditReport,
    MAX_ENTRIES,
};
use std::path::Path;

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
fn collector_rejects_invalid_root_and_bounds_before_evidence() {
    let root = tempfile::tempdir().expect("temporary duplicate-audit root");
    let file_root = root.path().join("file-root.bin");
    std::fs::write(&file_root, b"not a directory").expect("write file root fixture");
    let missing_root = root.path().join("missing-root");

    assert_eq!(
        collect_exact_duplicate_audit(Path::new("relative-root"), 50_010, 1, 100).unwrap_err(),
        "duplicate-audit-root-must-be-absolute"
    );
    assert_eq!(
        collect_exact_duplicate_audit(root.path(), 50_011, 0, 100).unwrap_err(),
        "duplicate-audit-min-bytes-out-of-range"
    );
    assert_eq!(
        collect_exact_duplicate_audit(root.path(), 50_012, 1, 0).unwrap_err(),
        "duplicate-audit-max-entries-out-of-range"
    );
    assert_eq!(
        collect_exact_duplicate_audit(root.path(), 50_013, 1, MAX_ENTRIES + 1).unwrap_err(),
        "duplicate-audit-max-entries-out-of-range"
    );
    assert_eq!(
        collect_exact_duplicate_audit(&missing_root, 50_014, 1, 100).unwrap_err(),
        "duplicate-audit-root-unavailable"
    );
    assert_eq!(
        collect_exact_duplicate_audit(&file_root, 50_015, 1, 100).unwrap_err(),
        "duplicate-audit-root-unsafe"
    );
}

#[test]
fn collector_marks_depth_bound_incomplete_without_descending_past_policy() {
    let root = tempfile::tempdir().expect("temporary duplicate-audit root");
    let mut current = root.path().to_path_buf();
    for index in 0..66 {
        current.push(format!("d{index:02}"));
        std::fs::create_dir(&current).expect("create bounded nested directory fixture");
    }
    std::fs::write(current.join("too-deep.bin"), b"must not be inspected")
        .expect("write beyond-depth fixture");

    let report = collect_exact_duplicate_audit(root.path(), 50_016, 1, 1_000)
        .expect("depth exhaustion is represented as incomplete evidence, not an API error");

    assert!(!report.evidence_complete);
    assert_eq!(
        report
            .issue_counts
            .get("duplicate-audit-depth-limit-reached")
            .copied(),
        Some(1)
    );
    assert_eq!(report.file_count, 0);
    assert_eq!(report.content_hashed_file_count, 0);
    assert!(report.clusters.is_empty());
    assert!(exact_duplicate_audit_integrity_valid(&report));
}

#[cfg(unix)]
#[test]
fn collector_skips_socket_entries_without_following_or_hashing_them() {
    use std::os::unix::net::UnixListener;

    let root = tempfile::tempdir().expect("temporary duplicate-audit root");
    std::fs::write(root.path().join("a.bin"), b"same exact bytes").expect("write first duplicate");
    std::fs::write(root.path().join("b.bin"), b"same exact bytes").expect("write second duplicate");
    let socket_path = root.path().join("runtime.sock");
    let _listener = UnixListener::bind(&socket_path).expect("bind local socket fixture");

    let report = collect_exact_duplicate_audit(root.path(), 50_017, 1, 100)
        .expect("socket entry must not make a read-only audit fail");

    assert!(report.evidence_complete);
    assert_eq!(report.file_count, 2);
    assert_eq!(report.cluster_count, 1);
    assert_eq!(report.clusters[0].members.len(), 2);
    assert!(report
        .clusters
        .iter()
        .flat_map(|cluster| &cluster.members)
        .all(|member| member.relative_path != "runtime.sock"));
    assert!(exact_duplicate_audit_integrity_valid(&report));
}

#[test]
fn collector_filters_below_minimum_and_hashes_same_size_nonduplicates_without_false_cluster() {
    let root = tempfile::tempdir().expect("temporary duplicate-audit root");
    std::fs::write(root.path().join("tiny-a.bin"), b"x").expect("write first tiny fixture");
    std::fs::write(root.path().join("tiny-b.bin"), b"x").expect("write second tiny fixture");
    std::fs::write(root.path().join("candidate-a.bin"), b"abcdefgh")
        .expect("write first same-size candidate");
    std::fs::write(root.path().join("candidate-b.bin"), b"abcdefgi")
        .expect("write second same-size candidate");

    let report = collect_exact_duplicate_audit(root.path(), 50_018, 2, 100)
        .expect("bounded collector must distinguish size collision from exact duplication");

    assert!(report.evidence_complete);
    assert_eq!(report.entries_seen, 4);
    assert_eq!(report.file_count, 4);
    assert_eq!(report.size_collision_candidate_count, 2);
    assert_eq!(report.content_hashed_file_count, 2);
    assert_eq!(report.cluster_count, 0);
    assert_eq!(report.duplicate_file_count, 0);
    assert_eq!(report.logical_duplicate_bytes, 0);
    assert_eq!(report.logical_redundant_bytes, 0);
    assert!(report.issue_counts.is_empty());
    assert!(exact_duplicate_audit_integrity_valid(&report));
}

#[test]
fn empty_duplicate_summary_stays_redacted_and_requires_no_canonical_selection() {
    let root = tempfile::tempdir().expect("temporary duplicate-audit root");
    std::fs::write(root.path().join("unique.bin"), b"unique payload")
        .expect("write unique fixture");
    let report = collect_exact_duplicate_audit(root.path(), 50_019, 1, 100)
        .expect("single-file audit must remain a valid no-duplicate report");
    assert!(exact_duplicate_audit_integrity_valid(&report));

    let summary = crate::duplicate_audit::summarize_exact_duplicate_audit(&report);

    assert_eq!(summary.cluster_count, 0);
    assert_eq!(summary.duplicate_file_count, 0);
    assert!(!summary.requires_human_canonical_selection);
    assert!(!summary.local_paths_included);
    assert!(!summary.content_digests_included);
    assert!(!summary.exact_content_match_is_delete_approval);
    assert!(!summary.automatic_delete_allowed);
    assert!(!summary.mutation_performed);
    assert_eq!(summary.physical_reclaimable_bytes, None);
    assert!(summary
        .notices
        .iter()
        .any(|notice| notice == "content-hashes-and-relative-paths-redacted-from-summary"));
}

#[test]
fn validator_rejects_nested_metadata_tamper_with_authors_context_and_embedded_evidence() {
    let report = valid_report();
    let mut tampered = report;
    let metadata = &mut tampered.clusters[0].members[0].production_metadata;
    metadata.authors = vec!["author-claim".into()];
    metadata.context = vec!["context-claim".into()];
    metadata.embedded_evidence.push(MetadataEvidence {
        field: "production-time".into(),
        value: "123".into(),
        source: "embedded:test".into(),
        confidence: "high".into(),
    });

    assert!(!exact_duplicate_audit_integrity_valid(&tampered));
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
