//! Black-box help and invalid-argument contracts for incomplete-download planning CLIs.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

const BINARIES: [(&str, &str); 2] = [
    (
        "disksage-incomplete-download-materialization",
        "usage: disksage-incomplete-download-materialization",
    ),
    (
        "disksage-incomplete-download-recovery",
        "usage: disksage-incomplete-download-recovery",
    ),
];

fn build_feature_gated_binaries() -> (tempfile::TempDir, Vec<PathBuf>) {
    let target_dir = tempfile::tempdir().expect("isolated Cargo target directory must be created");
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let mut command = Command::new(cargo);
    command
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["build", "--locked", "--features", "cloud-cli"]);
    for (binary, _) in BINARIES {
        command.args(["--bin", binary]);
    }
    let status = command
        .arg("--target-dir")
        .arg(target_dir.path())
        .status()
        .expect("feature-gated incomplete-download CLIs must be buildable for process contracts");
    assert!(
        status.success(),
        "feature-gated incomplete-download CLI build must succeed before process assertions"
    );

    let binaries = BINARIES
        .iter()
        .map(|(binary, _)| {
            let path = target_dir
                .path()
                .join("debug")
                .join(format!("{binary}{}", std::env::consts::EXE_SUFFIX));
            assert!(
                path.is_file(),
                "{binary} must exist after the explicit cloud-cli build"
            );
            path
        })
        .collect();
    (target_dir, binaries)
}

fn command(binary: &Path) -> Command {
    let mut command = Command::new(binary);
    command.env_remove("HOME").env_remove("USERPROFILE");
    command
}

fn assert_help_success(binary: &Path, usage: &str, flag: &str) {
    let output = command(binary)
        .arg(flag)
        .output()
        .expect("incomplete-download CLI must launch for its help contract");

    assert!(
        output.status.success(),
        "{flag} must be a successful terminal action, got status {:?} and stderr {:?}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "successful help must not be projected through stderr"
    );
    let stdout = String::from_utf8(output.stdout).expect("help output must be valid UTF-8");
    assert!(
        stdout.contains(usage),
        "help output must contain the stable usage synopsis"
    );
}

fn assert_invalid_argument_is_bounded(binary: &Path) {
    let output = command(binary)
        .arg("--opaque-option=not-shown")
        .output()
        .expect("incomplete-download CLI must launch for invalid argument validation");

    assert!(
        !output.status.success(),
        "an unknown argument must remain a non-zero failure"
    );
    assert!(
        output.stdout.is_empty(),
        "invalid invocation must not emit successful output on stdout"
    );
    let stderr = String::from_utf8(output.stderr).expect("CLI diagnostics must be valid UTF-8");
    assert!(!stderr.is_empty(), "invalid invocation must remain visible");
    assert!(
        !stderr.contains("not-shown"),
        "invalid diagnostics must not echo arbitrary argument payloads"
    );
}

fn assert_help_does_not_hide_invalid_argument(binary: &Path) {
    let output = command(binary)
        .args(["--help", "--opaque-option=not-shown"])
        .output()
        .expect("incomplete-download CLI must launch for mixed help validation");

    assert!(
        !output.status.success(),
        "help must not turn an otherwise invalid invocation into success"
    );
    assert!(
        output.stdout.is_empty(),
        "mixed invalid invocation must not emit successful help on stdout"
    );
    let stderr = String::from_utf8(output.stderr).expect("CLI diagnostics must be valid UTF-8");
    assert!(!stderr.is_empty(), "mixed invalid invocation must remain visible");
    assert!(
        !stderr.contains("not-shown"),
        "mixed invalid diagnostics must not echo arbitrary argument payloads"
    );
}

#[test]
fn incomplete_download_planning_help_is_successful_and_invalid_arguments_are_bounded() {
    let (_target_dir, binaries) = build_feature_gated_binaries();
    for ((_, usage), binary) in BINARIES.iter().zip(&binaries) {
        assert_help_success(binary, usage, "--help");
        assert_help_success(binary, usage, "-h");
        assert_invalid_argument_is_bounded(binary);
        assert_help_does_not_hide_invalid_argument(binary);
    }
}
