#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

fn run_git(root: &Path, args: &[&str]) {
    assert!(
        Command::new("git")
            .args(args)
            .current_dir(root)
            .status()
            .expect("git command must start")
            .success(),
        "git command failed: {args:?}"
    );
}

#[test]
fn stale_clone_planner_uses_authoritative_github_default_branch() {
    let root = std::env::temp_dir().join(format!(
        "disksage-stale-clone-default-branch-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    let repository = root.join("repo");
    let fake_bin = root.join("bin");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&repository).unwrap();
    fs::create_dir_all(&fake_bin).unwrap();

    run_git(&repository, &["init", "-q"]);
    run_git(&repository, &["config", "user.email", "test@example.invalid"]);
    run_git(&repository, &["config", "user.name", "DiskSage Test"]);
    run_git(&repository, &["checkout", "-qb", "feature"]);
    fs::write(repository.join("tracked.txt"), "tracked\n").unwrap();
    run_git(&repository, &["add", "tracked.txt"]);
    run_git(&repository, &["commit", "-qm", "fixture"]);
    run_git(
        &repository,
        &["remote", "add", "origin", "https://github.com/example/repo.git"],
    );
    run_git(
        &repository,
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/main",
        ],
    );

    let script = r#"#!/bin/sh
for arg in "$@"; do
  if [ "$arg" = ".default_branch" ]; then
    printf '%s\n' feature
    exit 0
  fi
done
printf '%s\n' '[]'
"#;
    let gh = fake_bin.join("gh");
    fs::write(&gh, script).unwrap();
    let mut permissions = fs::metadata(&gh).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&gh, permissions).unwrap();

    let original_path = std::env::var_os("PATH").unwrap_or_default();
    let path = std::env::join_paths(
        std::iter::once(fake_bin.as_path()).chain(std::env::split_paths(&original_path).as_ref()),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_disksage-stale-git-clone"))
        .args([
            "--repository-root",
            repository.to_str().unwrap(),
            "--open-age-days",
            "90",
        ])
        .env("PATH", path)
        .output()
        .expect("planner CLI must start");

    assert!(!output.status.success(), "stale local origin/HEAD was trusted");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("stale-git-clone-default-branch"),
        "planner did not reject the authoritative GitHub default branch: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    fs::remove_dir_all(root).unwrap();
}
