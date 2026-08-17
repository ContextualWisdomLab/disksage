//! Coverage for a clean secondary worktree whose HEAD is not reachable from retained references.
//!
//! The fixture creates an isolated local commit on a temporary linked worktree. Even though the
//! worktree is clean and structurally valid, that unique commit is outside the selected retained
//! `HEAD`; the production audit must preserve it and must not assess it for removal.

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
fn clean_unretained_secondary_head_is_preserved() {
    let root = initialized_repository();
    git(root.path(), &["branch", "unretained-secondary"]);

    let secondary_parent = tempfile::tempdir().unwrap();
    let secondary = secondary_parent.path().join("unretained-worktree");
    let secondary_text = secondary.to_string_lossy().into_owned();
    git(
        root.path(),
        &["worktree", "add", "-q", &secondary_text, "unretained-secondary"],
    );
    std::fs::write(secondary.join("unique.txt"), b"secondary-only\n").unwrap();
    git(&secondary, &["add", "unique.txt"]);
    git(&secondary, &["commit", "-q", "-m", "secondary unique commit"]);

    let report = audit_git_worktrees(
        root.path(),
        &["HEAD".into()],
        GitWorktreeAuditOptions::default(),
        7_000,
    )
    .unwrap();
    let entry = report
        .entries
        .iter()
        .find(|entry| entry.path == secondary_text)
        .expect("secondary worktree must be represented in exact evidence");

    assert_eq!(entry.status_clean, Some(true));
    assert_eq!(entry.contained_in_reference, Some(false));
    assert!(!entry.head_is_retained_tip);
    assert_eq!(entry.disposition, GitWorktreeDisposition::Preserve);
    assert!(entry
        .blockers
        .contains(&"head-not-contained-in-retention-set".to_string()));
    assert!(!entry.active_use.assessed);
    assert_eq!(report.removal_candidate_count, 0);
    assert!(report.evidence_complete);
    assert!(!report.filesystem_mutation_executed);

    assert!(secondary.exists());
    git(root.path(), &["worktree", "remove", "--force", &secondary_text]);
}
