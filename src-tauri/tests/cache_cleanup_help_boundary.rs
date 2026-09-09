//! Black-box terminal contract for the DiskSage cache-cleanup CLI.
//!
//! Help is pure operator documentation and must terminate before HOME/APPDATA/XDG resolution.
//! Mixed help plus another argument is not a help-only request and remains ordinary validation.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

const USAGE_PREFIX: &str = "Usage: disksage-cache-cleanup";

fn build_cache_cleanup() -> (tempfile::TempDir, PathBuf) {
    let target_dir = tempfile::tempdir().expect("isolated Cargo target directory must be created");
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let status = Command::new(cargo)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args([
            "build",
            "--locked",
            "--bin",
            "disksage-cache-cleanup",
            "--target-dir",
        ])
        .arg(target_dir.path())
        .status()
        .expect("cache-cleanup CLI must be buildable for its process contract");
    assert!(status.success(), "cache-cleanup CLI build must succeed before process assertions");

    let binary = target_dir
        .path()
        .join("debug")
        .join(format!("disksage-cache-cleanup{}", std::env::consts::EXE_SUFFIX));
    assert!(binary.is_file(), "cache-cleanup binary must exist after the explicit build");
    (target_dir, binary)
}

fn command(binary: &Path) -> Command {
    let mut command = Command::new(binary);
    command
        .env_remove("HOME")
        .env_remove("USERPROFILE")
        .env_remove("APPDATA")
        .env_remove("XDG_DATA_HOME");
    command
}

fn assert_help_success(binary: &Path, flag: &str) {
    let output = command(binary)
        .arg(flag)
        .output()
        .expect("cache-cleanup CLI must launch for its help contract");

    assert!(
        output.status.success(),
        "{flag} must succeed without home/app-data state, got status {:?} and stderr {:?}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty(), "successful help must not use stderr");
    let stdout = String::from_utf8(output.stdout).expect("help output must be UTF-8");
    assert!(stdout.starts_with(USAGE_PREFIX), "help must emit the stable cache-cleanup synopsis");
}

#[test]
fn cache_cleanup_help_terminates_before_environment_resolution() {
    let (_target_dir, binary) = build_cache_cleanup();

    assert_help_success(&binary, "--help");
    assert_help_success(&binary, "-h");

    let mixed = command(&binary)
        .args(["--help", "--unknown-option"])
        .output()
        .expect("mixed help invocation must launch");
    assert!(
        !mixed.status.success(),
        "help mixed with another option must not bypass ordinary argument validation"
    );
}
