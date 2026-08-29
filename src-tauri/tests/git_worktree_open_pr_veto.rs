#![cfg(unix)]

use disksage_lib::git_worktree::github_closed_pull_request_heads;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use std::sync::Mutex;

static PATH_ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn exact_open_pull_request_vetoes_historical_merged_worktree_authority() {
    let _env_guard = PATH_ENV_LOCK.lock().expect("serialize PATH mutation");
    let temp = tempfile::tempdir().expect("temporary fixture root");
    let repository = temp.path().join("repository");
    fs::create_dir_all(&repository).expect("create repository root");
    Command::new("git")
        .args(["init", "-q", "-b", "shared-head"])
        .current_dir(&repository)
        .status()
        .expect("initialize fixture repository");
    fs::write(repository.join("tracked.txt"), b"fixture\n").expect("write fixture");
    Command::new("git")
        .args(["add", "tracked.txt"])
        .current_dir(&repository)
        .status()
        .expect("stage fixture");
    Command::new("git")
        .args([
            "-c",
            "user.name=DiskSage Test",
            "-c",
            "user.email=disksage@example.invalid",
            "commit",
            "-q",
            "-m",
            "fixture",
        ])
        .current_dir(&repository)
        .status()
        .expect("commit fixture");
    let head = String::from_utf8(
        Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&repository)
            .output()
            .expect("resolve fixture head")
            .stdout,
    )
    .unwrap();
    let head = head.trim();

    let bin_dir = temp.path().join("bin");
    fs::create_dir(&bin_dir).expect("create fake bin directory");
    let gh_path = bin_dir.join("gh");
    fs::write(
        &gh_path,
        format!(
            r#"#!/bin/sh
set -eu
case " $* " in
  *' --state closed '*) printf '[]' ;;
  *' --state merged --head shared-head '*) printf '%s' '[{{"headRefName":"shared-head","headRefOid":"{head}","isCrossRepository":false,"state":"MERGED"}}]' ;;
  *' --state open --head shared-head '*) printf '%s' '[{{"headRefName":"shared-head","headRefOid":"{head}","isCrossRepository":false,"state":"OPEN"}}]' ;;
  *) exit 64 ;;
esac
"#
        ),
    )
    .expect("write fake gh executable");
    let mut permissions = fs::metadata(&gh_path).expect("fake gh metadata").permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&gh_path, permissions).expect("make fake gh executable");

    let original_path = std::env::var_os("PATH");
    let mut paths = vec![bin_dir];
    if let Some(existing) = original_path.as_ref() {
        paths.extend(std::env::split_paths(existing));
    }
    std::env::set_var("PATH", std::env::join_paths(paths).expect("join PATH"));
    let result = github_closed_pull_request_heads(&repository, 5_000);
    match original_path {
        Some(value) => std::env::set_var("PATH", value),
        None => std::env::remove_var("PATH"),
    }

    assert!(
        result.expect("bounded pull-request evidence").is_empty(),
        "an exact current open PR must veto historical merged authority for the same branch/head"
    );
}
