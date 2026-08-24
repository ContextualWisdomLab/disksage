//! Human-approval validation coverage for stale Git worktree removal.
//!
//! A real temporary Git repository produces one clean merged secondary worktree and therefore a
//! real executable audit plan. The tests stop at approval construction; they never remove a
//! worktree, delete a branch, prune metadata, or contact a provider.

use disksage_lib::git_worktree::{
    approve_stale_worktree_removal, audit_git_worktrees, GitWorktreeAuditOptions,
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

fn removable_audit(generated_at_ms: u64) -> (tempfile::TempDir, tempfile::TempDir, disksage_lib::git_worktree::GitWorktreeAuditReport) {
    let root = tempfile::tempdir().unwrap();
    git(root.path(), &["init", "-q"]);
    git(root.path(), &["config", "user.name", "DiskSage Test"]);
    git(root.path(), &["config", "user.email", "disksage@example.invalid"]);
    std::fs::write(root.path().join("tracked.txt"), b"tracked\n").unwrap();
    git(root.path(), &["add", "tracked.txt"]);
    git(root.path(), &["commit", "-q", "-m", "initial"]);
    git(root.path(), &["branch", "stale-approval-test"]);

    std::fs::write(root.path().join("newer.txt"), b"retained tip\n").unwrap();
    git(root.path(), &["add", "newer.txt"]);
    git(root.path(), &["commit", "-q", "-m", "advance retained tip"]);

    let secondary_parent = tempfile::tempdir().unwrap();
    let secondary = secondary_parent.path().join("stale-worktree");
    let secondary_text = secondary.to_string_lossy().into_owned();
    git(
        root.path(),
        &["worktree", "add", "-q", &secondary_text, "stale-approval-test"],
    );

    let report = audit_git_worktrees(
        root.path(),
        &["HEAD".into()],
        GitWorktreeAuditOptions::default(),
        generated_at_ms,
    )
    .unwrap();
    assert_eq!(report.removal_candidate_count, 1);
    assert!(report.evidence_complete);
    assert!(report.exact_approval_phrase.is_some());
    (root, secondary_parent, report)
}

#[cfg(unix)]
#[test]
fn approval_timestamp_must_not_predate_the_exact_audit() {
    let (_root, _secondary_parent, report) = removable_audit(10_000);
    let phrase = report.exact_approval_phrase.as_deref().unwrap();

    assert_eq!(
        approve_stale_worktree_removal(
            &report,
            phrase,
            9_999,
            "human:coverage-reviewer",
            "Reviewed the exact clean merged worktree evidence",
        )
        .unwrap_err(),
        "git-worktree-removal-approval-predates-audit"
    );
}

#[cfg(unix)]
#[test]
fn approval_rejects_oversized_rationale_and_nonhuman_attribution() {
    let (_root, _secondary_parent, report) = removable_audit(20_000);
    let phrase = report.exact_approval_phrase.as_deref().unwrap();

    assert_eq!(
        approve_stale_worktree_removal(
            &report,
            phrase,
            20_001,
            "human:coverage-reviewer",
            &"r".repeat(100_000),
        )
        .unwrap_err(),
        "git-worktree-removal-rationale-too-long"
    );

    assert_eq!(
        approve_stale_worktree_removal(
            &report,
            phrase,
            20_001,
            "agent:coverage-reviewer",
            "Reviewed the exact clean merged worktree evidence",
        )
        .unwrap_err(),
        "git-worktree-removal-human-attribution-invalid"
    );
}
