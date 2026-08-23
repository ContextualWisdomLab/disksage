#![cfg(all(feature = "cloud-cli", unix))]

//! Real-process coverage for malformed `git worktree list --porcelain -z` evidence.
//!
//! Git porcelain is an external authority boundary. These regressions launch the shipped audit CLI
//! with a deterministic fake `git` executable and prove that a worktree record missing either its
//! path or HEAD cannot become partial removal evidence. The CLI must fail before filesystem sizing,
//! active-use inspection, or any mutation-capable follow-up.

use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

const OID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_disksage-git-worktree-audit"))
}

fn install_fake_git(directory: &Path, porcelain_command: &str) -> PathBuf {
    let path = directory.join("git");
    let script = format!(
        "#!/bin/sh\n\
         if [ \"$1\" = rev-parse ] && [ \"$2\" = --path-format=absolute ]; then\n\
           pwd\n\
         elif [ \"$1\" = rev-list ]; then\n\
           printf '%s\\n' '{OID}'\n\
         elif [ \"$1\" = worktree ] && [ \"$2\" = list ]; then\n\
           {porcelain_command}\n\
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

fn run(porcelain_command: &str) -> std::process::Output {
    let temp = tempfile::tempdir().expect("temporary fixture directory should be created");
    let repository = temp.path().join("repository");
    let bin_dir = temp.path().join("bin");
    fs::create_dir_all(&repository).expect("repository directory should be created");
    fs::create_dir_all(&bin_dir).expect("binary directory should be created");
    install_fake_git(&bin_dir, porcelain_command);

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

#[test]
fn porcelain_record_without_path_fails_closed() {
    let output = run(&format!("printf 'HEAD {OID}\\0\\0'"));

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).expect("stderr should remain UTF-8"),
        "DiskSage Git worktree audit: git-worktree-porcelain-path-missing\n"
    );
}

#[test]
fn porcelain_record_without_head_fails_closed() {
    let output = run("printf 'worktree /tmp/disksage-untrusted-worktree\\0\\0'");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).expect("stderr should remain UTF-8"),
        "DiskSage Git worktree audit: git-worktree-porcelain-head-missing\n"
    );
}
