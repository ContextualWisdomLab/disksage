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

#[test]
fn regular_crdownload_candidate_is_observed_without_mutation_or_discard_authority() {
    const DAY_MS: u64 = 86_400_000;

    let root = tempfile::tempdir().expect("temporary candidate audit root");
    let partial = root.path().join("archive.zip.crdownload");
    // A real local partial-download fixture. The audit may derive type/recovery evidence from these
    // bytes, but this contract only requires the stable read-only candidate boundary.
    std::fs::write(
        &partial,
        [0x50, 0x4b, 0x03, 0x04, 0x14, 0x00, 0x00, 0x00, b'D', b'S'],
    )
    .expect("write partial-download fixture");
    let modified_ms = std::fs::metadata(&partial)
        .expect("partial metadata")
        .modified()
        .expect("partial modified time")
        .duration_since(std::time::UNIX_EPOCH)
        .expect("modified time after epoch")
        .as_millis() as u64;
    let observed_at_ms = modified_ms.saturating_add(40 * DAY_MS);

    let report = collect_incomplete_download_audit(
        root.path(),
        observed_at_ms,
        100,
        DEFAULT_STALE_AFTER_DAYS,
    )
    .expect("read-only candidate audit");

    assert_eq!(report.entries_seen, 1);
    assert_eq!(report.file_count, 1);
    assert_eq!(report.items.len(), 1);
    assert!(!report.mutation_performed);
    let item = &report.items[0];
    assert_eq!(item.relative_path, "archive.zip.crdownload");
    assert_eq!(item.logical_bytes, 10);
    assert!(item.modified_age_days >= DEFAULT_STALE_AFTER_DAYS);
    assert_eq!(item.candidate_fingerprint.len(), 64);
    assert!(item.partial_content_recovery_possible);
    assert!(item.requires_human_review);
    assert!(!item.automatic_discard_allowed);
    assert!(!item.final_sibling_exists);
    assert!(item.final_sibling_relative_path.is_none());

    let summary = summarize_incomplete_download_audit(&report);
    assert_eq!(summary.file_count, 1);
    assert_eq!(summary.items.len(), 1);
    assert!(!summary.mutation_performed);
    assert!(!summary.automatic_discard_allowed);
    let encoded = serde_json::to_string(&summary).expect("summary JSON");
    assert!(!encoded.contains(root.path().to_string_lossy().as_ref()));
    assert!(!encoded.contains("archive.zip.crdownload"));
}

#[test]
fn completed_sibling_is_recorded_as_recovery_evidence_without_authorizing_discard() {
    let root = tempfile::tempdir().expect("temporary sibling audit root");
    let partial = root.path().join("report.pdf.crdownload");
    let completed = root.path().join("report.pdf");
    std::fs::write(&partial, b"partial-download-bytes").expect("write partial fixture");
    std::fs::write(&completed, b"completed-file-bytes").expect("write completed sibling fixture");
    let modified_ms = std::fs::metadata(&partial)
        .expect("partial metadata")
        .modified()
        .expect("partial modified time")
        .duration_since(std::time::UNIX_EPOCH)
        .expect("modified time after epoch")
        .as_millis() as u64;

    let report = collect_incomplete_download_audit(
        root.path(),
        modified_ms.saturating_add(1),
        100,
        DEFAULT_STALE_AFTER_DAYS,
    )
    .expect("read-only audit with completed sibling");

    assert_eq!(report.entries_seen, 2);
    assert_eq!(report.file_count, 1);
    assert_eq!(report.final_sibling_count, 1);
    assert!(!report.mutation_performed);
    let item = &report.items[0];
    assert_eq!(item.relative_path, "report.pdf.crdownload");
    assert!(item.final_sibling_exists);
    assert_eq!(item.final_sibling_relative_path.as_deref(), Some("report.pdf"));
    assert_eq!(item.final_sibling_bytes, Some(20));
    assert!(item.recovery_candidate);
    assert!(item.requires_human_review);
    assert!(!item.automatic_discard_allowed);

    let summary = summarize_incomplete_download_audit(&report);
    assert_eq!(summary.final_sibling_count, 1);
    assert_eq!(summary.items.len(), 1);
    assert!(!summary.mutation_performed);
    assert!(!summary.automatic_discard_allowed);
    let encoded = serde_json::to_string(&summary).expect("summary JSON");
    assert!(!encoded.contains(root.path().to_string_lossy().as_ref()));
    assert!(!encoded.contains("report.pdf"));
}
