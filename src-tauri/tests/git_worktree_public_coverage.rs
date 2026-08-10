//! Public-contract coverage for the read-only Git worktree audit boundary.
//!
//! The fixtures create local temporary repositories only. They perform no fetch, branch deletion,
//! worktree removal, provider operation, or user-file cleanup.

use disksage_lib::git_worktree::{
    approve_stale_worktree_removal, audit_git_worktrees, public_summary, GitWorktreeAuditOptions,
    GitWorktreeDisposition, GIT_WORKTREE_AUDIT_SCHEMA_KIND,
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
fn option_and_reference_admission_fail_closed_before_audit() {
    let root = initialized_repository();
    let defaults = GitWorktreeAuditOptions::default();

    let invalid_options = [
        GitWorktreeAuditOptions { command_timeout_ms: 0, ..defaults },
        GitWorktreeAuditOptions { command_timeout_ms: 300_001, ..defaults },
        GitWorktreeAuditOptions { size_scan_timeout_ms: 0, ..defaults },
        GitWorktreeAuditOptions { size_scan_timeout_ms: 600_001, ..defaults },
        GitWorktreeAuditOptions { max_worktrees: 0, ..defaults },
        GitWorktreeAuditOptions { max_worktrees: 10_001, ..defaults },
        GitWorktreeAuditOptions { max_entries_per_worktree: 0, ..defaults },
        GitWorktreeAuditOptions { max_entries_per_worktree: 20_000_001, ..defaults },
        GitWorktreeAuditOptions { max_active_pids: 0, ..defaults },
        GitWorktreeAuditOptions { max_active_pids: 4_097, ..defaults },
    ];
    for options in invalid_options {
        assert!(audit_git_worktrees(root.path(), &["HEAD".into()], options, 1).is_err());
    }

    assert_eq!(
        audit_git_worktrees(
            Path::new("relative"),
            &["HEAD".into()],
            defaults,
            1,
        )
        .unwrap_err(),
        "git-worktree-repository-root-not-absolute"
    );
    assert!(audit_git_worktrees(root.path(), &[], defaults, 1).is_err());
    for reference in ["", "-dangerous", "bad\nref"] {
        assert_eq!(
            audit_git_worktrees(root.path(), &[reference.into()], defaults, 1).unwrap_err(),
            "git-worktree-reference-invalid"
        );
    }
    assert_eq!(
        audit_git_worktrees(root.path(), &["x".repeat(1_025)], defaults, 1).unwrap_err(),
        "git-worktree-reference-invalid"
    );
}

#[test]
fn retained_primary_worktree_is_preserved_with_privacy_safe_public_summary() {
    let root = initialized_repository();
    let report = audit_git_worktrees(
        root.path(),
        &["HEAD".into(), "HEAD".into()],
        GitWorktreeAuditOptions::default(),
        123,
    )
    .unwrap();

    assert_eq!(report.schema_kind, GIT_WORKTREE_AUDIT_SCHEMA_KIND);
    assert_eq!(report.version, 2);
    assert_eq!(report.generated_at_ms, 123);
    assert_eq!(report.retention_references.len(), 1);
    assert_eq!(report.retention_reference_set_fingerprint.len(), 64);
    assert!(report.retention_reachable_commit_count >= 1);
    assert_eq!(report.worktree_count, 1);
    assert_eq!(report.removal_candidate_count, 0);
    assert_eq!(report.preserved_count, 1);
    assert_eq!(report.evidence_gap_count, 0);
    assert!(report.evidence_complete);
    assert_eq!(report.exact_approval_phrase, None);
    assert!(!report.filesystem_mutation_executed);

    let entry = &report.entries[0];
    assert!(entry.primary);
    assert!(entry.audit_origin);
    assert!(entry.head_is_retained_tip);
    assert_eq!(entry.status_clean, Some(true));
    assert_eq!(entry.status_entry_count, Some(0));
    assert_eq!(entry.contained_in_reference, Some(true));
    assert_eq!(entry.disposition, GitWorktreeDisposition::Preserve);
    for blocker in ["primary-worktree", "audit-origin-worktree", "head-is-retained-tip"] {
        assert!(entry.blockers.contains(&blocker.to_string()));
    }
    assert!(entry.size.evidence_complete);
    assert!(!entry.active_use.assessed);
    assert_eq!(
        entry.active_use.error.as_deref(),
        Some("active-use-not-needed-for-preserved-worktree")
    );
    assert_eq!(entry.path_fingerprint.len(), 64);
    assert_eq!(entry.entry_fingerprint.len(), 64);

    let summary = public_summary(&report);
    assert_eq!(summary.schema_kind, GIT_WORKTREE_AUDIT_SCHEMA_KIND);
    assert_eq!(summary.worktree_count, 1);
    assert!(summary.local_paths_redacted);
    assert!(summary.branch_names_redacted);
    assert!(summary
        .metadata_semantics
        .contains(&"user-file-production-time-not-inferred".to_string()));
    assert!(summary.notices.contains(&"read-only-audit".to_string()));
    let serialized = serde_json::to_string(&summary).unwrap();
    assert!(!serialized.contains(root.path().to_string_lossy().as_ref()));
    assert!(!serialized.contains("refs/heads/"));

    assert_eq!(
        approve_stale_worktree_removal(
            &report,
            "not-authorized",
            124,
            "human:test",
            "Reviewed retained primary worktree",
        )
        .unwrap_err(),
        "git-worktree-removal-audit-not-executable"
    );
}

#[test]
fn removal_approval_rejects_tampered_audit_evidence_before_authorization() {
    let root = initialized_repository();
    let report = audit_git_worktrees(
        root.path(),
        &["HEAD".into()],
        GitWorktreeAuditOptions::default(),
        700,
    )
    .unwrap();

    let reject = |candidate: &disksage_lib::git_worktree::GitWorktreeAuditReport| {
        approve_stale_worktree_removal(
            candidate,
            "not-authorized",
            701,
            "human:test",
            "Review tamper-resistant worktree evidence",
        )
        .unwrap_err()
    };

    let mut invalid_envelope = report.clone();
    invalid_envelope.schema_kind = "disksage.git-worktree-audit/forged".into();
    assert_eq!(
        reject(&invalid_envelope),
        "git-worktree-removal-audit-integrity-invalid"
    );

    let mut invalid_reference_binding = report.clone();
    invalid_reference_binding.retention_reference_set_fingerprint = "0".repeat(64);
    assert_eq!(
        reject(&invalid_reference_binding),
        "git-worktree-removal-reference-binding-mismatch"
    );

    let mut invalid_entry = report.clone();
    invalid_entry.entries[0].path_fingerprint = "0".repeat(64);
    assert_eq!(
        reject(&invalid_entry),
        "git-worktree-removal-entry-integrity-mismatch"
    );

    let mut invalid_summary = report;
    invalid_summary.worktree_count = invalid_summary.worktree_count.saturating_add(1);
    assert_eq!(
        reject(&invalid_summary),
        "git-worktree-removal-audit-summary-mismatch"
    );
}

#[test]
fn dirty_primary_state_is_observed_without_becoming_removal_authority() {
    let root = initialized_repository();
    std::fs::write(root.path().join("untracked.txt"), b"local-only\n").unwrap();

    let report = audit_git_worktrees(
        root.path(),
        &["HEAD".into()],
        GitWorktreeAuditOptions::default(),
        456,
    )
    .unwrap();
    let entry = &report.entries[0];
    assert_eq!(entry.status_clean, Some(false));
    assert_eq!(entry.status_entry_count, Some(1));
    assert!(entry.blockers.contains(&"worktree-dirty".to_string()));
    assert_eq!(entry.disposition, GitWorktreeDisposition::Preserve);
    assert_eq!(report.removal_candidate_count, 0);
    assert!(!report.filesystem_mutation_executed);
}
