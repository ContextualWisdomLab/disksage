#![cfg(unix)]

use disksage_lib::git_worktree::github_closed_pull_request_heads;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use std::sync::Mutex;

static PATH_ENV_LOCK: Mutex<()> = Mutex::new(());

fn init_repository(path: &std::path::Path) -> String {
    Command::new("git")
        .args(["init", "-q", "-b", "main"])
        .current_dir(path)
        .status()
        .expect("initialize fixture repository");
    fs::write(path.join("tracked.txt"), b"fixture\n").expect("write fixture");
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
    String::from_utf8(
        Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(path)
            .output()
            .expect("resolve fixture head")
            .stdout,
    )
    .unwrap()
    .trim()
    .to_string()
}

#[test]
fn closed_and_merged_heads_use_one_paginated_rest_request_with_open_veto() {
    let _env_guard = PATH_ENV_LOCK.lock().expect("serialize PATH mutation");
    let temp = tempfile::tempdir().expect("temporary repository root");
    let head = init_repository(temp.path());
    let output_path = temp.path().join("pull-requests.json");
    fs::write(
        &output_path,
        format!(
            "{{\"number\":1,\"headRefName\":\"closed-work\",\"headRefOid\":\"{head}\",\"isCrossRepository\":false,\"createdAt\":\"2026-01-01T00:00:00Z\",\"state\":\"CLOSED\"}}\n{{\"number\":2,\"headRefName\":\"merged-work\",\"headRefOid\":\"{head}\",\"isCrossRepository\":false,\"createdAt\":\"2026-01-01T00:00:00Z\",\"state\":\"MERGED\"}}\n{{\"number\":3,\"headRefName\":\"closed-work\",\"headRefOid\":\"{head}\",\"isCrossRepository\":false,\"createdAt\":\"2026-01-01T00:00:00Z\",\"state\":\"OPEN\"}}\n"
        ),
    )
    .expect("write fake REST response");

    let bin_dir = temp.path().join("bin");
    fs::create_dir(&bin_dir).expect("create fake bin directory");
    let gh_path = bin_dir.join("gh");
    fs::write(
        &gh_path,
        "#!/bin/sh\nset -eu\nprintf '%s\\n' \"$*\" >> \"$DISKSAGE_FAKE_GH_LOG\"\ncase \" $* \" in\n  *' api --paginate repos/{owner}/{repo}/pulls?state=all&per_page=100 --jq '*) cat \"$DISKSAGE_FAKE_GH_OUTPUT\" ;;\n  *) exit 64 ;;\nesac\n",
    )
    .expect("write fake gh executable");
    let mut permissions = fs::metadata(&gh_path)
        .expect("fake gh metadata")
        .permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&gh_path, permissions).expect("make fake gh executable");

    let log_path = temp.path().join("gh.log");
    let original_path = std::env::var_os("PATH");
    let mut paths = vec![bin_dir];
    if let Some(existing) = original_path.as_ref() {
        paths.extend(std::env::split_paths(existing));
    }
    std::env::set_var("PATH", std::env::join_paths(paths).expect("join PATH"));
    std::env::set_var("DISKSAGE_FAKE_GH_OUTPUT", &output_path);
    std::env::set_var("DISKSAGE_FAKE_GH_LOG", &log_path);

    let result = github_closed_pull_request_heads(temp.path(), 5_000);

    match original_path {
        Some(value) => std::env::set_var("PATH", value),
        None => std::env::remove_var("PATH"),
    }
    std::env::remove_var("DISKSAGE_FAKE_GH_OUTPUT");
    std::env::remove_var("DISKSAGE_FAKE_GH_LOG");

    assert_eq!(
        result.expect("paginated REST evidence"),
        [("refs/heads/merged-work".to_string(), head)]
            .into_iter()
            .collect()
    );
    assert_eq!(
        fs::read_to_string(log_path)
            .expect("read fake gh log")
            .lines()
            .count(),
        1
    );
}
