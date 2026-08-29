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
fn pull_request_membership_report_uses_v4_schema() {
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

    assert_eq!(report.schema_kind, "disksage.git-worktree-audit/v4");
    assert_eq!(report.version, 4);
}
