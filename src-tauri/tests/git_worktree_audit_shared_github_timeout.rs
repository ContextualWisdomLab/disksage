#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

fn git(repository: &Path, arguments: &[&str]) {
    let output = Command::new("git")
        .current_dir(repository)
        .args(arguments)
        .output()
        .expect("Git should be available");
    assert!(output.status.success(), "git {arguments:?} failed: {}", String::from_utf8_lossy(&output.stderr));
}

fn initialized_repository() -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().expect("temporary directory should be created");
    let repository = temp.path().join("repository");
    fs::create_dir(&repository).expect("repository directory should be created");
    git(&repository, &["init", "-q", "-b", "main"]);
    git(&repository, &["config", "user.email", "coverage@example.invalid"]);
    git(&repository, &["config", "user.name", "DiskSage Test"]);
    fs::write(repository.join("tracked.txt"), b"tracked\n").expect("tracked fixture should be written");
    git(&repository, &["add", "tracked.txt"]);
    git(&repository, &["commit", "-q", "-m", "fixture"]);
    (temp, repository)
}

#[test]
fn one_timeout_bounds_the_complete_github_evidence_phase() {
    let (temp, repository) = initialized_repository();
    let bin_dir = temp.path().join("bin");
    fs::create_dir(&bin_dir).expect("fake bin directory should be created");
    let gh_path = bin_dir.join("gh");
    fs::write(
        &gh_path,
        r#"#!/bin/sh
set -eu
case "$*" in
  "pr list --state closed --search is:unmerged"*) sleep 1; printf '[]\n' ;;
  "pr list "*) printf '[]\n' ;;
  "repo view "*) sleep 1; printf 'ContextualWisdomLab/disksage\n' ;;
  "api --paginate --slurp repos/ContextualWisdomLab/disksage/commits/"*) printf '[[]]\n' ;;
  *) printf 'unexpected fake gh invocation\n' >&2; exit 9 ;;
esac
"#,
    )
    .expect("fake gh should be written");
    let mut permissions = fs::metadata(&gh_path).expect("fake gh metadata").permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&gh_path, permissions).expect("fake gh should be executable");

    let mut paths = vec![bin_dir];
    if let Some(existing) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&existing));
    }
    let path = std::env::join_paths(paths).expect("PATH should be joinable");
    let output = Command::new(env!("CARGO_BIN_EXE_disksage-git-worktree-audit"))
        .env("PATH", path)
        .arg("--repository-root")
        .arg(&repository)
        .args([
            "--reference-ref",
            "HEAD",
            "--include-closed-pull-requests",
            "--command-timeout-ms",
            "1500",
        ])
        .output()
        .expect("Git worktree audit binary should start");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).expect("stderr should remain UTF-8"),
        "DiskSage Git worktree audit: github-repository-identity-timeout\n"
    );
}
