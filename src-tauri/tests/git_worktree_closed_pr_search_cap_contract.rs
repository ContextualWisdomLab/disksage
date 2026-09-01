#![cfg(unix)]

use disksage_lib::git_worktree::github_closed_pull_request_heads;
use serde_json::json;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

#[test]
fn paginated_rest_result_above_supported_bound_fails_closed() {
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
    let records: Vec<_> = (0..10_001u64)
        .map(|index| {
            json!({
                "number": index + 1,
                "state": "closed",
                "created_at": "2026-01-01T00:00:00Z",
                "merged_at": null,
                "head": {"ref": format!("closed-{index}"), "sha": format!("{index:040x}"), "repo": {"full_name": "owner/repo"}},
                "base": {"ref": "main", "sha": format!("{:040x}", 10_002), "repo": {"full_name": "owner/repo"}}
            })
        })
        .collect();
    fs::write(
        &output_path,
        serde_json::to_vec(&vec![records]).expect("serialize fake GitHub response"),
    )
    .expect("write fake GitHub response");

    let gh_path = bin_dir.join("gh");
    fs::write(
        &gh_path,
        "#!/bin/sh\nset -eu\ncase \" $* \" in\n  *' api --paginate --slurp repos/{owner}/{repo}/pulls?state=all&per_page=100 '*) cat \"$DISKSAGE_FAKE_GH_OUTPUT\" ;;\n  *) exit 64 ;;\nesac\n",
    )
    .expect("write fake gh executable");
    let mut permissions = fs::metadata(&gh_path)
        .expect("fake gh metadata")
        .permissions();
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
        result.expect_err("oversized REST evidence must fail closed"),
        "github-closed-pr-count-exceeds-limit"
    );
}
