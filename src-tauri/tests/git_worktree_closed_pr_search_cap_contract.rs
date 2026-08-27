#![cfg(unix)]

use disksage_lib::git_worktree::github_closed_pull_request_heads;
use serde_json::json;
use std::fs;
use std::os::unix::fs::PermissionsExt;

#[test]
fn closed_pr_listing_avoids_search_cap_and_accepts_more_than_1000_complete_records() {
    let temp = tempfile::tempdir().expect("temporary repository root");
    let bin_dir = temp.path().join("bin");
    fs::create_dir(&bin_dir).expect("create fake bin directory");

    let output_path = temp.path().join("closed-prs.json");
    let records: Vec<_> = (0..1_001u64)
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
        "#!/bin/sh\nset -eu\nfor arg in \"$@\"; do\n  if [ \"$arg\" = \"--search\" ]; then\n    printf '%s\\n' 'warning: search results capped at 1000' >&2\n  fi\ndone\ncat \"$DISKSAGE_FAKE_GH_OUTPUT\"\n",
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

    let heads = result.expect("non-search closed-PR listing must remain complete past 1,000 records");
    assert_eq!(heads.len(), 1_001);
    assert!(heads.contains(&(
        "refs/heads/closed-0".to_string(),
        "0000000000000000000000000000000000000000".to_string()
    )));
    assert!(heads.contains(&(
        "refs/heads/closed-1000".to_string(),
        "00000000000000000000000000000000000003e8".to_string()
    )));
}
