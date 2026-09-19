//! Real-filesystem regression for ignored build/cache debris in stale Git worktrees.
//!
//! Git-ignored artifacts are invisible to the ordinary tracked/untracked cleanliness check, but
//! deleting the worktree would still destroy local generated state. The audit must therefore keep
//! `status_clean` scoped to tracked/untracked Git state while independently blocking removal when
//! ignored artifacts are present.

use disksage_lib::git_worktree::{
    audit_git_worktrees, GitWorktreeAuditEntry, GitWorktreeAuditOptions, GitWorktreeAuditReport,
    GitWorktreeDisposition,
};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const IGNORED_ARTIFACT_BLOCKER: &str = "ignored-artifacts-present";

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
fn ignored_artifacts_preserve_otherwise_clean_removal_candidate() {
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

    let exclude = repository.join(".git").join("info").join("exclude");
    fs::create_dir_all(exclude.parent().unwrap()).expect("create Git info directory");
    fs::write(&exclude, b"build/\n").expect("ignore generated build directory");
    fs::create_dir_all(secondary.join("build")).expect("create ignored build directory");
    fs::write(secondary.join("build").join("cache.bin"), b"local generated state\n")
        .expect("write ignored artifact");

    let report = audit_git_worktrees(
        &repository,
        &["main".into()],
        GitWorktreeAuditOptions::default(),
        now_ms(),
    )
    .expect("audit real linked worktree");
    let entry = entry_for_path(&report, &secondary);

    assert_eq!(
        entry.status_clean,
        Some(true),
        "ignored debris must not be conflated with tracked/untracked Git dirtiness"
    );
    assert_ne!(
        entry.disposition,
        GitWorktreeDisposition::RemovalCandidate,
        "ignored generated state must preserve an otherwise removable worktree"
    );
    assert!(
        entry
            .blockers
            .iter()
            .any(|blocker| blocker == IGNORED_ARTIFACT_BLOCKER),
        "blockers={:?}",
        entry.blockers
    );
}
