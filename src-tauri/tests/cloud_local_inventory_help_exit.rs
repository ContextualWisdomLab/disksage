//! Black-box process contract for the cloud local-inventory CLI.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

const EXPECTED_USAGE: &str = "usage: disksage-cloud-local-inventory (--cloud-root ABSOLUTE_PATH [--relative-subpath SAFE_RELATIVE_PATH] | --all-roots) [--min-allocated-mib N] [--max-entries N] [--max-results N] [--max-depth N] [--max-duration-ms N] [--max-issues N]";

fn build_feature_gated_binary() -> (tempfile::TempDir, PathBuf) {
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
            "disksage-cloud-local-inventory",
            "--target-dir",
        ])
        .arg(target_dir.path())
        .status()
        .expect("cloud local-inventory CLI must be buildable for its process contract");
    assert!(
        status.success(),
        "feature-gated cloud local-inventory CLI build must succeed before process assertions"
    );

    let binary = target_dir
        .path()
        .join("debug")
        .join(format!(
            "disksage-cloud-local-inventory{}",
            std::env::consts::EXE_SUFFIX
        ));
    assert!(
        binary.is_file(),
        "cloud local-inventory binary must exist after the explicit cloud-cli build"
    );
    (target_dir, binary)
}

fn assert_help_success(binary: &Path, flag: &str) {
    let output = Command::new(binary)
        .env_remove("HOME")
        .env_remove("USERPROFILE")
        .arg(flag)
        .output()
        .expect("cloud local-inventory CLI must launch for its help contract");

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
    assert_eq!(
        stdout,
        format!("{EXPECTED_USAGE}\n"),
        "help output must equal the stable usage synopsis"
    );
}

fn assert_invalid_argument_is_bounded(binary: &Path) {
    let output = Command::new(binary)
        .env_remove("HOME")
        .env_remove("USERPROFILE")
        .arg("--opaque-option=not-shown")
        .output()
        .expect("cloud local-inventory CLI must launch for invalid argument validation");

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
    let output = Command::new(binary)
        .env_remove("HOME")
        .env_remove("USERPROFILE")
        .args(["--help", "--opaque-option=not-shown"])
        .output()
        .expect("cloud local-inventory CLI must launch for mixed help validation");

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
fn cloud_local_inventory_help_is_successful_and_invalid_arguments_are_bounded() {
    let (_target_dir, binary) = build_feature_gated_binary();
    assert_help_success(&binary, "--help");
    assert_help_success(&binary, "-h");
    assert_invalid_argument_is_bounded(&binary);
    assert_help_does_not_hide_invalid_argument(&binary);
}
