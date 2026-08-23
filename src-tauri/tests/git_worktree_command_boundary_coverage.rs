#![cfg(all(feature = "cloud-cli", unix))]

//! Real-process coverage for the Git-worktree audit command's bounded Git probe.
//!
//! The fixture installs a deterministic `git` executable that never returns. The shipped
//! `disksage-git-worktree-audit` binary must terminate that probe at its configured deadline and
//! return the stable fail-closed diagnostic instead of hanging or entering later Git operations.

use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_disksage-git-worktree-audit"))
}

fn install_hanging_git(directory: &Path) -> PathBuf {
    let path = directory.join("git");
    fs::write(&path, b"#!/bin/sh\nwhile :; do :; done\n")
        .expect("fake git should be written");
    let mut permissions = fs::metadata(&path)
        .expect("fake git metadata should be readable")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&path, permissions).expect("fake git should be executable");
    path
}

#[test]
fn common_dir_probe_timeout_fails_closed_at_the_real_cli_boundary() {
    let temp = tempfile::tempdir().expect("temporary directory should be created");
    let repository = temp.path().join("repository");
    let bin_dir = temp.path().join("bin");
    fs::create_dir_all(&repository).expect("repository directory should be created");
    fs::create_dir_all(&bin_dir).expect("binary directory should be created");
    install_hanging_git(&bin_dir);

    let mut search_paths = vec![bin_dir];
    if let Some(existing) = env::var_os("PATH") {
        search_paths.extend(env::split_paths(&existing));
    }
    let path = env::join_paths(search_paths).expect("PATH should be constructible");

    let output = command()
        .arg("--repository-root")
        .arg(&repository)
        .arg("--reference-ref")
        .arg("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        .arg("--command-timeout-ms")
        .arg("20")
        .env("PATH", path)
        .output()
        .expect("Git worktree audit binary should start");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).expect("stderr should remain UTF-8"),
        "DiskSage Git worktree audit: git-common-dir-resolve-timeout\n"
    );
}
