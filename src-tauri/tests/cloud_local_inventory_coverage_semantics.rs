//! Coverage instrumentation must not replace the shipped cloud-local-inventory CLI with a no-op.
//!
//! This contract compiles the actual feature-gated binary with the `coverage` cfg that generic
//! `cargo llvm-cov` supplies and then launches it as a process. Instrumentation may measure the
//! shipped behavior, but it must not substitute different runtime semantics.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

const EXPECTED_USAGE: &str = "usage: disksage-cloud-local-inventory (--cloud-root ABSOLUTE_PATH [--relative-subpath SAFE_RELATIVE_PATH] | --all-roots) [--min-allocated-mib N] [--max-entries N] [--max-results N] [--max-depth N] [--max-duration-ms N] [--max-issues N]";

fn build_coverage_cfg_binary() -> (tempfile::TempDir, PathBuf) {
    let target_dir = tempfile::tempdir().expect("isolated coverage target directory must be created");
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let output = Command::new(cargo)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args([
            "rustc",
            "--locked",
            "--features",
            "cloud-cli",
            "--bin",
            "disksage-cloud-local-inventory",
            "--target-dir",
        ])
        .arg(target_dir.path())
        .args(["--", "--cfg", "coverage"])
        .output()
        .expect("cloud-local-inventory coverage build must start");
    assert!(
        output.status.success(),
        "cloud-local-inventory coverage build must succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let binary = target_dir
        .path()
        .join("debug")
        .join(format!(
            "disksage-cloud-local-inventory{}",
            std::env::consts::EXE_SUFFIX
        ));
    assert!(binary.is_file(), "coverage-configured shipped binary must exist");
    (target_dir, binary)
}

fn run(binary: &Path, args: &[&str]) -> std::process::Output {
    Command::new(binary)
        .env_remove("HOME")
        .env_remove("USERPROFILE")
        .args(args)
        .output()
        .expect("coverage-configured cloud-local-inventory binary must launch")
}

#[test]
fn coverage_cfg_preserves_terminal_help_and_pre_discovery_limit_rejection() {
    let (_target_dir, binary) = build_coverage_cfg_binary();

    for flag in ["--help", "-h"] {
        let output = run(&binary, &[flag]);
        assert_eq!(output.status.code(), Some(0), "help flag: {flag}");
        assert!(output.stderr.is_empty(), "help flag: {flag}");
        assert_eq!(
            String::from_utf8(output.stdout).expect("help stdout must remain UTF-8"),
            format!("{EXPECTED_USAGE}\n"),
            "coverage instrumentation must not replace the shipped help path"
        );
    }

    let invalid = run(&binary, &["--all-roots", "--max-results", "0"]);
    assert_eq!(invalid.status.code(), Some(2));
    assert!(invalid.stdout.is_empty());
    assert_eq!(
        String::from_utf8(invalid.stderr).expect("invalid stderr must remain UTF-8"),
        "cloud-local-inventory-max-results-invalid\n"
    );
}
