//! Coverage for stale Git worktree registrations whose filesystem path disappeared.
//!
//! The fixture deletes only a temporary test worktree directory. The production audit must retain
//! the stale registration as an evidence gap instead of treating Git's `prunable` metadata as
//! removal authority.

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
fn missing_secondary_path_is_prunable_but_remains_an_evidence_gap() {
    let root = initialized_repository();
    git(root.path(), &["branch", "prunable-test-worktree"]);

    std::fs::write(root.path().join("retained.txt"), b"retained\n").unwrap();
    git(root.path(), &["add", "retained.txt"]);
    git(root.path(), &["commit", "-q", "-m", "advance retained tip"]);

    let secondary_parent = tempfile::tempdir().unwrap();
    let secondary = secondary_parent.path().join("missing-worktree");
    let secondary_text = secondary.to_string_lossy().into_owned();
    git(
        root.path(),
        &["worktree", "add", "-q", &secondary_text, "prunable-test-worktree"],
    );
    std::fs::remove_dir_all(&secondary).unwrap();

    let report = audit_git_worktrees(
        root.path(),
        &["HEAD".into()],
        GitWorktreeAuditOptions::default(),
        4_000,
    )
    .unwrap();
    let entry = report
        .entries
        .iter()
        .find(|entry| entry.path == secondary_text)
        .expect("stale registered worktree must remain visible to the audit");

    assert!(entry.prunable);
    assert!(entry.prunable_reason.is_some());
    assert_eq!(entry.status_clean, None);
    assert!(!entry.size.evidence_complete);
    assert_eq!(
        entry.size.error.as_deref(),
        Some("worktree-path-evidence-incomplete")
    );
    assert_eq!(entry.disposition, GitWorktreeDisposition::EvidenceGap);
    assert!(entry
        .blockers
        .contains(&"worktree-prunable-metadata".to_string()));
    assert!(entry
        .blockers
        .contains(&"worktree-path-evidence-incomplete".to_string()));
    assert!(entry
        .blockers
        .contains(&"git-status-evidence-incomplete".to_string()));
    match entry.actor_cwd_inside {
        Some(false) => assert!(!entry
            .blockers
            .contains(&"actor-cwd-evidence-incomplete".to_string())),
        None => assert!(entry
            .blockers
            .contains(&"actor-cwd-evidence-incomplete".to_string())),
        Some(true) => panic!("unrelated stale worktree must not contain the actor CWD"),
    }
    assert!(entry
        .blockers
        .contains(&"size-evidence-incomplete".to_string()));
    assert!(report.evidence_gap_count >= 1);
    assert!(!report.evidence_complete);
    assert!(!report.issues.is_empty());
    assert!(!report.filesystem_mutation_executed);
}
