//! Real-process coverage for the Git worktree active-use safety boundary.
//!
//! A linked worktree that would otherwise be a removal candidate must be preserved while an
//! independent process has its current working directory inside that worktree. This regression
//! uses the shipped audit boundary, real Git worktrees, and the host `lsof` process probe; it never
//! removes the audited worktree or mutates user repositories.

#![cfg(unix)]

use disksage_lib::git_worktree::{
    audit_git_worktrees, GitWorktreeAuditOptions, GitWorktreeDisposition,
};
use std::path::Path;
use std::process::{Child, Command, Stdio};

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
        .expect("git fixture output must be UTF-8")
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
        .expect("write initial fixture");
    git(repository.path(), &["add", "tracked.txt"]);
    git(repository.path(), &["commit", "-m", "first"]);
    repository
}

struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn otherwise_removable_worktree_is_preserved_while_real_process_uses_it() {
    let repository = initialized_repository();
    let ancestor = git(repository.path(), &["rev-parse", "HEAD"]);

    std::fs::write(repository.path().join("tracked.txt"), b"second\n")
        .expect("advance retained fixture");
    git(repository.path(), &["add", "tracked.txt"]);
    git(repository.path(), &["commit", "-m", "second"]);

    let linked_parent = tempfile::tempdir().expect("linked worktree parent");
    let linked_path = linked_parent.path().join("active-worktree");
    let linked_path_text = linked_path.to_string_lossy().into_owned();
    git(
        repository.path(),
        &["worktree", "add", "--detach", &linked_path_text, &ancestor],
    );

    let mut child = ChildGuard(
        Command::new("sleep")
            .arg("30")
            .current_dir(&linked_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("active-use fixture process must start"),
    );
    let active_pid = child.0.id();

    let report = audit_git_worktrees(
        repository.path(),
        &["refs/heads/main".into()],
        GitWorktreeAuditOptions::default(),
        9_001,
    )
    .expect("audit must complete while the linked worktree is active");

    let linked = report
        .entries
        .iter()
        .find(|entry| entry.path == linked_path_text)
        .expect("linked worktree must be audited");

    assert_eq!(linked.status_clean, Some(true));
    assert_eq!(linked.contained_in_reference, Some(true));
    assert!(!linked.head_is_retained_tip);
    assert!(linked.active_use.assessed);
    assert!(linked.active_use.evidence_complete);
    assert!(linked.active_use.active);
    assert!(linked.active_use.observed_pids.contains(&active_pid));
    assert!(!linked.active_use.results_truncated);
    assert_eq!(linked.active_use.error, None);
    assert!(linked
        .blockers
        .iter()
        .any(|blocker| blocker == "active-use-detected"));
    assert_eq!(linked.disposition, GitWorktreeDisposition::Preserve);
    assert_eq!(report.removal_candidate_count, 0);
    assert!(!report.filesystem_mutation_executed);

    child.0.kill().expect("stop active-use fixture process");
    child.0.wait().expect("reap active-use fixture process");
}
