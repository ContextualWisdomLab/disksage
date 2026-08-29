#![cfg(unix)]

use disksage_lib::git_worktree::{
    github_closed_pull_request_heads, github_closed_pull_request_heads_with_options,
    GitWorktreeAuditOptions,
};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use std::sync::Mutex;

static PATH_ENV_LOCK: Mutex<()> = Mutex::new(());

fn init_fixture_repository(path: &std::path::Path, branch: &str) -> String {
    fs::create_dir_all(path).expect("create fixture repository root");
    Command::new("git")
        .args(["init", "-q", "-b", branch])
        .current_dir(path)
        .status()
        .expect("initialize fixture repository");
    fs::write(path.join("tracked.txt"), b"fixture\n").expect("write tracked fixture");
    Command::new("git")
        .args(["add", "tracked.txt"])
        .current_dir(path)
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
        .current_dir(path)
        .status()
        .expect("commit fixture");
    let head = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(path)
        .output()
        .expect("resolve fixture head");
    String::from_utf8(head.stdout).unwrap().trim().to_string()
}

#[test]
fn merged_pull_request_query_is_scoped_to_registered_branch() {
    let _env_guard = PATH_ENV_LOCK.lock().expect("serialize PATH mutation");
    let temp = tempfile::tempdir().expect("temporary repository root");
    let head = init_fixture_repository(temp.path(), "merged-work");

    let bin_dir = temp.path().join("bin");
    fs::create_dir(&bin_dir).expect("create fake bin directory");
    let gh_path = bin_dir.join("gh");
    fs::write(
        &gh_path,
        format!(
            "#!/bin/sh\nset -eu\ncase \" $* \" in\n  *' --state closed '*) printf '[]' ;;\n  *' --state open '*) printf '[]' ;;\n  *' --state merged --head merged-work '*) printf '%s' '[{{\"headRefName\":\"merged-work\",\"headRefOid\":\"{head}\",\"isCrossRepository\":false,\"state\":\"MERGED\"}}]' ;;\n  *) exit 64 ;;\nesac\n"
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
    let result = github_closed_pull_request_heads(temp.path(), 5_000);
    match original_path {
        Some(value) => std::env::set_var("PATH", value),
        None => std::env::remove_var("PATH"),
    }

    assert_eq!(
        result.expect("branch-scoped merged evidence"),
        [("refs/heads/merged-work".to_string(), head)]
            .into_iter()
            .collect()
    );
}

#[test]
fn merged_pull_request_lookup_honors_the_callers_worktree_limit() {
    let _env_guard = PATH_ENV_LOCK.lock().expect("serialize PATH mutation");
    let temp = tempfile::tempdir().expect("temporary fixture parent");
    let repository = temp.path().join("repository");
    let linked = temp.path().join("linked");
    init_fixture_repository(&repository, "main");
    Command::new("git")
        .args(["branch", "linked-work"])
        .current_dir(&repository)
        .status()
        .expect("create linked worktree branch");
    Command::new("git")
        .args(["worktree", "add", "-q"])
        .arg(&linked)
        .arg("linked-work")
        .current_dir(&repository)
        .status()
        .expect("create linked worktree");

    let bin_dir = temp.path().join("bin");
    fs::create_dir(&bin_dir).expect("create fake bin directory");
    let gh_path = bin_dir.join("gh");
    fs::write(
        &gh_path,
        "#!/bin/sh\nset -eu\ncase \" $* \" in\n  *' --state closed '*) printf '[]' ;;\n  *' --state open '*) printf '[]' ;;\n  *) exit 64 ;;\nesac\n",
    )
    .expect("write bounded fake gh executable");
    let mut permissions = fs::metadata(&gh_path).expect("fake gh metadata").permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&gh_path, permissions).expect("make fake gh executable");

    let original_path = std::env::var_os("PATH");
    let mut paths = vec![bin_dir];
    if let Some(existing) = original_path.as_ref() {
        paths.extend(std::env::split_paths(existing));
    }
    std::env::set_var("PATH", std::env::join_paths(paths).expect("join PATH"));

    let options = GitWorktreeAuditOptions {
        max_worktrees: 1,
        ..GitWorktreeAuditOptions::default()
    };
    let result = github_closed_pull_request_heads_with_options(&repository, options);

    match original_path {
        Some(value) => std::env::set_var("PATH", value),
        None => std::env::remove_var("PATH"),
    }

    assert_eq!(result.unwrap_err(), "git-worktree-list-exceeds-limit");
}

#[test]
fn merged_pull_request_lookup_keeps_many_branch_queries_inside_one_timeout_budget() {
    let _env_guard = PATH_ENV_LOCK.lock().expect("serialize PATH mutation");
    let temp = tempfile::tempdir().expect("temporary fixture parent");
    let repository = temp.path().join("repository");
    let head = init_fixture_repository(&repository, "main");

    for index in 0..3 {
        let branch = format!("merged-work-{index}");
        let linked = temp.path().join(format!("linked-{index}"));
        Command::new("git")
            .args(["branch", &branch])
            .current_dir(&repository)
            .status()
            .expect("create linked worktree branch");
        Command::new("git")
            .args(["worktree", "add", "-q"])
            .arg(&linked)
            .arg(&branch)
            .current_dir(&repository)
            .status()
            .expect("create linked worktree");
    }

    let bin_dir = temp.path().join("bin");
    fs::create_dir(&bin_dir).expect("create fake bin directory");
    let gh_path = bin_dir.join("gh");
    fs::write(
        &gh_path,
        format!(
            r#"#!/bin/sh
set -eu
case " $* " in
  *' --state closed '*) printf '[]'; exit 0 ;;
  *' --state open '*) sleep 0.20; printf '[]'; exit 0 ;;
  *' --state merged --head '*)
    branch=''
    previous=''
    for argument in "$@"; do
      if [ "$previous" = '--head' ]; then branch="$argument"; break; fi
      previous="$argument"
    done
    sleep 0.20
    printf '[{{"headRefName":"%s","headRefOid":"{head}","isCrossRepository":false,"state":"MERGED"}}]' "$branch"
    ;;
  *) exit 64 ;;
esac
"#
        ),
    )
    .expect("write delayed fake gh executable");
    let mut permissions = fs::metadata(&gh_path).expect("fake gh metadata").permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&gh_path, permissions).expect("make fake gh executable");

    let original_path = std::env::var_os("PATH");
    let mut paths = vec![bin_dir];
    if let Some(existing) = original_path.as_ref() {
        paths.extend(std::env::split_paths(existing));
    }
    std::env::set_var("PATH", std::env::join_paths(paths).expect("join PATH"));
    let options = GitWorktreeAuditOptions {
        command_timeout_ms: 500,
        max_worktrees: 8,
        ..GitWorktreeAuditOptions::default()
    };
    let result = github_closed_pull_request_heads_with_options(&repository, options);
    match original_path {
        Some(value) => std::env::set_var("PATH", value),
        None => std::env::remove_var("PATH"),
    }

    let expected = ["main", "merged-work-0", "merged-work-1", "merged-work-2"]
        .into_iter()
        .map(|branch| (format!("refs/heads/{branch}"), head.clone()))
        .collect();
    assert_eq!(result.expect("bounded concurrent merged lookup"), expected);
}
