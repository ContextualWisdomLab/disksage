#![cfg(all(feature = "cloud-cli", unix))]

//! Real-process coverage for retention-reachable-commit evidence admission.
//!
//! The worktree audit must never proceed to removal classification when the bounded `git rev-list`
//! snapshot is empty, malformed, undecodable, or unsuccessful. These fixtures launch the shipped
//! CLI with a deterministic fake `git` executable so the external command boundary, output decoder,
//! OID validator, and fail-closed diagnostics are exercised together.

use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

const OID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_disksage-git-worktree-audit"))
}

fn install_fake_git(directory: &Path, rev_list_command: &str) -> PathBuf {
    let path = directory.join("git");
    let script = format!(
        "#!/bin/sh\n\
         if [ \"$1\" = rev-parse ] && [ \"$2\" = --path-format=absolute ]; then\n\
           pwd\n\
         elif [ \"$1\" = rev-list ]; then\n\
           {rev_list_command}\n\
         else\n\
           exit 97\n\
         fi\n"
    );
    fs::write(&path, script).expect("fake git should be written");
    let mut permissions = fs::metadata(&path)
        .expect("fake git metadata should be readable")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&path, permissions).expect("fake git should be executable");
    path
}

fn run(rev_list_command: &str) -> std::process::Output {
    let temp = tempfile::tempdir().expect("temporary fixture directory should be created");
    let repository = temp.path().join("repository");
    let bin_dir = temp.path().join("bin");
    fs::create_dir_all(&repository).expect("repository directory should be created");
    fs::create_dir_all(&bin_dir).expect("binary directory should be created");
    install_fake_git(&bin_dir, rev_list_command);

    let mut search_paths = vec![bin_dir];
    if let Some(existing) = env::var_os("PATH") {
        search_paths.extend(env::split_paths(&existing));
    }
    let path = env::join_paths(search_paths).expect("PATH should be constructible");

    command()
        .arg("--repository-root")
        .arg(&repository)
        .args(["--reference-ref", OID, "--command-timeout-ms", "5000"])
        .env("PATH", path)
        .output()
        .expect("Git worktree audit binary should start")
}

fn assert_failure(rev_list_command: &str, expected_reason: &str) {
    let output = run(rev_list_command);
    assert_eq!(output.status.code(), Some(2), "reason: {expected_reason}");
    assert!(output.stdout.is_empty(), "reason: {expected_reason}");
    assert_eq!(
        String::from_utf8(output.stderr).expect("stderr should remain UTF-8"),
        format!("DiskSage Git worktree audit: {expected_reason}\n")
    );
}

#[test]
fn empty_or_invalid_reachable_commit_sets_fail_closed() {
    assert_failure(
        "printf ''",
        "git-retention-reachable-commit-set-empty",
    );
    assert_failure(
        "printf 'not-an-object-id\\n'",
        "git-retention-reachable-commit-invalid",
    );
}

#[test]
fn rev_list_process_and_encoding_failures_remain_distinct() {
    assert_failure("exit 9", "git-retention-reachable-commits-failed");
    assert_failure(
        "printf '\\377\\n'",
        "git-retention-reachable-commits-not-utf8",
    );
}

#[test]
fn oversized_reachable_commit_output_is_never_partial_evidence() {
    assert_failure(
        "yes aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa | head -c 5000000",
        "git-retention-reachable-commits-output-truncated",
    );
}
