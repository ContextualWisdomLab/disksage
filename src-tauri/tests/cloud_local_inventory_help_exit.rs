//! Black-box process contract for the cloud local-inventory CLI.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

const EXPECTED_USAGE: &str = "usage: disksage-cloud-local-inventory (--cloud-root ABSOLUTE_PATH [--relative-subpath SAFE_RELATIVE_PATH] | --all-roots) [--min-allocated-mib N] [--max-entries N] [--max-results N] [--max-depth N] [--max-duration-ms N] [--max-issues N]\n다음 단계: 결과의 완전성과 공급자 상태를 검토하세요. 이 명령은 파일을 다운로드하거나 제거하지 않습니다.";

fn build_feature_gated_binary() -> PathBuf {
    let target_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("cloud-cli-contracts");
    std::fs::create_dir_all(&target_dir)
        .expect("shared Cargo contract target directory must be created");
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
        .arg(&target_dir)
        .status()
        .expect("cloud local-inventory CLI must be buildable for its process contract");
    assert!(
        status.success(),
        "feature-gated cloud local-inventory CLI build must succeed before process assertions"
    );

    let binary = target_dir.join("debug").join(format!(
        "disksage-cloud-local-inventory{}",
        std::env::consts::EXE_SUFFIX
    ));
    assert!(
        binary.is_file(),
        "cloud local-inventory binary must exist after the explicit cloud-cli build"
    );
    binary
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
    assert!(
        !stderr.is_empty(),
        "mixed invalid invocation must remain visible"
    );
    assert!(
        !stderr.contains("not-shown"),
        "mixed invalid diagnostics must not echo arbitrary argument payloads"
    );
}

fn assert_duplicate_batch_option_is_bounded(binary: &Path, duplicate_args: &[&str]) {
    let home = tempfile::tempdir().expect("isolated empty home must be created");
    let output = Command::new(binary)
        .env("HOME", home.path())
        .env_remove("USERPROFILE")
        .arg("--all-roots")
        .args(duplicate_args)
        .output()
        .expect("cloud local-inventory CLI must launch for duplicate-option validation");

    assert!(
        !output.status.success(),
        "duplicate option must fail before empty-home discovery can produce a successful batch report: {duplicate_args:?}"
    );
    assert!(
        output.stdout.is_empty(),
        "duplicate-option failure must not emit a successful inventory report"
    );
    let stderr = String::from_utf8(output.stderr).expect("CLI diagnostics must remain valid UTF-8");
    assert!(
        !stderr.is_empty(),
        "duplicate-option failure must remain visible"
    );
}

fn assert_duplicate_all_roots_is_bounded(binary: &Path) {
    let home = tempfile::tempdir().expect("isolated empty home must be created");
    let output = Command::new(binary)
        .env("HOME", home.path())
        .env_remove("USERPROFILE")
        .args(["--all-roots", "--all-roots"])
        .output()
        .expect("cloud local-inventory CLI must launch for duplicate --all-roots validation");

    assert!(
        !output.status.success(),
        "duplicate --all-roots must fail rather than silently producing an empty batch report"
    );
    assert!(output.stdout.is_empty());
}

fn assert_argument_failure(binary: &Path, args: &[&str], expected: &str) {
    let home = tempfile::tempdir().expect("isolated empty home must be created");
    let output = Command::new(binary)
        .env("HOME", home.path())
        .env_remove("USERPROFILE")
        .args(args)
        .output()
        .expect("cloud local-inventory CLI must launch for parser admission validation");

    assert_eq!(
        output.status.code(),
        Some(2),
        "invalid parser input must use the bounded argument-error exit: {args:?}"
    );
    assert!(
        output.stdout.is_empty(),
        "invalid parser input must not emit a successful inventory report: {args:?}"
    );
    let stderr = String::from_utf8(output.stderr).expect("CLI diagnostics must remain valid UTF-8");
    assert_eq!(
        stderr,
        format!("{expected}\n"),
        "parser admission diagnostics must stay exact and bounded: {args:?}"
    );
}

fn assert_parser_admission_matrix_is_bounded(binary: &Path) {
    for flag in [
        "--min-allocated-mib",
        "--max-entries",
        "--max-results",
        "--max-depth",
        "--max-duration-ms",
        "--max-issues",
    ] {
        assert_argument_failure(
            binary,
            &["--all-roots", flag],
            &format!("{flag} 값이 필요함"),
        );
        assert_argument_failure(
            binary,
            &["--all-roots", flag, "not-a-number"],
            &format!("{flag}는 정수여야 함"),
        );
    }

    assert_argument_failure(binary, &[], "--cloud-root 또는 --all-roots 값이 필요함");
    assert_argument_failure(binary, &["--cloud-root"], "--cloud-root 값이 필요함");
    assert_argument_failure(
        binary,
        &["--all-roots", "--relative-subpath"],
        "--relative-subpath 값이 필요함",
    );

    #[cfg(windows)]
    let absolute_root = r"C:\Cloud";
    #[cfg(not(windows))]
    let absolute_root = "/Cloud";

    assert_argument_failure(
        binary,
        &["--cloud-root", absolute_root, "--all-roots"],
        "--cloud-root와 --all-roots는 함께 사용할 수 없음",
    );
    assert_argument_failure(
        binary,
        &["--cloud-root", "relative"],
        "--cloud-root는 절대 경로여야 함",
    );
    for relative in ["", "../escape", ".", "./Archive"] {
        assert_argument_failure(
            binary,
            &[
                "--cloud-root",
                absolute_root,
                "--relative-subpath",
                relative,
            ],
            "--relative-subpath는 안전한 상대 경로여야 함",
        );
    }
    #[cfg(windows)]
    let absolute_subpath = r"C:\escape";
    #[cfg(not(windows))]
    let absolute_subpath = "/escape";
    assert_argument_failure(
        binary,
        &[
            "--cloud-root",
            absolute_root,
            "--relative-subpath",
            absolute_subpath,
        ],
        "--relative-subpath는 안전한 상대 경로여야 함",
    );
    assert_argument_failure(
        binary,
        &["--all-roots", "--relative-subpath", "Archive"],
        "--relative-subpath는 --all-roots와 함께 사용할 수 없음",
    );
}

#[cfg(unix)]
fn assert_non_utf8_argument_is_bounded(binary: &Path) {
    use std::os::unix::ffi::OsStringExt;

    let opaque = OsString::from_vec(vec![b'-', b'-', b'o', b'p', b'a', b'q', b'u', b'e', 0xff]);
    let output = Command::new(binary)
        .env_remove("HOME")
        .env_remove("USERPROFILE")
        .arg(opaque)
        .output()
        .expect("cloud local-inventory CLI must launch for non-UTF-8 argument validation");

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
    assert!(
        !stderr.is_empty(),
        "invalid non-UTF-8 input must remain visible"
    );
    assert!(
        !stderr.contains("panicked") && !stderr.contains("thread 'main'"),
        "invalid host arguments must not escape through a Rust panic"
    );
}

#[test]
fn cloud_local_inventory_help_is_successful_and_invalid_arguments_are_bounded() {
    let binary = build_feature_gated_binary();
    assert_help_success(&binary, "--help");
    assert_help_success(&binary, "-h");
    assert_invalid_argument_is_bounded(&binary);
    assert_help_does_not_hide_invalid_argument(&binary);
    assert_duplicate_all_roots_is_bounded(&binary);
    for duplicate_args in [
        ["--min-allocated-mib", "1", "--min-allocated-mib", "2"],
        ["--max-entries", "1", "--max-entries", "2"],
        ["--max-results", "1", "--max-results", "2"],
        ["--max-depth", "1", "--max-depth", "2"],
        ["--max-duration-ms", "1", "--max-duration-ms", "2"],
        ["--max-issues", "1", "--max-issues", "2"],
    ] {
        assert_duplicate_batch_option_is_bounded(&binary, &duplicate_args);
    }
    assert_parser_admission_matrix_is_bounded(&binary);
    #[cfg(unix)]
    assert_non_utf8_argument_is_bounded(&binary);
}
