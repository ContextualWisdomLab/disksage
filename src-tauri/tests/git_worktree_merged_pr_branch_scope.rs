#![cfg(unix)]

use disksage_lib::git_worktree::github_closed_pull_request_heads;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

#[test]
fn merged_pull_request_query_is_scoped_to_registered_branch() {
    let temp = tempfile::tempdir().expect("temporary repository root");
    Command::new("git")
        .args(["init", "-q", "-b", "merged-work"])
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
    let head = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(temp.path())
        .output()
        .expect("resolve fixture head");
    let head = String::from_utf8(head.stdout).unwrap().trim().to_string();

    let bin_dir = temp.path().join("bin");
    fs::create_dir(&bin_dir).expect("create fake bin directory");
    let gh_path = bin_dir.join("gh");
    fs::write(
        &gh_path,
        format!(
            "#!/bin/sh\nset -eu\ncase \" $* \" in\n  *' --state closed '*) printf '[]' ;;\n  *' --state merged --head merged-work '*) printf '%s' '[{{\"headRefName\":\"merged-work\",\"headRefOid\":\"{head}\",\"isCrossRepository\":false,\"state\":\"MERGED\"}}]' ;;\n  *) exit 64 ;;\nesac\n"
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
