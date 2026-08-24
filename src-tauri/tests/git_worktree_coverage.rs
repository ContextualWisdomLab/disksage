//! Real-Git coverage for the fail-closed worktree audit boundary.
//!
//! These tests build disposable repositories and linked worktrees with the real `git` executable.
//! They intentionally avoid mocks so parsing, reference binding, filesystem evidence, classification,
//! and public-summary redaction are exercised together without touching any user repository.

use disksage_lib::git_worktree::{
    audit_git_worktrees, public_summary, GitWorktreeAuditOptions, GitWorktreeDisposition,
};
use std::path::Path;
use std::process::Command;

fn git(cwd: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .expect("git executable must be available for worktree integration coverage");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("git fixture output must be utf-8")
        .trim()
        .to_string()
}

fn initialized_repository() -> tempfile::TempDir {
    let repository = tempfile::tempdir().expect("repository tempdir");
    git(repository.path(), &["init", "-b", "main"]);
    git(
        repository.path(),
        &["config", "user.email", "coverage@example.invalid"],
    );
    git(
        repository.path(),
        &["config", "user.name", "DiskSage Coverage"],
    );
    std::fs::write(repository.path().join("tracked.txt"), b"first\n")
        .expect("write first fixture version");
    git(repository.path(), &["add", "tracked.txt"]);
    git(repository.path(), &["commit", "-m", "first"]);
    repository
}

#[test]
fn primary_worktree_audit_is_read_only_and_public_summary_redacts_identity() {
    let repository = initialized_repository();
    let report = audit_git_worktrees(
        repository.path(),
        &["HEAD".to_string()],
        GitWorktreeAuditOptions::default(),
        1_000,
    )
    .expect("audit a clean primary worktree");

    assert_eq!(report.worktree_count, 1);
    assert_eq!(report.removal_candidate_count, 0);
    assert_eq!(report.preserved_count, 1);
    assert_eq!(report.evidence_gap_count, 0);
    assert!(report.evidence_complete);
    assert!(!report.filesystem_mutation_executed);
    assert!(report.exact_approval_phrase.is_none());

    let entry = &report.entries[0];
    assert!(entry.primary);
    assert!(entry.audit_origin);
    assert_eq!(entry.status_clean, Some(true));
    assert_eq!(entry.status_entry_count, Some(0));
    assert_eq!(entry.contained_in_reference, Some(true));
    assert!(entry.head_is_retained_tip);
    assert_eq!(entry.disposition, GitWorktreeDisposition::Preserve);
    assert!(entry.blockers.iter().any(|blocker| blocker == "primary-worktree"));
    assert!(entry
        .blockers
        .iter()
        .any(|blocker| blocker == "audit-origin-worktree"));
    assert!(!entry.active_use.assessed);
    assert_eq!(
        entry.active_use.error.as_deref(),
        Some("active-use-not-needed-for-preserved-worktree")
    );

    let summary = public_summary(&report);
    assert_eq!(summary.worktree_count, 1);
    assert!(summary.local_paths_redacted);
    assert!(summary.branch_names_redacted);
    assert!(summary
        .metadata_semantics
        .iter()
        .any(|value| value == "user-file-production-time-not-inferred"));
    assert!(summary
        .notices
        .iter()
        .any(|value| value == "no-worktree-prune-remove-or-branch-delete"));
}

#[test]
fn locked_ancestor_worktree_is_preserved_with_real_porcelain_and_reference_evidence() {
    let repository = initialized_repository();
    let ancestor = git(repository.path(), &["rev-parse", "HEAD"]);

    std::fs::write(repository.path().join("tracked.txt"), b"second\n")
        .expect("write retained fixture version");
    git(repository.path(), &["add", "tracked.txt"]);
    git(repository.path(), &["commit", "-m", "second"]);

    let linked_parent = tempfile::tempdir().expect("linked worktree parent");
    let linked_path = linked_parent.path().join("stale-worktree");
    let linked_path_text = linked_path.to_string_lossy().into_owned();
    git(
        repository.path(),
        &["worktree", "add", "--detach", &linked_path_text, &ancestor],
    );
    git(
        repository.path(),
        &[
            "worktree",
            "lock",
            "--reason",
            "coverage-fixture",
            &linked_path_text,
        ],
    );

    let report = audit_git_worktrees(
        repository.path(),
        &["refs/heads/main".to_string()],
        GitWorktreeAuditOptions::default(),
        2_000,
    )
    .expect("audit a locked ancestor worktree");

    assert_eq!(report.worktree_count, 2);
    assert_eq!(report.removal_candidate_count, 0);
    assert_eq!(report.preserved_count, 2);
    assert_eq!(report.evidence_gap_count, 0);
    assert!(report.evidence_complete);

    let linked = report
        .entries
        .iter()
        .find(|entry| !entry.primary)
        .expect("linked worktree entry");
    assert!(linked.detached);
    assert!(linked.locked);
    assert_eq!(linked.lock_reason.as_deref(), Some("coverage-fixture"));
    assert_eq!(linked.status_clean, Some(true));
    assert_eq!(linked.status_entry_count, Some(0));
    assert_eq!(linked.contained_in_reference, Some(true));
    assert!(!linked.head_is_retained_tip);
    assert_eq!(linked.disposition, GitWorktreeDisposition::Preserve);
    assert!(linked
        .blockers
        .iter()
        .any(|blocker| blocker == "worktree-locked"));
    assert!(!linked.active_use.assessed);
    assert!(linked.size.evidence_complete);
    assert!(linked.size.visited_entries > 0);
}

#[test]
fn audit_rejects_untrusted_roots_options_and_references_before_mutation() {
    let repository = initialized_repository();

    let relative_error = audit_git_worktrees(
        Path::new("relative-repository"),
        &["HEAD".to_string()],
        GitWorktreeAuditOptions::default(),
        3_000,
    )
    .expect_err("relative roots must fail closed");
    assert_eq!(
        relative_error,
        "git-worktree-repository-root-not-absolute"
    );

    let mut invalid_options = GitWorktreeAuditOptions::default();
    invalid_options.command_timeout_ms = 0;
    let option_error = audit_git_worktrees(
        repository.path(),
        &["HEAD".to_string()],
        invalid_options,
        3_001,
    )
    .expect_err("zero command timeout must fail closed");
    assert_eq!(option_error, "git-worktree-command-timeout-out-of-bounds");

    let reference_error = audit_git_worktrees(
        repository.path(),
        &["-dangerous-option-shaped-reference".to_string()],
        GitWorktreeAuditOptions::default(),
        3_002,
    )
    .expect_err("option-shaped references must never reach git");
    assert_eq!(reference_error, "git-worktree-reference-invalid");

    let worktree_list = git(repository.path(), &["worktree", "list", "--porcelain"]);
    assert_eq!(worktree_list.matches("worktree ").count(), 1);
}
