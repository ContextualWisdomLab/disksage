#![cfg(unix)]

use disksage_lib::git_clone_reclaim::{
    approve_git_clone_reclaim, execute_git_clone_reclaim_with_default_branch,
    plan_git_clone_reclaim_with_default_branch,
};
use disksage_lib::git_worktree::GitWorktreeAuditOptions;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;
use std::sync::Mutex;

static PATH_ENV_LOCK: Mutex<()> = Mutex::new(());

fn git(repository: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repository)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .expect("run git fixture command");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("git output is UTF-8")
        .trim()
        .into()
}

#[test]
fn execution_refreshes_default_branch_evidence_timestamp_after_old_plan() {
    let _env_guard = PATH_ENV_LOCK.lock().expect("serialize PATH mutation");
    let repository_parent = tempfile::tempdir().expect("create repository parent");
    let repository = repository_parent.path().join("clone");
    std::fs::create_dir(&repository).expect("create repository root");
    git(&repository, &["init", "-b", "main"]);
    git(&repository, &["config", "user.email", "clone@example.invalid"]);
    git(&repository, &["config", "user.name", "DiskSage Clone Test"]);
    git(
        &repository,
        &["remote", "add", "origin", "https://github.com/example/disksage-fixture.git"],
    );
    std::fs::write(repository.join("tracked.txt"), b"base\n").expect("write base payload");
    git(&repository, &["add", "tracked.txt"]);
    git(&repository, &["commit", "-m", "base"]);
    let ancestor = git(&repository, &["rev-parse", "HEAD"]);
    git(&repository, &["switch", "-c", "default-next"]);
    std::fs::write(repository.join("tracked.txt"), b"default next\n")
        .expect("write default-branch payload");
    git(&repository, &["commit", "-am", "default next"]);
    let default_oid = git(&repository, &["rev-parse", "HEAD"]);
    git(
        &repository,
        &["update-ref", "refs/remotes/origin/main", &default_oid],
    );
    git(&repository, &["switch", "--detach", &ancestor]);
    git(&repository, &["switch", "-c", "completed-local"]);

    let fake_bin = tempfile::tempdir().expect("create fake binary directory");
    let fake_gh = fake_bin.path().join("gh");
    std::fs::write(
        &fake_gh,
        format!(
            r#"#!/bin/sh
case "$*" in
  "repo view --json nameWithOwner --jq .nameWithOwner")
    printf '%s\n' 'example/disksage-fixture'
    exit 0
    ;;
  "repo view --json nameWithOwner,defaultBranchRef --jq [.nameWithOwner,.defaultBranchRef.name]|@tsv")
    printf '%s\t%s\n' 'example/disksage-fixture' 'main'
    exit 0
    ;;
  "api repos/example/disksage-fixture/commits/main --jq .sha")
    printf '%s\n' '{default_oid}'
    exit 0
    ;;
  *)
    printf 'unexpected gh invocation: %s\n' "$*" >&2
    exit 43
    ;;
esac
"#
        ),
    )
    .expect("write fake gh");
    let mut permissions = std::fs::metadata(&fake_gh)
        .expect("stat fake gh")
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&fake_gh, permissions).expect("make fake gh executable");

    let original_path = std::env::var_os("PATH").unwrap_or_default();
    let combined_path = std::env::join_paths(
        std::iter::once(fake_bin.path().to_path_buf()).chain(std::env::split_paths(&original_path)),
    )
    .expect("build fake PATH");
    unsafe { std::env::set_var("PATH", &combined_path) };

    let options = GitWorktreeAuditOptions::default();
    let plan = plan_git_clone_reclaim_with_default_branch(
        &repository,
        &["refs/remotes/origin/main".into()],
        false,
        None,
        options,
        10,
    )
    .expect("old plan is eligible from exact default-branch ancestry evidence");
    assert!(plan.eligible_after_human_approval, "{:?}", plan.blockers);
    let phrase = plan
        .exact_approval_phrase
        .as_deref()
        .expect("eligible plan has exact approval phrase");
    let approval = approve_git_clone_reclaim(
        &plan,
        phrase,
        300_011,
        "human:test",
        "fresh execution after operator review",
    )
    .expect("approve old plan immediately before execution");
    let journal_dir = tempfile::tempdir().expect("create external journal parent");
    let journal = journal_dir.path().join("journal.jsonl");

    let result = execute_git_clone_reclaim_with_default_branch(
        &plan,
        &approval,
        &["refs/remotes/origin/main".into()],
        false,
        None,
        options,
        &journal,
        300_012,
    );

    unsafe { std::env::set_var("PATH", original_path) };
    let result = result.expect(
        "fresh provider evidence must be timestamped at execution, not inherit the old plan observation",
    );
    assert!(result.trash_move_executed);
    assert!(result.path_absence_verified);
}
