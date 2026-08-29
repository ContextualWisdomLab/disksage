#![cfg(unix)]

use disksage_lib::git_worktree::{
    audit_git_worktrees_with_pull_request_membership, ClosedPullRequestHeads,
    GitWorktreeAuditOptions, GitWorktreeDisposition, PullRequestCommitMembership,
    StaleOpenPullRequestHeads,
};
use std::fs;
use std::process::Command;

fn git(repository: &std::path::Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(repository)
        .status()
        .expect("run git fixture command");
    assert!(status.success(), "git fixture command failed: {args:?}");
}

#[test]
fn exact_stale_open_head_can_use_explicit_cutoff_authority_despite_open_membership() {
    let temp = tempfile::tempdir().expect("temporary fixture root");
    let repository = temp.path().join("repository");
    let stale_worktree = temp.path().join("stale-open-worktree");
    fs::create_dir_all(&repository).expect("create repository root");

    git(&repository, &["init", "-q", "-b", "main"]);
    fs::write(repository.join("tracked.txt"), b"base\n").unwrap();
    git(&repository, &["add", "tracked.txt"]);
    git(
        &repository,
        &[
            "-c",
            "user.name=DiskSage Test",
            "-c",
            "user.email=disksage@example.invalid",
            "commit",
            "-q",
            "-m",
            "base",
        ],
    );
    let base = String::from_utf8(
        Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&repository)
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_owned();

    git(
        &repository,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "stale-open",
            stale_worktree.to_str().unwrap(),
            &base,
        ],
    );
    fs::write(repository.join("tracked.txt"), b"base\nretained tip\n").unwrap();
    git(&repository, &["add", "tracked.txt"]);
    git(
        &repository,
        &[
            "-c",
            "user.name=DiskSage Test",
            "-c",
            "user.email=disksage@example.invalid",
            "commit",
            "-q",
            "-m",
            "retained tip",
        ],
    );

    let stale_heads = StaleOpenPullRequestHeads::from([(
        "refs/heads/stale-open".to_string(),
        base.clone(),
    )]);
    let mut membership = PullRequestCommitMembership::default();
    membership.open.insert(base.clone());

    let report = audit_git_worktrees_with_pull_request_membership(
        &repository,
        &["refs/heads/main".into()],
        &ClosedPullRequestHeads::new(),
        &stale_heads,
        &membership,
        Some(1),
        GitWorktreeAuditOptions::default(),
        2,
    )
    .expect("audit stale-open worktree");

    let entry = report
        .entries
        .iter()
        .find(|entry| entry.branch.as_deref() == Some("refs/heads/stale-open"))
        .expect("stale-open worktree entry");
    assert!(entry.stale_open_pull_request_head);
    assert!(entry.open_pull_request_commit);
    assert_eq!(entry.disposition, GitWorktreeDisposition::RemovalCandidate);
    assert!(
        !entry.blockers.iter().any(|blocker| blocker == "open-pull-request-commit"),
        "the exact stale-open head must use its explicit cutoff authority rather than be vetoed by its own PR membership: {:?}",
        entry.blockers
    );
}
