//! macOS regression for read-only root binding.
//!
//! DiskSage audits must be able to traverse children through the root identity boundary instead of
//! converting a valid bound directory into incomplete evidence before any child can be observed.

#[cfg(target_os = "macos")]
#[test]
fn duplicate_audit_traverses_children_through_bound_root() {
    use disksage_lib::duplicate_audit::collect_exact_duplicate_audit;

    let root = tempfile::tempdir().expect("temporary audit root");
    let child = root.path().join("observed.bin");
    std::fs::write(&child, vec![0x5a; 4096]).expect("write realistic audit fixture");

    let report = collect_exact_duplicate_audit(root.path(), 1_700_000_000_000, 1, 32)
        .expect("a real macOS directory must remain traversable after root binding");

    assert!(
        report.evidence_complete,
        "bound-root traversal must not degrade a readable directory into incomplete evidence: {:?}",
        report.issue_counts
    );
    assert_eq!(report.entries_seen, 1);
    assert_eq!(report.file_count, 1);
    assert!(
        !report
            .issue_counts
            .contains_key("duplicate-audit-directory-read-failed"),
        "the bound root must support directory traversal on macOS"
    );
}
