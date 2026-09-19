//! Focused regression coverage for #454 safe-cleanup guards:
//! ignored artifacts must not be silent RemovalCandidates, and durable
//! merged/closed evidence is required (retention containment and/or caller admission).

#![cfg(all(unix, not(coverage)))]

use disksage_lib::git_worktree::{
    audit_git_worktrees, GitWorktreeAuditOptions, GitWorktreeDisposition,
};
use disksage_lib::reclaim_protection::REASON_IGNORED_ARTIFACTS;
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

fn rev_parse(cwd: &Path) -> String {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(cwd)
        .output()
        .expect("rev-parse should spawn");
    assert!(output.status.success());
    String::from_utf8(output.stdout)
        .expect("oid utf8")
        .trim()
        .to_ascii_lowercase()
}

fn find_entry<'a>(
    report: &'a disksage_lib::git_worktree::GitWorktreeAuditReport,
    path: &Path,
) -> &'a disksage_lib::git_worktree::GitWorktreeAuditEntry {
    let canonical = fs::canonicalize(path).expect("canonicalize worktree");
    report
        .entries
        .iter()
        .find(|entry| Path::new(&entry.path) == canonical.as_path())
        .expect("worktree entry present")
}

#[test]
fn ignored_artifacts_block_removal_without_conflating_untracked() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repository = temp.path().join("repository");
    let secondary = temp.path().join("secondary");
    fs::create_dir(&repository).unwrap();
    git(&repository, &["init", "-b", "main"]);
    fs::write(repository.join("evidence.txt"), b"first\n").unwrap();
    git(&repository, &["add", "evidence.txt"]);
    git(&repository, &["commit", "-m", "first"]);
    fs::write(repository.join("evidence.txt"), b"second\n").unwrap();
    git(&repository, &["commit", "-am", "second"]);
    git(&repository, &["branch", "merged", "HEAD~1"]);
    git(
        &repository,
        &["worktree", "add", secondary.to_str().unwrap(), "merged"],
    );

    let exclude = repository.join(".git").join("info").join("exclude");
    fs::create_dir_all(exclude.parent().unwrap()).unwrap();
    fs::write(&exclude, b"build/\n").unwrap();
    fs::create_dir_all(secondary.join("build")).unwrap();
    fs::write(secondary.join("build").join("cache.bin"), b"debris\n").unwrap();

    let report = audit_git_worktrees(
        &repository,
        &["main".into()],
        GitWorktreeAuditOptions::default(),
        now_ms(),
    )
    .expect("audit");
    let entry = find_entry(&report, &secondary);
    assert_ne!(entry.disposition, GitWorktreeDisposition::RemovalCandidate);
    assert!(
        entry
            .blockers
            .iter()
            .any(|blocker| blocker == REASON_IGNORED_ARTIFACTS),
        "blockers={:?}",
        entry.blockers
    );
    assert_eq!(entry.status_clean, Some(true));
    assert!(!entry
        .blockers
        .iter()
        .any(|blocker| blocker == "untracked-nonignored"));
}

#[test]
fn sleeping_session_maps_to_preserve_class_not_reclaimable() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repository = temp.path().join("repository");
    let secondary = temp.path().join("secondary");
    fs::create_dir(&repository).unwrap();
    git(&repository, &["init", "-b", "main"]);
    fs::write(repository.join("evidence.txt"), b"first\n").unwrap();
    git(&repository, &["add", "evidence.txt"]);
    git(&repository, &["commit", "-m", "first"]);
    fs::write(repository.join("evidence.txt"), b"second\n").unwrap();
    git(&repository, &["commit", "-am", "second"]);
    git(&repository, &["branch", "merged", "HEAD~1"]);
    git(
        &repository,
        &["worktree", "add", secondary.to_str().unwrap(), "merged"],
    );

    let mut options = GitWorktreeAuditOptions::default();
    options.protection.orca_sleep_worktree_paths = vec![fs::canonicalize(&secondary).unwrap()];
    let report = audit_git_worktrees(&repository, &["main".into()], options, now_ms())
        .expect("audit");
    let entry = find_entry(&report, &secondary);
    assert_eq!(entry.disposition, GitWorktreeDisposition::Preserve);
    assert!(entry
        .blockers
        .iter()
        .any(|code| code == "orca-session-sleeping"));
}

#[test]
fn caller_admitted_closed_merged_supplies_durable_evidence() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repository = temp.path().join("repository");
    let secondary = temp.path().join("secondary");
    fs::create_dir(&repository).unwrap();
    git(&repository, &["init", "-b", "main"]);
    fs::write(repository.join("evidence.txt"), b"first\n").unwrap();
    git(&repository, &["add", "evidence.txt"]);
    git(&repository, &["commit", "-m", "first"]);
    git(&repository, &["checkout", "-b", "unmerged"]);
    fs::write(repository.join("evidence.txt"), b"only-on-unmerged\n").unwrap();
    git(&repository, &["commit", "-am", "unmerged tip"]);
    let unmerged_head = rev_parse(&repository);
    git(&repository, &["checkout", "main"]);
    git(
        &repository,
        &["worktree", "add", secondary.to_str().unwrap(), "unmerged"],
    );

    let blocked = audit_git_worktrees(
        &repository,
        &["main".into()],
        GitWorktreeAuditOptions::default(),
        now_ms(),
    )
    .expect("audit without admission");
    let blocked_entry = find_entry(&blocked, &secondary);
    assert!(blocked_entry
        .blockers
        .iter()
        .any(|blocker| blocker == "reference-does-not-contain-head"));
    assert!(blocked_entry.merged_closed_evidence.is_none());

    let mut options = GitWorktreeAuditOptions::default();
    options.closed_merged_head_oids.push(unmerged_head);
    let report = audit_git_worktrees(&repository, &["main".into()], options, now_ms())
        .expect("audit with admission");
    let entry = find_entry(&report, &secondary);
    assert_eq!(
        entry.merged_closed_evidence.as_deref(),
        Some("caller-admitted-closed-merged")
    );
    assert_eq!(
        entry.disposition,
        GitWorktreeDisposition::RemovalCandidate,
        "blockers={:?}",
        entry.blockers
    );
}
