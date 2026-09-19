//! Black-box process contract for the log-archive CLI help / argument surface.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

const EXPECTED_USAGE: &str = "usage: disksage-log-archive --root ABSOLUTE_PATH --older-than-days N (0=no age gate) [--execute] [--journal-path ABSOLUTE_PATH] [--min-stable-secs N] [--zstd-bin ABSOLUTE_PATH]";

fn binary_path() -> &'static Path {
    static BINARY_PATH: OnceLock<PathBuf> = OnceLock::new();
    BINARY_PATH
        .get_or_init(|| PathBuf::from(env!("CARGO_BIN_EXE_disksage-log-archive")))
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
            .expect("log archive binary should start");
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
fn missing_required_root_is_bounded_failure() {
    let output = command()
        .args(["--older-than-days", "30"])
        .output()
        .expect("log archive binary should start");
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--root is required"));
    assert!(stderr.contains(EXPECTED_USAGE));
}

#[test]
fn unknown_argument_is_bounded_failure() {
    let output = command()
        .args(["--not-a-real-flag"])
        .output()
        .expect("log archive binary should start");
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown argument"));
    assert!(stderr.contains(EXPECTED_USAGE));
}
