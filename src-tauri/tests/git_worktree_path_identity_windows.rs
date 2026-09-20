#![cfg(windows)]

use disksage_lib::git_worktree::{
    audit_git_worktrees, GitWorktreeAuditOptions, GitWorktreeDisposition,
};
use std::fs;
use std::path::Path;
use std::process::Command;

fn git(cwd: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_AUTHOR_NAME", "DiskSage Test")
        .env("GIT_AUTHOR_EMAIL", "disksage@example.invalid")
        .env("GIT_COMMITTER_NAME", "DiskSage Test")
        .env("GIT_COMMITTER_EMAIL", "disksage@example.invalid")
        .output()
        .expect("git must start");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn copy_regular_directory(source: &Path, destination: &Path) {
    fs::create_dir(destination).expect("replacement directory must exist before source removal");
    for entry in fs::read_dir(source).expect("source worktree must remain readable") {
        let entry = entry.expect("worktree entry must be readable");
        let file_type = entry.file_type().expect("worktree entry type must be readable");
        let target = destination.join(entry.file_name());
        if file_type.is_dir() {
            copy_regular_directory(&entry.path(), &target);
        } else {
            assert!(file_type.is_file(), "fixture refuses symlink or special entries");
            fs::copy(entry.path(), target).expect("fixture copy must preserve worktree bytes");
        }
    }
}

fn secondary_fingerprint(
    repository: &Path,
    secondary: &Path,
    generated_at_ms: u64,
) -> (String, String) {
    let report = audit_git_worktrees(
        repository,
        &["main".into()],
        GitWorktreeAuditOptions::default(),
        generated_at_ms,
    )
    .expect("real linked worktree must be auditable");
    let canonical_secondary = fs::canonicalize(secondary).expect("secondary must exist");
    let entry = report
        .entries
        .iter()
        .find(|entry| Path::new(&entry.path) == canonical_secondary)
        .expect("linked worktree must remain registered at the same path");
    assert_eq!(
        entry.disposition,
        GitWorktreeDisposition::RemovalCandidate,
        "fixture must remain otherwise removal-eligible: {entry:#?}"
    );
    (entry.path_fingerprint.clone(), report.removal_plan_fingerprint)
}

#[test]
fn same_registered_path_replacement_changes_path_and_plan_identity() {
    let temp = tempfile::tempdir().expect("temporary filesystem fixture");
    let repository = temp.path().join("repository");
    let secondary = temp.path().join("secondary");
    let replacement = temp.path().join("replacement");
    fs::create_dir(&repository).expect("repository root");

    git(&repository, &["init", "-b", "main"]);
    fs::write(repository.join("evidence.txt"), b"first\n").expect("first evidence");
    git(&repository, &["add", "evidence.txt"]);
    git(&repository, &["commit", "-m", "first"]);
    fs::write(repository.join("evidence.txt"), b"second\n").expect("second evidence");
    git(&repository, &["commit", "-am", "second"]);
    git(&repository, &["branch", "merged", "HEAD~1"]);
    git(
        &repository,
        &[
            "worktree",
            "add",
            secondary.to_str().expect("UTF-8 temp path"),
            "merged",
        ],
    );

    let (before_path_fingerprint, before_plan_fingerprint) =
        secondary_fingerprint(&repository, &secondary, 100);

    // The replacement exists concurrently before the approved directory is removed, so Windows
    // cannot legitimately describe both objects with one live filesystem identity.
    copy_regular_directory(&secondary, &replacement);
    fs::remove_dir_all(&secondary).expect("fixture removes the approved object");
    fs::rename(&replacement, &secondary).expect("replacement occupies the registered pathname");

    let (after_path_fingerprint, after_plan_fingerprint) =
        secondary_fingerprint(&repository, &secondary, 101);

    assert_ne!(
        before_path_fingerprint, after_path_fingerprint,
        "Windows path fingerprint must bind volume/file-index identity, not only canonical text"
    );
    assert_ne!(
        before_plan_fingerprint, after_plan_fingerprint,
        "deletion approval must become stale when the filesystem object at the registered path changes"
    );
}
