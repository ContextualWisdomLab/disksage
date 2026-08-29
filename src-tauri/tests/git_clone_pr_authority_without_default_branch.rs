#![cfg(unix)]

use disksage_lib::git_clone_reclaim::plan_git_clone_reclaim_with_default_branch;
use disksage_lib::git_worktree::GitWorktreeAuditOptions;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;
use std::sync::Mutex;

static PATH_LOCK: Mutex<()> = Mutex::new(());

fn git(repository: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repository)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}

#[test]
fn closed_pr_authority_does_not_require_default_branch_provider_evidence() {
    let _path_guard = PATH_LOCK.lock().unwrap();
    let repository = tempfile::tempdir().unwrap();
    git(repository.path(), &["init", "-b", "main"]);
    git(
        repository.path(),
        &["config", "user.email", "clone@example.invalid"],
    );
    git(
        repository.path(),
        &["config", "user.name", "DiskSage Clone Test"],
    );
    std::fs::write(repository.path().join("tracked.txt"), b"main\n").unwrap();
    git(repository.path(), &["add", "tracked.txt"]);
    git(repository.path(), &["commit", "-m", "main"]);
    git(repository.path(), &["switch", "-c", "old-pr"]);
    std::fs::write(repository.path().join("tracked.txt"), b"old pr\n").unwrap();
    git(repository.path(), &["commit", "-am", "old pr"]);
    let head = git(repository.path(), &["rev-parse", "HEAD"]);

    let fake_bin = tempfile::tempdir().unwrap();
    let fake_gh = fake_bin.path().join("gh");
    std::fs::write(
        &fake_gh,
        format!(
            r#"#!/bin/sh
case "$*" in
  *"--state closed"*)
    printf '%s\n' '[{{"headRefName":"old-pr","headRefOid":"{head}","isCrossRepository":false,"state":"CLOSED"}}]'
    exit 0
    ;;
  *"--state merged"*|*"--state open"*)
    printf '%s\n' '[]'
    exit 0
    ;;
  "repo view"*)
    printf '%s\n' 'default branch unavailable' >&2
    exit 42
    ;;
  *)
    printf 'unexpected gh invocation: %s\n' "$*" >&2
    exit 43
    ;;
esac
"#
        ),
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&fake_gh).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&fake_gh, permissions).unwrap();

    let original_path = std::env::var_os("PATH").unwrap_or_default();
    let combined_path = std::env::join_paths(
        std::iter::once(fake_bin.path().to_path_buf()).chain(std::env::split_paths(&original_path)),
    )
    .unwrap();
    unsafe { std::env::set_var("PATH", &combined_path) };

    let plan = plan_git_clone_reclaim_with_default_branch(
        repository.path(),
        &["refs/heads/main".into()],
        true,
        None,
        GitWorktreeAuditOptions::default(),
        10,
    );

    unsafe { std::env::set_var("PATH", original_path) };
    let plan = plan.expect("fresh closed-PR authority must not depend on default-branch lookup");
    assert!(plan.closed_pull_request_head);
    assert!(!plan.stale_open_pull_request_head);
    assert!(plan.default_branch_reference.is_none());
    assert!(plan.default_branch_oid.is_none());
    assert!(plan.default_branch_observed_at_ms.is_none());
    assert!(plan.eligible_after_human_approval, "{:?}", plan.blockers);
}

#[test]
fn locally_blocked_non_pr_clone_does_not_require_default_branch_provider_evidence() {
    let _path_guard = PATH_LOCK.lock().unwrap();
    let repository = tempfile::tempdir().unwrap();
    git(repository.path(), &["init", "-b", "main"]);
    git(
        repository.path(),
        &["config", "user.email", "clone@example.invalid"],
    );
    git(
        repository.path(),
        &["config", "user.name", "DiskSage Clone Test"],
    );
    std::fs::write(repository.path().join("tracked.txt"), b"main\n").unwrap();
    git(repository.path(), &["add", "tracked.txt"]);
    git(repository.path(), &["commit", "-m", "main"]);
    git(repository.path(), &["switch", "-c", "blocked-local"]);
    std::fs::write(repository.path().join("tracked.txt"), b"branch\n").unwrap();
    git(repository.path(), &["commit", "-am", "branch"]);
    std::fs::write(repository.path().join("tracked.txt"), b"dirty\n").unwrap();

    let fake_bin = tempfile::tempdir().unwrap();
    let fake_gh = fake_bin.path().join("gh");
    std::fs::write(
        &fake_gh,
        r#"#!/bin/sh
case "$*" in
  *"--state closed"*|*"--state merged"*|*"--state open"*)
    printf '%s\n' '[]'
    exit 0
    ;;
  "repo view"*)
    printf '%s\n' 'default branch unavailable' >&2
    exit 42
    ;;
  *)
    printf 'unexpected gh invocation: %s\n' "$*" >&2
    exit 43
    ;;
esac
"#,
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&fake_gh).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&fake_gh, permissions).unwrap();

    let original_path = std::env::var_os("PATH").unwrap_or_default();
    let combined_path = std::env::join_paths(
        std::iter::once(fake_bin.path().to_path_buf()).chain(std::env::split_paths(&original_path)),
    )
    .unwrap();
    unsafe { std::env::set_var("PATH", &combined_path) };

    let plan = plan_git_clone_reclaim_with_default_branch(
        repository.path(),
        &["refs/heads/main".into()],
        true,
        None,
        GitWorktreeAuditOptions::default(),
        10,
    );

    unsafe { std::env::set_var("PATH", original_path) };
    let plan = plan.expect("local blockers must remain observable while GitHub is unavailable");
    assert!(!plan.closed_pull_request_head);
    assert!(!plan.stale_open_pull_request_head);
    assert!(plan.default_branch_reference.is_none());
    assert!(plan.default_branch_oid.is_none());
    assert!(plan.default_branch_observed_at_ms.is_none());
    assert!(!plan.eligible_after_human_approval);
    assert!(
        plan.blockers
            .iter()
            .any(|blocker| blocker == "git-clone-working-tree-not-clean"),
        "{:?}",
        plan.blockers
    );
    assert!(
        plan.blockers
            .iter()
            .any(|blocker| blocker == "git-clone-pr-head-authority-missing"),
        "{:?}",
        plan.blockers
    );
}
