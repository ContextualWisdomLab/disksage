//! Coverage for deterministic Git worktree size-evidence exhaustion.
//!
//! The fixture is an isolated temporary repository. It verifies that a deliberately tiny valid
//! traversal budget fails closed as incomplete evidence instead of becoming removal authority.

use disksage_lib::git_worktree::{audit_git_worktrees, GitWorktreeAuditOptions, GitWorktreeDisposition};
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
fn valid_tiny_entry_budget_yields_incomplete_non_executable_size_evidence() {
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
    assert!(report.exact_approval_phrase.is_none());
    assert!(!report.filesystem_mutation_executed);

    let entry = &report.entries[0];
    assert_eq!(entry.disposition, GitWorktreeDisposition::Preserve);
    assert!(!entry.size.evidence_complete);
    assert_eq!(entry.size.error.as_deref(), Some("size-scan-entry-limit"));
    assert!(entry
        .blockers
        .contains(&"size-evidence-incomplete".to_string()));
}
