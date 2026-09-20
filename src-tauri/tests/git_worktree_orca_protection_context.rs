//! Real-filesystem contract for live Orca protection in the canonical v4 worktree audit.
//!
//! The linked worktree is otherwise safely removable because its HEAD is already contained in the
//! selected retention reference. Supplying live Orca terminal evidence must therefore be the only
//! reason it is preserved. This keeps live protection evidence inside the same audit/fingerprint
//! boundary that later execution re-audits.

use disksage_lib::git_worktree::{
    approve_stale_worktree_removal, audit_git_worktrees, execute_stale_worktree_removal,
    GitWorktreeAuditEntry, GitWorktreeAuditOptions, GitWorktreeAuditReport,
    GitWorktreeDisposition,
};
use disksage_lib::reclaim_protection::{ProtectionContext, REASON_ORCA_TERMINAL_LIVE};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

fn git(cwd: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_AUTHOR_NAME", "DiskSage Test")
        .env("GIT_AUTHOR_EMAIL", "disksage@example.invalid")
        .env("GIT_COMMITTER_NAME", "DiskSage Test")
        .env("GIT_COMMITTER_EMAIL", "disksage@example.invalid")
        .output()
        .expect("git should spawn");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn entry_for_path<'a>(report: &'a GitWorktreeAuditReport, path: &Path) -> &'a GitWorktreeAuditEntry {
    let canonical = fs::canonicalize(path).expect("canonicalize linked worktree");
    report
        .entries
        .iter()
        .find(|entry| Path::new(&entry.path) == canonical.as_path())
        .expect("linked worktree must be present in audit")
}

#[test]
fn live_orca_terminal_preserves_otherwise_removable_worktree() {
    let temp = tempfile::tempdir().expect("temporary fixture root");
    let repository = temp.path().join("repository");
    let secondary = temp.path().join("secondary");
    fs::create_dir(&repository).expect("create repository root");

    git(&repository, &["init", "-q", "-b", "main"]);
    fs::write(repository.join("evidence.txt"), b"first\n").expect("write first revision");
    git(&repository, &["add", "evidence.txt"]);
    git(&repository, &["commit", "-q", "-m", "first"]);
    fs::write(repository.join("evidence.txt"), b"second\n").expect("write second revision");
    git(&repository, &["commit", "-q", "-am", "second"]);
    git(&repository, &["branch", "stale", "HEAD~1"]);
    git(
        &repository,
        &["worktree", "add", "-q", secondary.to_str().unwrap(), "stale"],
    );

    let baseline = audit_git_worktrees(
        &repository,
        &["main".into()],
        GitWorktreeAuditOptions::default(),
        now_ms(),
    )
    .expect("baseline audit real linked worktree without live Orca evidence");
    let baseline_entry = entry_for_path(&baseline, &secondary);
    assert_eq!(
        baseline_entry.disposition,
        GitWorktreeDisposition::RemovalCandidate,
        "{baseline_entry:#?}"
    );
    let baseline_fingerprint = baseline_entry.entry_fingerprint.clone();

    let live_path = fs::canonicalize(&secondary).expect("canonical live worktree path");
    let options = GitWorktreeAuditOptions {
        protection: ProtectionContext {
            orca_live_worktree_paths: vec![live_path],
            ..ProtectionContext::default()
        },
        ..GitWorktreeAuditOptions::default()
    };
    let report = audit_git_worktrees(&repository, &["main".into()], options, now_ms())
        .expect("audit real linked worktree with live Orca evidence");
    let entry = entry_for_path(&report, &secondary);

    assert_eq!(entry.status_clean, Some(true), "{entry:#?}");
    assert_eq!(entry.contained_in_reference, Some(true), "{entry:#?}");
    assert!(
        entry
            .blockers
            .iter()
            .any(|blocker| blocker == REASON_ORCA_TERMINAL_LIVE),
        "blockers={:?}",
        entry.blockers
    );
    assert_eq!(entry.disposition, GitWorktreeDisposition::Preserve);
    assert_ne!(
        entry.entry_fingerprint, baseline_fingerprint,
        "protection authority that changes the decision must change entry integrity"
    );
    assert_eq!(report.removal_candidate_count, 0, "{report:#?}");
    assert_eq!(report.exact_approval_phrase, None);
}

#[test]
fn live_orca_terminal_arriving_after_approval_vetoes_removal_before_mutation() {
    let temp = tempfile::tempdir().expect("temporary fixture root");
    let repository = temp.path().join("repository");
    let secondary = temp.path().join("secondary");
    fs::create_dir(&repository).expect("create repository root");

    git(&repository, &["init", "-q", "-b", "main"]);
    fs::write(repository.join("evidence.txt"), b"first\n").expect("write first revision");
    git(&repository, &["add", "evidence.txt"]);
    git(&repository, &["commit", "-q", "-m", "first"]);
    fs::write(repository.join("evidence.txt"), b"second\n").expect("write second revision");
    git(&repository, &["commit", "-q", "-am", "second"]);
    git(&repository, &["branch", "stale", "HEAD~1"]);
    git(
        &repository,
        &["worktree", "add", "-q", secondary.to_str().unwrap(), "stale"],
    );

    let audited_at_ms = now_ms();
    let approved_report = audit_git_worktrees(
        &repository,
        &["main".into()],
        GitWorktreeAuditOptions::default(),
        audited_at_ms,
    )
    .expect("initial audit should observe an otherwise-removable linked worktree");
    assert_eq!(approved_report.removal_candidate_count, 1, "{approved_report:#?}");
    let approval_phrase = approved_report
        .exact_approval_phrase
        .as_deref()
        .expect("otherwise-removable worktree must require an exact approval phrase");
    let approved_at_ms = audited_at_ms.saturating_add(1);
    let approval = approve_stale_worktree_removal(
        &approved_report,
        approval_phrase,
        approved_at_ms,
        "human:disksage-test",
        "Verify that newly observed live protection vetoes deletion before mutation.",
    )
    .expect("bind approval to the initial removable plan");

    let live_path = fs::canonicalize(&secondary).expect("canonical live worktree path");
    let execution_options = GitWorktreeAuditOptions {
        protection: ProtectionContext {
            orca_live_worktree_paths: vec![live_path],
            ..ProtectionContext::default()
        },
        ..GitWorktreeAuditOptions::default()
    };
    let execution = execute_stale_worktree_removal(
        &approved_report,
        &approval,
        approval_phrase,
        execution_options,
        approved_at_ms.saturating_add(1),
    );

    assert!(
        execution.is_err(),
        "a worktree that becomes live after approval must fail closed before mutation"
    );
    assert!(secondary.exists(), "live worktree path must not be removed");
    assert_eq!(
        fs::read_to_string(secondary.join("evidence.txt")).expect("read preserved worktree file"),
        "first\n"
    );
}
