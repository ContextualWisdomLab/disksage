//! Black-box process contracts for DiskSage audit-oriented operational CLIs.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

fn build_feature_gated_audit_binaries() -> (tempfile::TempDir, PathBuf, PathBuf) {
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
            "disksage-multipart-archive-audit",
            "--bin",
            "disksage-incomplete-download-audit",
            "--target-dir",
        ])
        .arg(target_dir.path())
        .status()
        .expect("feature-gated audit CLIs must be buildable for their process contracts");
    assert!(
        status.success(),
        "feature-gated audit CLI build must succeed before process assertions"
    );

    let executable_name = |name: &str| {
        target_dir
            .path()
            .join("debug")
            .join(format!("{name}{}", std::env::consts::EXE_SUFFIX))
    };
    let multipart_archive_audit = executable_name("disksage-multipart-archive-audit");
    let incomplete_download_audit = executable_name("disksage-incomplete-download-audit");
    assert!(
        multipart_archive_audit.is_file(),
        "multipart archive audit binary must exist after the explicit cloud-cli build"
    );
    assert!(
        incomplete_download_audit.is_file(),
        "incomplete download audit binary must exist after the explicit cloud-cli build"
    );

    (
        target_dir,
        multipart_archive_audit,
        incomplete_download_audit,
    )
}

fn assert_help_success(binary: &Path, flag: &str, usage_marker: &str) {
    let output = Command::new(binary)
        .env_remove("HOME")
        .arg(flag)
        .output()
        .expect("DiskSage audit CLI must launch for its help contract");

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
        stdout.contains(usage_marker),
        "help output must contain the stable usage synopsis"
    );
}

fn assert_invalid_argument_is_bounded(binary: &Path) {
    let output = Command::new(binary)
        .env_remove("HOME")
        .arg("--opaque-option=not-shown")
        .output()
        .expect("DiskSage audit CLI must launch for invalid argument validation");

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
        .expect("DiskSage audit CLI must launch for mixed help validation");

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
fn audit_cli_help_is_successful_and_invalid_arguments_are_bounded() {
    let (_target_dir, multipart_archive_audit, incomplete_download_audit) =
        build_feature_gated_audit_binaries();

    for (binary, usage_marker) in [
        (
            multipart_archive_audit.as_path(),
            "usage: disksage-multipart-archive-audit",
        ),
        (
            incomplete_download_audit.as_path(),
            "usage: disksage-incomplete-download-audit",
        ),
    ] {
        assert_help_success(binary, "--help", usage_marker);
        assert_help_success(binary, "-h", usage_marker);
        assert_invalid_argument_is_bounded(binary);
        assert_help_does_not_hide_invalid_argument(binary);
    }
}
