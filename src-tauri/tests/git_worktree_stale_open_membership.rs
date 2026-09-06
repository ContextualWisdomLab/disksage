#![cfg(unix)]

use disksage_lib::git_worktree::{
    audit_git_worktrees_with_pull_request_membership, ClosedPullRequestHeads,
    GitWorktreeAuditOptions, GitWorktreeDisposition, PullRequestCommitMembership,
    StaleOpenPullRequestHeads,
};
use std::collections::{BTreeMap, BTreeSet};
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
fn stale_open_head_exempts_only_cutoff_authorized_pull_request_membership() {
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

    let stale_pull_request_number = 101u64;
    let second_stale_pull_request_number = 102u64;
    let other_open_pull_request_number = 202u64;
    let head_binding = ("refs/heads/stale-open".to_string(), base.clone());
    let stale_heads = StaleOpenPullRequestHeads::from([(
        head_binding.clone(),
        BTreeSet::from([stale_pull_request_number]),
    )]);
    let mut own_membership = PullRequestCommitMembership::default();
    own_membership.open.insert(
        base.clone(),
        BTreeSet::from([stale_pull_request_number]),
    );

    let report = audit_git_worktrees_with_pull_request_membership(
        &repository,
        &["refs/heads/main".into()],
        &ClosedPullRequestHeads::new(),
        &stale_heads,
        &own_membership,
        Some(1),
        GitWorktreeAuditOptions::default(),
        2,
    )
    .expect("audit stale-open worktree with only cutoff-authorized open membership");

    let entry = report
        .entries
        .iter()
        .find(|entry| entry.branch.as_deref() == Some("refs/heads/stale-open"))
        .expect("stale-open worktree entry");
    assert!(entry.stale_open_pull_request_head);
    assert!(
        !entry.open_pull_request_commit,
        "cutoff-authorized stale PR membership must not veto its own cleanup authority"
    );
    assert_eq!(entry.disposition, GitWorktreeDisposition::RemovalCandidate);

    let mut independent_open_membership = PullRequestCommitMembership::default();
    independent_open_membership.open.insert(
        base.clone(),
        BTreeSet::from([
            stale_pull_request_number,
            other_open_pull_request_number,
        ]),
    );
    let report = audit_git_worktrees_with_pull_request_membership(
        &repository,
        &["refs/heads/main".into()],
        &ClosedPullRequestHeads::new(),
        &stale_heads,
        &independent_open_membership,
        Some(1),
        GitWorktreeAuditOptions::default(),
        3,
    )
    .expect("audit stale-open worktree with independent open membership");

    let entry = report
        .entries
        .iter()
        .find(|entry| entry.branch.as_deref() == Some("refs/heads/stale-open"))
        .expect("stale-open worktree entry");
    assert!(entry.open_pull_request_commit);
    assert!(entry
        .blockers
        .iter()
        .any(|blocker| blocker == "open-pull-request-commit"));
    assert_eq!(entry.disposition, GitWorktreeDisposition::Preserve);

    let all_stale_heads = StaleOpenPullRequestHeads::from([(
        head_binding,
        BTreeSet::from([
            stale_pull_request_number,
            second_stale_pull_request_number,
        ]),
    )]);
    let mut all_stale_membership = PullRequestCommitMembership::default();
    all_stale_membership.open = BTreeMap::from([(
        base,
        BTreeSet::from([
            stale_pull_request_number,
            second_stale_pull_request_number,
        ]),
    )]);
    let report = audit_git_worktrees_with_pull_request_membership(
        &repository,
        &["refs/heads/main".into()],
        &ClosedPullRequestHeads::new(),
        &all_stale_heads,
        &all_stale_membership,
        Some(1),
        GitWorktreeAuditOptions::default(),
        4,
    )
    .expect("audit worktree shared only by cutoff-authorized stale pull requests");

    let entry = report
        .entries
        .iter()
        .find(|entry| entry.branch.as_deref() == Some("refs/heads/stale-open"))
        .expect("shared stale-open worktree entry");
    assert!(entry.stale_open_pull_request_head);
    assert!(
        !entry.open_pull_request_commit,
        "multiple cutoff-authorized stale PRs sharing one exact head must not veto one another"
    );
    assert_eq!(entry.disposition, GitWorktreeDisposition::RemovalCandidate);
}
