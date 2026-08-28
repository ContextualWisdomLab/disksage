#![cfg(unix)]

use disksage_lib::git_worktree::github_closed_pull_request_heads;
use serde_json::json;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

#[test]
fn search_cap_warning_never_counts_as_complete_closed_pr_evidence() {
    let temp = tempfile::tempdir().expect("temporary repository root");
    Command::new("git")
        .args(["init", "-q", "-b", "main"])
        .current_dir(temp.path())
        .status()
        .expect("initialize fixture repository");
    fs::write(temp.path().join("tracked.txt"), b"fixture\n").expect("write tracked fixture");
    Command::new("git")
        .args(["add", "tracked.txt"])
        .current_dir(temp.path())
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
        .current_dir(temp.path())
        .status()
        .expect("commit fixture");
    let bin_dir = temp.path().join("bin");
    fs::create_dir(&bin_dir).expect("create fake bin directory");

    let output_path = temp.path().join("closed-prs.json");
    let records: Vec<_> = (0..1_000u64)
        .map(|index| {
            json!({
                "headRefName": format!("closed-{index}"),
                "headRefOid": format!("{index:040x}"),
                "isCrossRepository": false,
                "state": "CLOSED"
            })
        })
        .collect();
    fs::write(
        &output_path,
        serde_json::to_vec(&records).expect("serialize fake GitHub response"),
    )
    .expect("write fake GitHub response");

    let gh_path = bin_dir.join("gh");
    fs::write(
        &gh_path,
        "#!/bin/sh\nset -eu\ncat \"$DISKSAGE_FAKE_GH_OUTPUT\"\nprintf '%s\\n' 'warning: search results capped at 1000' >&2\n",
    )
    .expect("write fake gh executable");
    let mut permissions = fs::metadata(&gh_path).expect("fake gh metadata").permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&gh_path, permissions).expect("make fake gh executable");

    let original_path = std::env::var_os("PATH");
    let joined_path = match original_path.as_ref() {
        Some(existing) => {
            let mut paths = vec![bin_dir.clone()];
            paths.extend(std::env::split_paths(existing));
            std::env::join_paths(paths).expect("join PATH")
        }
        None => bin_dir.into_os_string(),
    };
    std::env::set_var("PATH", &joined_path);
    std::env::set_var("DISKSAGE_FAKE_GH_OUTPUT", &output_path);

    let result = github_closed_pull_request_heads(temp.path(), 5_000);

    match original_path {
        Some(value) => std::env::set_var("PATH", value),
        None => std::env::remove_var("PATH"),
    }
    std::env::remove_var("DISKSAGE_FAKE_GH_OUTPUT");

    assert_eq!(
        result.expect_err("capped GitHub search evidence must fail closed"),
        "github-closed-pr-list-incomplete"
    );
}
