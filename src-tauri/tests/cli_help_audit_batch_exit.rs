//! Black-box process contracts for DiskSage audit-oriented operational CLIs.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

const MULTIPART_AUDIT_SOURCE: &str = include_str!("../src/bin/disksage-multipart-archive-audit.rs");
const INCOMPLETE_AUDIT_SOURCE: &str =
    include_str!("../src/bin/disksage-incomplete-download-audit.rs");
const UNKNOWN_ARGUMENT: &str = "알 수 없는 인자";

/// Build both feature-gated audit binaries in an isolated Cargo target directory.
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

/// Require one help flag to terminate successfully with the exact stable usage text.
fn assert_help_success(binary: &Path, flag: &str, expected_usage: &str) {
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
    assert_eq!(
        stdout,
        format!("{expected_usage}\n"),
        "help output must equal the stable usage synopsis"
    );
}

/// Require an unknown option to fail visibly without reflecting its opaque payload.
fn assert_invalid_argument_is_bounded(binary: &Path) {
    let output = Command::new(binary)
        .env_remove("HOME")
        .arg("--opaque-option=not-shown")
        .output()
        .expect("DiskSage audit CLI must launch for invalid argument validation");

    assert_eq!(output.status.code(), Some(2));
    assert!(
        output.stdout.is_empty(),
        "invalid invocation must not emit successful output on stdout"
    );
    let stderr = String::from_utf8(output.stderr).expect("CLI diagnostics must be valid UTF-8");
    assert!(
        stderr.contains(UNKNOWN_ARGUMENT),
        "unknown input must use the stable bounded argument diagnostic"
    );
    assert!(
        !stderr.contains("not-shown"),
        "invalid diagnostics must not echo arbitrary argument payloads"
    );
}

/// Require a mixed help and invalid request to remain an argument failure, not failed help.
fn assert_help_does_not_hide_invalid_argument(binary: &Path) {
    let output = Command::new(binary)
        .env_remove("HOME")
        .args(["--help", "--opaque-option=not-shown"])
        .output()
        .expect("DiskSage audit CLI must launch for mixed help validation");

    assert_eq!(output.status.code(), Some(2));
    assert!(
        output.stdout.is_empty(),
        "mixed invalid invocation must not emit successful help on stdout"
    );
    let stderr = String::from_utf8(output.stderr).expect("CLI diagnostics must be valid UTF-8");
    assert!(
        stderr.contains(UNKNOWN_ARGUMENT),
        "help is terminal only when sole; mixed input must remain the stable argument failure"
    );
    assert!(
        !stderr.contains("usage:"),
        "mixed invalid input must not project successful-help content through stderr"
    );
    assert!(
        !stderr.contains("not-shown"),
        "mixed invalid diagnostics must not echo arbitrary argument payloads"
    );
}

/// Require a duplicate bounded option to fail before the read-only audit begins.
fn assert_duplicate_option_is_bounded(binary: &Path, extra_args: &[&str]) {
    let root = tempfile::tempdir().expect("empty audit root fixture must be created");
    let output = Command::new(binary)
        .env_remove("HOME")
        .arg("--root")
        .arg(root.path())
        .args(extra_args)
        .output()
        .expect("DiskSage audit CLI must launch for duplicate-option validation");

    assert!(
        !output.status.success(),
        "duplicate option must be rejected rather than silently selecting one value: {extra_args:?}"
    );
    assert!(
        output.stdout.is_empty(),
        "duplicate-option failure must not emit a successful audit summary"
    );
    let stderr = String::from_utf8(output.stderr).expect("CLI diagnostics must remain valid UTF-8");
    assert!(
        !stderr.is_empty(),
        "duplicate-option failure must remain visible"
    );
}

#[cfg(unix)]
fn assert_non_utf8_argument_is_bounded(binary: &Path) {
    use std::os::unix::ffi::OsStringExt;

    let opaque = OsString::from_vec(vec![b'-', b'-', b'o', b'p', b'a', b'q', b'u', b'e', 0xff]);
    let output = Command::new(binary)
        .env_remove("HOME")
        .arg(opaque)
        .output()
        .expect("DiskSage audit CLI must launch for non-UTF-8 argument validation");

    assert_eq!(
        output.status.code(),
        Some(2),
        "invalid non-UTF-8 input must use the ordinary bounded argument-error exit"
    );
    assert!(
        output.stdout.is_empty(),
        "invalid non-UTF-8 input must not emit successful output"
    );
    let stderr = String::from_utf8(output.stderr).expect("CLI diagnostics must remain valid UTF-8");
    assert!(stderr.contains(UNKNOWN_ARGUMENT));
    assert!(
        !stderr.contains("panicked") && !stderr.contains("thread 'main'"),
        "invalid host arguments must not escape through a Rust panic"
    );
}

#[cfg(unix)]
fn assert_native_non_utf8_root_reaches_audit(binary: &Path) {
    use std::os::unix::ffi::OsStringExt;

    let parent = tempfile::tempdir().expect("native-path parent fixture must be created");
    let root = parent.path().join(OsString::from_vec(vec![
        b'a', b'u', b'd', b'i', b't', b'-', 0xff,
    ]));
    std::fs::create_dir(&root).expect("native non-UTF-8 audit root must be created");

    let output = Command::new(binary)
        .env_remove("HOME")
        .arg("--root")
        .arg(&root)
        .output()
        .expect("DiskSage audit CLI must launch for native-path admission");

    assert!(
        output.status.success(),
        "valid native filesystem roots must reach the read-only audit boundary; status={:?}, stderr={:?}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "successful native-path audit must not emit argument diagnostics"
    );
    let summary: serde_json::Value = serde_json::from_slice(&output.stdout)
        .expect("successful native-path audit must emit machine-readable JSON");
    assert!(
        summary.is_object(),
        "audit success output must remain a JSON object"
    );
}

#[test]
fn audit_coverage_contract_keeps_shipped_entrypoints_real() {
    for (name, source) in [
        ("disksage-multipart-archive-audit", MULTIPART_AUDIT_SOURCE),
        (
            "disksage-incomplete-download-audit",
            INCOMPLETE_AUDIT_SOURCE,
        ),
    ] {
        assert!(
            !source.contains("#[cfg(coverage)]\nfn main()"),
            "coverage must never replace the shipped {name} entrypoint with a synthetic main"
        );
        assert!(
            !source.contains("#[cfg(not(coverage))]\nfn main()"),
            "the shipped {name} entrypoint must remain present under instrumentation"
        );
        assert!(
            !source.contains("#[cfg(not(coverage))]\nfn run()"),
            "the shipped {name} runtime must remain present under instrumentation"
        );
    }
}

/// Prove exact help output and bounded invalid-input behavior for both audit CLIs.
#[test]
fn audit_cli_help_is_successful_and_invalid_arguments_are_bounded() {
    let (_target_dir, multipart_archive_audit, incomplete_download_audit) =
        build_feature_gated_audit_binaries();

    for (binary, expected_usage) in [
        (
            multipart_archive_audit.as_path(),
            "usage: disksage-multipart-archive-audit --root ABSOLUTE_PATH [--max-entries 1..=200000] [--private-output ABSOLUTE_NEW_FILE.json]\n다음 단계: 누락되거나 중복된 분할 조각을 검토하세요. 이 명령은 파일을 이동하거나 삭제하지 않습니다.",
        ),
        (
            incomplete_download_audit.as_path(),
            "usage: disksage-incomplete-download-audit --root ABSOLUTE_PATH [--max-entries 1..=200000] [--stale-after-days 1..=3650] [--private-output ABSOLUTE_NEW_FILE.json]\n다음 단계: 불완전 항목과 근거 시각을 검토하세요. 이 명령은 파일을 이동하거나 삭제하지 않습니다.",
        ),
    ] {
        assert_help_success(binary, "--help", expected_usage);
        assert_help_success(binary, "-h", expected_usage);
        assert_invalid_argument_is_bounded(binary);
        assert_help_does_not_hide_invalid_argument(binary);
        assert_duplicate_option_is_bounded(binary, &["--max-entries", "1", "--max-entries", "2"]);
        #[cfg(unix)]
        {
            assert_non_utf8_argument_is_bounded(binary);
            assert_native_non_utf8_root_reaches_audit(binary);
        }
    }

    assert_duplicate_option_is_bounded(
        &incomplete_download_audit,
        &["--stale-after-days", "1", "--stale-after-days", "2"],
    );
}
