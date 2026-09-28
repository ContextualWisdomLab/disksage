//! Black-box process contract for the Homebrew audit CLI help / argument surface.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

const EXPECTED_USAGE: &str = "usage: disksage-homebrew-audit [--repo-root ABSOLUTE_PATH ...] [--stale-after-days N] [--name NAME ...] [--output NEW_ABSOLUTE_JSON_PATH] [--command-timeout-ms N]";

fn binary_path() -> &'static Path {
    static BINARY_PATH: OnceLock<PathBuf> = OnceLock::new();
    BINARY_PATH
        .get_or_init(|| PathBuf::from(env!("CARGO_BIN_EXE_disksage-homebrew-audit")))
        .as_path()
}

fn command() -> Command {
    Command::new(binary_path())
}

#[test]
fn sole_help_flags_are_terminal_success_without_domain_environment() {
    for flag in ["--help", "-h"] {
        let output = command()
            .env_remove("HOME")
            .env_remove("USERPROFILE")
            .env("PATH", "")
            .arg(flag)
            .output()
            .expect("homebrew audit binary should start");
        assert_eq!(output.status.code(), Some(0), "flag={flag}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim_end(),
            EXPECTED_USAGE,
            "flag={flag}"
        );
        assert!(output.stderr.is_empty(), "flag={flag}");
    }
}

#[test]
fn unknown_argument_is_bounded_failure() {
    let output = command()
        .args(["--not-a-real-flag"])
        .output()
        .expect("homebrew audit binary should start");
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown argument"));
    assert!(stderr.contains(EXPECTED_USAGE));
}
