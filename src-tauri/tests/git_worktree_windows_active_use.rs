#![cfg(windows)]

use disksage_lib::git_worktree::{
    audit_git_worktrees, GitWorktreeAuditOptions, GitWorktreeDisposition,
};
use std::fs;
use std::path::Path;
use std::process::Command;

fn git(cwd: &Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("git must be available on the hosted Windows runner");
    assert!(
        output.status.success(),
        "git {args:?} failed\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn open_worktree_file_reports_complete_restart_manager_evidence_with_holder_pid() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let secondary = temporary.path().join("stale-worktree");
    fs::create_dir(&repository).unwrap();

    git(&repository, &["init", "-b", "main"]);
    git(&repository, &["config", "user.email", "disksage-test@example.invalid"]);
    git(&repository, &["config", "user.name", "DiskSage Test"]);

    fs::write(repository.join("held.bin"), b"committed worktree payload\n").unwrap();
    git(&repository, &["add", "held.bin"]);
    git(&repository, &["commit", "-m", "base"]);
    git(&repository, &["branch", "stale"]);

    fs::write(repository.join("main-only.txt"), b"main advances\n").unwrap();
    git(&repository, &["add", "main-only.txt"]);
    git(&repository, &["commit", "-m", "advance main"]);
    git(
        &repository,
        &["worktree", "add", secondary.to_str().unwrap(), "stale"],
    );

    let held_path = secondary.join("held.bin");
    let _held = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&held_path)
        .unwrap();

    let report = audit_git_worktrees(
        &repository,
        &["refs/heads/main".into()],
        GitWorktreeAuditOptions::default(),
        42,
    )
    .unwrap();
    let entry = report
        .entries
        .iter()
        .find(|entry| entry.branch.as_deref() == Some("refs/heads/stale"))
        .expect("secondary worktree must be present in the audit");

    assert!(entry.active_use.assessed, "{entry:#?}");
    assert!(entry.active_use.evidence_complete, "{entry:#?}");
    assert!(entry.active_use.active, "{entry:#?}");
    assert!(
        entry.active_use.observed_pids.contains(&std::process::id()),
        "{entry:#?}"
    );
    assert_eq!(entry.disposition, GitWorktreeDisposition::Preserve);
}
