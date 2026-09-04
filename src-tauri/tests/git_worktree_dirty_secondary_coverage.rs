//! Real-repository coverage for a dirty secondary Git worktree.
//!
//! A secondary worktree whose HEAD is already contained in the retained tip would otherwise be a
//! reclaim candidate. Local untracked content must make that exact worktree non-executable while
//! preserving both the worktree and its branch.

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
    let temp = tempfile::tempdir().expect("create temporary Git repository");
    git(temp.path(), &["init", "-q"]);
    git(temp.path(), &["config", "user.name", "DiskSage Test"]);
    git(
        temp.path(),
        &["config", "user.email", "disksage@example.invalid"],
    );
    std::fs::write(temp.path().join("tracked.txt"), b"tracked\n")
        .expect("write initial tracked fixture");
    git(temp.path(), &["add", "tracked.txt"]);
    git(temp.path(), &["commit", "-q", "-m", "initial"]);
    temp
}

#[cfg(unix)]
#[test]
fn dirty_secondary_worktree_is_preserved_even_when_its_head_is_retained() {
    let root = initialized_repository();
    git(root.path(), &["branch", "dirty-stale-worktree"]);

    std::fs::write(root.path().join("retained.txt"), b"new retained tip\n")
        .expect("advance retained branch fixture");
    git(root.path(), &["add", "retained.txt"]);
    git(root.path(), &["commit", "-q", "-m", "advance retained tip"]);

    let secondary_parent = tempfile::tempdir().expect("create secondary worktree parent");
    let secondary = secondary_parent.path().join("dirty-worktree");
    let secondary_text = secondary.to_string_lossy().into_owned();
    git(
        root.path(),
        &[
            "worktree",
            "add",
            "-q",
            &secondary_text,
            "dirty-stale-worktree",
        ],
    );
    std::fs::write(secondary.join("local-only.txt"), b"must be preserved\n")
        .expect("create local untracked worktree content");

    let report = audit_git_worktrees(
        root.path(),
        &["HEAD".into()],
        GitWorktreeAuditOptions::default(),
        2_000,
    )
    .expect("audit local Git worktrees");

    assert_eq!(report.worktree_count, 2);
    assert_eq!(report.removal_candidate_count, 0);
    assert_eq!(report.exact_approval_phrase, None);
    assert!(!report.filesystem_mutation_executed);

    let entry = report
        .entries
        .iter()
        .find(|entry| entry.path == secondary_text)
        .expect("secondary worktree must be represented in exact evidence");
    assert!(!entry.primary);
    assert!(!entry.audit_origin);
    assert!(entry.contained_in_reference == Some(true));
    assert!(!entry.head_is_retained_tip);
    assert_eq!(entry.status_clean, Some(false));
    assert!(entry.status_entry_count.is_some_and(|count| count >= 1));
    assert!(entry.blockers.contains(&"worktree-dirty".to_string()));
    assert_eq!(entry.disposition, GitWorktreeDisposition::Preserve);

    assert!(secondary.exists());
    assert_eq!(
        std::fs::read(secondary.join("local-only.txt")).expect("local content must remain"),
        b"must be preserved\n"
    );
    git(
        root.path(),
        &[
            "show-ref",
            "--verify",
            "--quiet",
            "refs/heads/dirty-stale-worktree",
        ],
    );
}
