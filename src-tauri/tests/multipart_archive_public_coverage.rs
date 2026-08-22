//! Integration coverage for public split-ZIP audit and redacted summary behavior.
//!
//! Fixtures are ordinary local temporary files. The audit is read-only, never attempts reassembly,
//! and never grants discard authority from a locally contiguous or incomplete part set.

#![cfg(not(coverage))]

use disksage_lib::multipart_archive::{
    collect_multipart_archive_audit, parse_multipart_archive_name, summarize_multipart_audit,
    MultipartSetState,
};
use std::path::Path;

#[test]
fn multipart_name_parser_is_strict_and_case_insensitive() {
    assert_eq!(
        parse_multipart_archive_name("REPORT.ZIP.PART007"),
        Some(("report.zip".into(), 7))
    );
    for invalid in [
        "report.zip",
        "report.part007",
        "report.zip.part7",
        "report.zip.part0007",
        "report.zip.partabc",
    ] {
        assert_eq!(parse_multipart_archive_name(invalid), None, "{invalid}");
    }
}

#[test]
fn audit_rejects_unsafe_public_roots_without_mutation() {
    assert_eq!(
        collect_multipart_archive_audit(Path::new("relative-root"), 1, 100).unwrap_err(),
        "multipart-audit-root-must-be-absolute"
    );

    let root = tempfile::tempdir().expect("temporary multipart audit root");
    assert_eq!(
        collect_multipart_archive_audit(&root.path().join("missing"), 1, 100).unwrap_err(),
        "multipart-audit-root-unavailable"
    );

    let regular_file = root.path().join("not-a-directory.bin");
    std::fs::write(&regular_file, b"ordinary file").expect("regular-file root fixture");
    assert_eq!(
        collect_multipart_archive_audit(&regular_file, 1, 100).unwrap_err(),
        "multipart-audit-root-unsafe"
    );
}

#[test]
fn audit_distinguishes_missing_parts_from_terminal_unverified_sets() {
    let root = tempfile::tempdir().expect("temporary multipart audit root");
    std::fs::write(root.path().join("missing.zip.part000"), b"aa").expect("part zero");
    std::fs::write(root.path().join("missing.zip.part002"), b"bbbb").expect("part two");
    std::fs::write(root.path().join("contiguous.zip.part000"), b"x").expect("contiguous zero");
    std::fs::write(root.path().join("contiguous.zip.part001"), b"yy").expect("contiguous one");
    std::fs::write(root.path().join("ordinary.txt"), b"ignored").expect("ignored fixture");

    let report = collect_multipart_archive_audit(root.path(), 42, 100)
        .expect("bounded read-only multipart audit");
    assert!(report.evidence_complete);
    assert_eq!(report.set_count, 2);
    assert_eq!(report.part_count, 4);
    assert_eq!(report.part_bytes, 9);
    assert_eq!(report.incomplete_set_count, 1);
    assert_eq!(report.ambiguous_set_count, 0);
    assert_eq!(report.terminal_unverified_set_count, 1);
    assert_eq!(report.discard_review_bytes, 6);
    assert!(!report.mutation_performed);

    let missing = report
        .sets
        .iter()
        .find(|set| set.base_name == "missing.zip")
        .expect("missing set");
    assert_eq!(missing.state, MultipartSetState::MissingParts);
    assert_eq!(missing.present_parts, vec![0, 2]);
    assert_eq!(missing.missing_parts, vec![1]);
    assert_eq!(missing.complete_reassembly_possible, Some(false));
    assert!(missing.requires_human_review);
    assert!(!missing.automatic_discard_allowed);

    let contiguous = report
        .sets
        .iter()
        .find(|set| set.base_name == "contiguous.zip")
        .expect("contiguous set");
    assert_eq!(
        contiguous.state,
        MultipartSetState::ContiguousTerminalUnverified
    );
    assert_eq!(contiguous.present_parts, vec![0, 1]);
    assert!(contiguous.missing_parts.is_empty());
    assert_eq!(contiguous.complete_reassembly_possible, None);
    assert!(contiguous.requires_human_review);
    assert!(!contiguous.automatic_discard_allowed);

    let summary = summarize_multipart_audit(&report);
    assert_eq!(summary.output_mode, "multipart-archive-audit-summary");
    assert!(summary.human_discard_approval_required);
    assert!(!summary.automatic_discard_allowed);
    assert!(!summary.mutation_performed);
    assert_eq!(summary.notices.len(), 5);
    for redacted in [
        "absolute-source-root",
        "relative-directory",
        "archive-base-name",
        "member-relative-paths",
        "member-modification-times",
    ] {
        assert!(summary.redacted_from_summary.contains(&redacted.to_string()));
    }
    let encoded = serde_json::to_string(&summary).expect("summary JSON");
    assert!(!encoded.contains(root.path().to_string_lossy().as_ref()));
    assert!(!encoded.contains("missing.zip"));
    assert!(!encoded.contains("contiguous.zip"));
}

#[test]
fn empty_audit_is_complete_and_requires_no_discard_approval() {
    let root = tempfile::tempdir().expect("temporary empty multipart root");
    let report = collect_multipart_archive_audit(root.path(), 43, 100)
        .expect("empty read-only multipart audit");
    assert!(report.evidence_complete);
    assert_eq!(report.set_count, 0);
    assert_eq!(report.part_count, 0);
    assert_eq!(report.part_bytes, 0);
    assert!(!report.mutation_performed);
    assert!(!summarize_multipart_audit(&report).human_discard_approval_required);
}

#[cfg(target_os = "linux")]
#[test]
fn case_distinct_names_with_same_normalized_part_are_ambiguous() {
    let root = tempfile::tempdir().expect("temporary multipart audit root");
    std::fs::write(root.path().join("dup.zip.part000"), b"a").expect("lowercase part");
    std::fs::write(root.path().join("DUP.ZIP.PART000"), b"bb").expect("uppercase part");

    let report = collect_multipart_archive_audit(root.path(), 44, 100)
        .expect("case-sensitive filesystem multipart audit");
    assert_eq!(report.set_count, 1);
    assert_eq!(report.ambiguous_set_count, 1);
    assert_eq!(report.discard_review_bytes, 3);
    let set = &report.sets[0];
    assert_eq!(set.state, MultipartSetState::DuplicatePartIndex);
    assert_eq!(set.present_parts, vec![0]);
    assert_eq!(set.duplicate_part_indices, vec![0]);
    assert_eq!(set.complete_reassembly_possible, Some(false));
    assert!(!set.automatic_discard_allowed);
}
