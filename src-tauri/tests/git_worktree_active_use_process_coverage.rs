//! Real-process coverage for the Git worktree active-use safety boundary.
//!
//! A linked worktree that would otherwise be a removal candidate must be preserved while an
//! independent process has its current working directory inside that worktree. These regressions
//! use the shipped audit boundary, real Git worktrees, and the host `lsof` process probe; they never
//! remove the audited worktree or mutate user repositories.

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

fn lsof_probe_available() -> bool {
    Command::new("lsof")
        .arg("-v")
        .output()
        .is_ok_and(|output| output.status.success())
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

fn detached_ancestor_worktree(repository: &Path, name: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let ancestor = git(repository, &["rev-parse", "HEAD"]);
    std::fs::write(repository.join("tracked.txt"), b"second\n")
        .expect("advance retained fixture");
    git(repository, &["add", "tracked.txt"]);
    git(repository, &["commit", "-m", "second"]);

    let linked_parent = tempfile::tempdir().expect("linked worktree parent");
    let linked_path = linked_parent.path().join(name);
    let linked_path_text = linked_path.to_string_lossy().into_owned();
    git(
        repository,
        &["worktree", "add", "--detach", &linked_path_text, &ancestor],
    );
    (linked_parent, linked_path)
}

struct ChildGuard(Child);

impl ChildGuard {
    fn sleeping_in(cwd: &Path) -> Self {
        Self(
            Command::new("sleep")
                .arg("30")
                .current_dir(cwd)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("active-use fixture process must start"),
        )
    }

    fn id(&self) -> u32 {
        self.0.id()
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn otherwise_removable_worktree_is_preserved_while_real_process_uses_it() {
    if !lsof_probe_available() {
        return;
    }
    let repository = initialized_repository();
    let (_linked_parent, linked_path) =
        detached_ancestor_worktree(repository.path(), "active-worktree");
    let linked_path_text = std::fs::canonicalize(&linked_path)
        .expect("linked worktree path must canonicalize")
        .to_string_lossy()
        .into_owned();
    let child = ChildGuard::sleeping_in(&linked_path);
    let active_pid = child.id();

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
}

#[test]
fn active_process_evidence_truncation_never_grants_removal_authority() {
    if !lsof_probe_available() {
        return;
    }
    let repository = initialized_repository();
    let (_linked_parent, linked_path) =
        detached_ancestor_worktree(repository.path(), "busy-worktree");
    let linked_path_text = std::fs::canonicalize(&linked_path)
        .expect("linked worktree path must canonicalize")
        .to_string_lossy()
        .into_owned();
    let first = ChildGuard::sleeping_in(&linked_path);
    let second = ChildGuard::sleeping_in(&linked_path);
    let active_pids = [first.id(), second.id()];
    let options = GitWorktreeAuditOptions {
        max_active_pids: 1,
        ..GitWorktreeAuditOptions::default()
    };

    let report = audit_git_worktrees(
        repository.path(),
        &["refs/heads/main".into()],
        options,
        9_002,
    )
    .expect("audit must remain fail-closed when active-use evidence is truncated");

    let linked = report
        .entries
        .iter()
        .find(|entry| entry.path == linked_path_text)
        .expect("linked worktree must be audited");

    assert!(linked.active_use.assessed);
    assert!(!linked.active_use.evidence_complete);
    assert!(linked.active_use.active);
    assert_eq!(linked.active_use.observed_pids.len(), 1);
    assert!(active_pids.contains(&linked.active_use.observed_pids[0]));
    assert!(linked.active_use.results_truncated);
    assert_eq!(linked.active_use.error.as_deref(), Some("active-use-pid-limit"));
    assert!(linked
        .blockers
        .iter()
        .any(|blocker| blocker == "active-use-evidence-incomplete"));
    assert_eq!(linked.disposition, GitWorktreeDisposition::EvidenceGap);
    assert_eq!(report.removal_candidate_count, 0);
    assert_eq!(report.evidence_gap_count, 1);
    assert!(!report.evidence_complete);
    assert!(!report.filesystem_mutation_executed);
}
