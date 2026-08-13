//! Black-box process contract for the provider client-runtime audit CLI.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

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
            "disksage-provider-client-runtime",
            "--target-dir",
        ])
        .arg(target_dir.path())
        .status()
        .expect("provider client-runtime CLI must be buildable for its process contract");
    assert!(
        status.success(),
        "feature-gated provider client-runtime CLI build must succeed before process assertions"
    );

    let binary = target_dir
        .path()
        .join("debug")
        .join(format!(
            "disksage-provider-client-runtime{}",
            std::env::consts::EXE_SUFFIX
        ));
    assert!(
        binary.is_file(),
        "provider client-runtime binary must exist after the explicit cloud-cli build"
    );
    (target_dir, binary)
}

fn assert_help_success(binary: &Path, flag: &str) {
    let output = Command::new(binary)
        .env_remove("HOME")
        .arg(flag)
        .output()
        .expect("provider client-runtime CLI must launch for its help contract");

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
        stdout.contains("usage: disksage-provider-client-runtime"),
        "help output must contain the stable usage synopsis"
    );
}

fn assert_invalid_argument_is_bounded(binary: &Path) {
    let output = Command::new(binary)
        .env_remove("HOME")
        .arg("--opaque-option=not-shown")
        .output()
        .expect("provider client-runtime CLI must launch for invalid argument validation");

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
        .args(["--help", "--opaque-option=not-shown"])
        .output()
        .expect("provider client-runtime CLI must launch for mixed help validation");

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
fn provider_client_runtime_help_is_successful_and_invalid_arguments_are_bounded() {
    let (_target_dir, binary) = build_feature_gated_binary();
    assert_help_success(&binary, "--help");
    assert_help_success(&binary, "-h");
    assert_invalid_argument_is_bounded(&binary);
    assert_help_does_not_hide_invalid_argument(&binary);
}
