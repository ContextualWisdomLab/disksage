//! Black-box help and invalid-argument contract for incomplete-download execution.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

const BINARY: &str = "disksage-incomplete-download-materialize";
const USAGE: &str = "usage: disksage-incomplete-download-materialize";

fn build_binary() -> (tempfile::TempDir, PathBuf) {
    let target_dir = tempfile::tempdir().expect("isolated Cargo target directory must be created");
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let status = Command::new(cargo)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args([
            "build",
            "--locked",
            "--features",
            "cloud-cli",
            "--bin",
            BINARY,
            "--target-dir",
        ])
        .arg(target_dir.path())
        .status()
        .expect("feature-gated execution CLI must be buildable for process contracts");
    assert!(status.success(), "execution CLI build must succeed before process assertions");

    let binary = target_dir
        .path()
        .join("debug")
        .join(format!("{BINARY}{}", std::env::consts::EXE_SUFFIX));
    assert!(binary.is_file(), "execution CLI must exist after explicit cloud-cli build");
    (target_dir, binary)
}

fn command(binary: &Path) -> Command {
    let mut command = Command::new(binary);
    command.env_remove("HOME").env_remove("USERPROFILE");
    command
}

#[test]
fn materialization_execution_help_is_successful_and_invalid_arguments_are_bounded() {
    let (_target_dir, binary) = build_binary();

    for flag in ["--help", "-h"] {
        let output = command(&binary)
            .arg(flag)
            .output()
            .expect("execution CLI must launch for help validation");
        assert!(
            output.status.success(),
            "{flag} must be a successful terminal action, got status {:?} and stderr {:?}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty(), "successful help must keep stderr empty");
        let stdout = String::from_utf8(output.stdout).expect("help output must be valid UTF-8");
        assert!(stdout.contains(USAGE), "help must contain the stable usage synopsis");
    }

    for args in [
        vec!["--opaque-option=not-shown"],
        vec!["--help", "--opaque-option=not-shown"],
    ] {
        let output = command(&binary)
            .args(args)
            .output()
            .expect("execution CLI must launch for invalid argument validation");
        assert!(!output.status.success(), "invalid invocation must remain non-zero");
        assert!(output.stdout.is_empty(), "invalid invocation must keep stdout empty");
        let stderr = String::from_utf8(output.stderr).expect("diagnostics must be valid UTF-8");
        assert!(!stderr.is_empty(), "invalid invocation must remain visible");
        assert!(
            !stderr.contains("not-shown"),
            "diagnostics must not echo arbitrary argument payloads"
        );
    }
}
