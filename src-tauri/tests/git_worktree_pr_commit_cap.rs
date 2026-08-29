#![cfg(unix)]

use disksage_lib::git_worktree::{
    github_pull_request_commit_membership, GitWorktreeAuditOptions,
};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use std::sync::Mutex;

static PATH_ENV_LOCK: Mutex<()> = Mutex::new(());

fn init_repository(path: &std::path::Path) -> String {
    fs::create_dir_all(path).unwrap();
    Command::new("git").args(["init", "-q", "-b", "main"]).current_dir(path).status().unwrap();
    fs::write(path.join("tracked.txt"), b"fixture\n").unwrap();
    Command::new("git").args(["add", "tracked.txt"]).current_dir(path).status().unwrap();
    Command::new("git")
        .args(["-c", "user.name=DiskSage Test", "-c", "user.email=disksage@example.invalid", "commit", "-q", "-m", "fixture"])
        .current_dir(path)
        .status()
        .unwrap();
    String::from_utf8(
        Command::new("git").args(["rev-parse", "HEAD"]).current_dir(path).output().unwrap().stdout,
    )
    .unwrap()
    .trim()
    .to_string()
}

#[test]
fn exact_250_commit_pr_list_is_incomplete_evidence() {
    let _guard = PATH_ENV_LOCK.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let repository = temp.path().join("repository");
    let head = init_repository(&repository);
    let bin_dir = temp.path().join("bin");
    fs::create_dir(&bin_dir).unwrap();
    let gh = bin_dir.join("gh");
    fs::write(
        &gh,
        format!(
            r#"#!/bin/sh
set -eu
case " $* " in
  *' repo view '*) printf '%s\n' 'ContextualWisdomLab/disksage' ;;
  *' api --paginate --slurp repos/ContextualWisdomLab/disksage/commits/{head}/pulls?per_page=100 '*)
    printf '%s' '[[{{"number":1,"state":"open","base":{{"repo":{{"full_name":"ContextualWisdomLab/disksage"}}}}}}]]'
    ;;
  *' api --paginate repos/ContextualWisdomLab/disksage/pulls/1/commits?per_page=100 '*)
    printf '%s\n' '{head}'
    i=1
    while [ "$i" -lt 250 ]; do
      printf '%040x\n' "$i"
      i=$((i + 1))
    done
    ;;
  *) exit 64 ;;
esac
"#
        ),
    )
    .unwrap();
    let mut permissions = fs::metadata(&gh).unwrap().permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&gh, permissions).unwrap();

    let original_path = std::env::var_os("PATH");
    let mut paths = vec![bin_dir];
    if let Some(existing) = original_path.as_ref() {
        paths.extend(std::env::split_paths(existing));
    }
    std::env::set_var("PATH", std::env::join_paths(paths).unwrap());
    let result = github_pull_request_commit_membership(
        &repository,
        GitWorktreeAuditOptions { command_timeout_ms: 5_000, ..GitWorktreeAuditOptions::default() },
    );
    match original_path {
        Some(value) => std::env::set_var("PATH", value),
        None => std::env::remove_var("PATH"),
    }

    assert_eq!(result.unwrap_err(), "github-pr-commit-count-exceeds-limit");
}
