//! Safety-boundary coverage for incomplete Git worktree evidence.
//!
//! The fixture is local-only and read-only from DiskSage's perspective. It proves that a bounded
//! size scan which cannot inspect the whole worktree is surfaced as an evidence gap and never
//! becomes removal authority.

use disksage_lib::git_worktree::{
    audit_git_worktrees, GitWorktreeAuditOptions, GitWorktreeDisposition,
};
use std::path::Path;
use std::process::Command;

fn git(cwd: &Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .status()
        .expect("git must be available in the test environment");
    assert!(status.success(), "git {args:?} failed");
}

fn initialized_repository() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    git(temp.path(), &["init", "-q"]);
    git(temp.path(), &["config", "user.name", "DiskSage Test"]);
    git(temp.path(), &["config", "user.email", "disksage@example.invalid"]);
    std::fs::write(temp.path().join("tracked.txt"), b"tracked\n").unwrap();
    git(temp.path(), &["add", "tracked.txt"]);
    git(temp.path(), &["commit", "-q", "-m", "initial"]);
    temp
}

#[test]
fn bounded_size_scan_gap_never_becomes_removal_authority() {
    let root = initialized_repository();
    let options = GitWorktreeAuditOptions {
        max_entries_per_worktree: 1,
        ..GitWorktreeAuditOptions::default()
    };

    let report = audit_git_worktrees(root.path(), &["HEAD".into()], options, 42).unwrap();
    assert_eq!(report.worktree_count, 1);
    assert_eq!(report.removal_candidate_count, 0);
    assert_eq!(report.evidence_gap_count, 1);
    assert!(!report.evidence_complete);
    assert_eq!(report.exact_approval_phrase, None);
    assert!(!report.filesystem_mutation_executed);

    let entry = &report.entries[0];
    assert!(!entry.size.evidence_complete);
    assert_eq!(entry.size.visited_entries, 1);
    assert_eq!(entry.size.error.as_deref(), Some("size-scan-entry-limit"));
    assert!(entry.blockers.contains(&"size-evidence-incomplete".to_string()));
    assert_eq!(entry.disposition, GitWorktreeDisposition::EvidenceGap);
    assert!(root.path().join("tracked.txt").exists());
}
