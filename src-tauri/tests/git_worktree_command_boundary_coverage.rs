#![cfg(all(feature = "cloud-cli", unix))]

//! Real-process coverage for the Git-worktree audit command's bounded Git probe.
//!
//! The fixtures install deterministic `git` executables that either never return or emit more
//! output than DiskSage permits. The shipped `disksage-git-worktree-audit` binary must terminate
//! or bound those probes and return stable fail-closed diagnostics instead of hanging, accepting
//! truncated evidence, or entering later Git operations.

use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

const OID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_disksage-git-worktree-audit"))
}

fn install_fake_git(directory: &Path, body: &str) -> PathBuf {
    let path = directory.join("git");
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("fake git should be written");
    let mut permissions = fs::metadata(&path)
        .expect("fake git metadata should be readable")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&path, permissions).expect("fake git should be executable");
    path
}

fn fixture(body: &str) -> (tempfile::TempDir, PathBuf, std::ffi::OsString) {
    let temp = tempfile::tempdir().expect("temporary directory should be created");
    let repository = temp.path().join("repository");
    let bin_dir = temp.path().join("bin");
    fs::create_dir_all(&repository).expect("repository directory should be created");
    fs::create_dir_all(&bin_dir).expect("binary directory should be created");
    install_fake_git(&bin_dir, body);

    let mut search_paths = vec![bin_dir];
    if let Some(existing) = env::var_os("PATH") {
        search_paths.extend(env::split_paths(&existing));
    }
    let path = env::join_paths(search_paths).expect("PATH should be constructible");
    (temp, repository, path)
}

fn run(repository: &Path, path: &std::ffi::OsStr, timeout_ms: &str) -> std::process::Output {
    command()
        .arg("--repository-root")
        .arg(repository)
        .args(["--reference-ref", OID, "--command-timeout-ms", timeout_ms])
        .env("PATH", path)
        .output()
        .expect("Git worktree audit binary should start")
}

#[test]
fn common_dir_probe_timeout_fails_closed_at_the_real_cli_boundary() {
    let (_temp, repository, path) = fixture("while :; do :; done");
    let output = run(&repository, &path, "20");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).expect("stderr should remain UTF-8"),
        "DiskSage Git worktree audit: git-common-dir-resolve-timeout\n"
    );
}

#[test]
fn oversized_common_dir_output_is_rejected_instead_of_becoming_partial_evidence() {
    let (_temp, repository, path) = fixture("yes x | head -c 5000000");
    let output = run(&repository, &path, "5000");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).expect("stderr should remain UTF-8"),
        "DiskSage Git worktree audit: git-common-dir-resolve-output-truncated\n"
    );
}
