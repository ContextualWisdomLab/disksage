#![cfg(unix)]

use disksage_lib::git_worktree::{
    audit_git_worktrees_with_pull_request_membership, GitWorktreeAuditOptions,
    PullRequestCommitMembership,
};
use std::collections::BTreeSet;
use std::fs;
use std::process::Command;

fn init_repository(path: &std::path::Path) {
    fs::create_dir_all(path).unwrap();
    assert!(Command::new("git")
        .args(["init", "-q", "-b", "main"])
        .current_dir(path)
        .status()
        .unwrap()
        .success());
    fs::write(path.join("tracked.txt"), b"fixture\n").unwrap();
    assert!(Command::new("git")
        .args(["add", "tracked.txt"])
        .current_dir(path)
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
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
        .unwrap()
        .success());
}

#[test]
fn pull_request_membership_report_matches_shared_v4_contract() {
    let contract: serde_json::Value = serde_json::from_str(include_str!(
        "../../contracts/git-worktree-audit-v4.json"
    ))
    .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let repository = temp.path().join("repository");
    init_repository(&repository);

    let report = audit_git_worktrees_with_pull_request_membership(
        &repository,
        &["refs/heads/main".into()],
        &BTreeSet::new(),
        &std::collections::BTreeMap::new(),
        &PullRequestCommitMembership::default(),
        None,
        GitWorktreeAuditOptions::default(),
        42,
    )
    .unwrap();

    assert_eq!(
        report.schema_kind,
        contract["schema_kind"].as_str().unwrap()
    );
    assert_eq!(report.version, contract["version"].as_u64().unwrap() as u32);

    let serialized_entry = serde_json::to_value(report.entries.first().expect("audit entry"))
        .expect("serialize audit entry");
    for field in contract["entry_membership_fields"]
        .as_array()
        .expect("membership field list")
    {
        let field = field.as_str().expect("membership field name");
        assert!(
            serialized_entry.get(field).is_some(),
            "runtime audit entry is missing shared contract field {field}"
        );
    }
}
