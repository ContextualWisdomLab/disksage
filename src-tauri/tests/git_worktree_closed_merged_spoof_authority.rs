//! Regression for the merged/closed evidence trust boundary.
//!
//! Caller-provided OIDs or branch names are hints, not proof that a worktree was actually
//! merged/closed. Until an authenticated/immutable evidence owner is bound to the audit, spoofed
//! values must not upgrade a commit that is outside the retained reference set into removal
//! authority.

use disksage_lib::git_worktree::{audit_git_worktrees, GitWorktreeAuditOptions};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn git(cwd: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_AUTHOR_NAME", "DiskSage Test")
        .env("GIT_AUTHOR_EMAIL", "disksage@example.invalid")
        .env("GIT_COMMITTER_NAME", "DiskSage Test")
        .env("GIT_COMMITTER_EMAIL", "disksage@example.invalid")
        .output()
        .expect("git should spawn");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn rev_parse(cwd: &Path) -> String {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(cwd)
        .output()
        .expect("rev-parse should spawn");
    assert!(output.status.success());
    String::from_utf8(output.stdout)
        .expect("oid utf8")
        .trim()
        .to_ascii_lowercase()
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

#[test]
fn raw_caller_closed_merged_values_never_become_removal_authority() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repository = temp.path().join("repository");
    let secondary = temp.path().join("secondary");
    fs::create_dir(&repository).expect("repository");

    git(&repository, &["init", "-b", "main"]);
    fs::write(repository.join("evidence.txt"), b"retained\n").expect("write retained");
    git(&repository, &["add", "evidence.txt"]);
    git(&repository, &["commit", "-m", "retained"]);

    git(&repository, &["checkout", "-b", "not-merged"]);
    fs::write(repository.join("evidence.txt"), b"not merged\n").expect("write branch");
    git(&repository, &["commit", "-am", "not merged"]);
    let unmerged_head = rev_parse(&repository);
    git(&repository, &["checkout", "main"]);
    git(
        &repository,
        &["worktree", "add", secondary.to_str().expect("secondary utf8"), "not-merged"],
    );

    let mut options = GitWorktreeAuditOptions::default();
    options.closed_merged_head_oids.push(unmerged_head);
    options.closed_merged_branches.push("refs/heads/not-merged".into());

    let report = audit_git_worktrees(&repository, &["main".into()], options, now_ms())
        .expect("audit should complete");
    let canonical_secondary = fs::canonicalize(&secondary).expect("canonical secondary");
    let entry = report
        .entries
        .iter()
        .find(|entry| Path::new(&entry.path) == canonical_secondary.as_path())
        .expect("secondary entry");

    assert_eq!(entry.contained_in_reference, Some(false));
    assert!(
        entry.merged_closed_evidence.is_none(),
        "raw caller assertion became authority: {:?}",
        entry.merged_closed_evidence
    );
    assert!(
        entry
            .blockers
            .iter()
            .any(|blocker| blocker == "reference-does-not-contain-head"),
        "blockers={:?}",
        entry.blockers
    );
}
