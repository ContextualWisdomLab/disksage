//! Integration coverage for public incomplete-download audit admission and redaction boundaries.
//!
//! Fixtures are local temporary filesystem objects only. The audit remains read-only and no test
//! grants discard authority or interprets filesystem timestamps as production time.

use disksage_lib::incomplete_download::{
    collect_incomplete_download_audit, summarize_incomplete_download_audit,
    DEFAULT_STALE_AFTER_DAYS, MAX_STALE_AFTER_DAYS,
};
use std::path::Path;

#[test]
fn audit_rejects_invalid_public_root_and_staleness_inputs() {
    assert_eq!(
        collect_incomplete_download_audit(Path::new("relative-root"), 1, 100, DEFAULT_STALE_AFTER_DAYS)
            .unwrap_err(),
        "incomplete-download-audit-root-must-be-absolute"
    );

    let root = tempfile::tempdir().expect("temporary audit root");
    assert_eq!(
        collect_incomplete_download_audit(root.path(), 1, 100, 0).unwrap_err(),
        "incomplete-download-stale-days-out-of-range"
    );
    assert_eq!(
        collect_incomplete_download_audit(root.path(), 1, 100, MAX_STALE_AFTER_DAYS + 1)
            .unwrap_err(),
        "incomplete-download-stale-days-out-of-range"
    );
    assert_eq!(
        collect_incomplete_download_audit(
            &root.path().join("missing"),
            1,
            100,
            DEFAULT_STALE_AFTER_DAYS,
        )
        .unwrap_err(),
        "incomplete-download-audit-root-unavailable"
    );

    let regular_file = root.path().join("not-a-directory.bin");
    std::fs::write(&regular_file, b"ordinary file").expect("regular-file root fixture");
    assert_eq!(
        collect_incomplete_download_audit(&regular_file, 1, 100, DEFAULT_STALE_AFTER_DAYS)
            .unwrap_err(),
        "incomplete-download-audit-root-unsafe"
    );
}

#[test]
fn empty_audit_is_complete_read_only_and_privacy_redacted() {
    let root = tempfile::tempdir().expect("temporary empty audit root");
    let report = collect_incomplete_download_audit(root.path(), 42, 100, DEFAULT_STALE_AFTER_DAYS)
        .expect("empty read-only audit");

    assert!(report.evidence_complete);
    assert_eq!(report.observed_at_ms, 42);
    assert_eq!(report.entries_seen, 0);
    assert_eq!(report.file_count, 0);
    assert_eq!(report.logical_bytes, 0);
    assert_eq!(report.allocated_bytes, 0);
    assert!(!report.mutation_performed);

    let summary = summarize_incomplete_download_audit(&report);
    assert!(!summary.human_discard_approval_required);
    assert!(!summary.automatic_discard_allowed);
    assert!(!summary.mutation_performed);
    assert_eq!(summary.notices.len(), 8);
    assert!(summary
        .redacted_from_summary
        .contains(&"absolute-source-root".to_string()));
    let encoded = serde_json::to_string(&summary).expect("summary JSON");
    assert!(!encoded.contains(root.path().to_string_lossy().as_ref()));
}

#[test]
fn bounded_traversal_marks_entry_limit_evidence_incomplete() {
    let root = tempfile::tempdir().expect("temporary bounded audit root");
    std::fs::write(root.path().join("a.txt"), b"first").expect("first ordinary file");
    std::fs::write(root.path().join("b.txt"), b"second").expect("second ordinary file");

    let report = collect_incomplete_download_audit(root.path(), 43, 1, DEFAULT_STALE_AFTER_DAYS)
        .expect("bounded read-only audit");
    assert!(!report.evidence_complete);
    assert_eq!(report.entries_seen, 1);
    assert_eq!(report.file_count, 0);
    assert_eq!(report.issue_counts.get("entry-limit-reached"), Some(&1));
    assert!(!report.mutation_performed);

    let summary = summarize_incomplete_download_audit(&report);
    assert!(!summary.evidence_complete);
    assert_eq!(summary.issue_counts.get("entry-limit-reached"), Some(&1));
    assert!(!summary.automatic_discard_allowed);
    assert!(!summary.mutation_performed);
}

#[cfg(unix)]
#[test]
fn descendant_symlink_is_counted_but_never_followed() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().expect("temporary audit root");
    let external = tempfile::tempdir().expect("external symlink target root");
    let target = external.path().join("outside.crdownload");
    std::fs::write(&target, b"outside-data").expect("external target fixture");
    symlink(&target, root.path().join("linked.crdownload")).expect("descendant symlink fixture");

    let report = collect_incomplete_download_audit(root.path(), 44, 100, DEFAULT_STALE_AFTER_DAYS)
        .expect("read-only audit with descendant symlink");
    assert!(report.evidence_complete);
    assert_eq!(report.entries_seen, 1);
    assert_eq!(report.file_count, 0);
    assert_eq!(report.logical_bytes, 0);
    assert!(!report.mutation_performed);
}
