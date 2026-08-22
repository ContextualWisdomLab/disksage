//! Black-box admission regressions for the shipped cloud local-inventory CLI.
//!
//! These cases terminate during argument parsing, before home discovery, provider discovery,
//! filesystem traversal, watchdog work, or JSON report production. They complement the dedicated
//! help/non-UTF-8 process contract by exercising the remaining bounded parser failure modes on the
//! real feature-gated executable.

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
            "disksage-cloud-local-inventory",
            "--target-dir",
        ])
        .arg(target_dir.path())
        .status()
        .expect("cloud local-inventory CLI must be buildable for admission tests");
    assert!(status.success(), "feature-gated CLI build must succeed");

    let binary = target_dir
        .path()
        .join("debug")
        .join(format!(
            "disksage-cloud-local-inventory{}",
            std::env::consts::EXE_SUFFIX
        ));
    assert!(binary.is_file(), "feature-gated CLI binary must exist");
    (target_dir, binary)
}

fn assert_rejected(binary: &Path, args: &[&str], expected: &str) {
    let output = Command::new(binary)
        .env_remove("HOME")
        .env_remove("USERPROFILE")
        .args(args)
        .output()
        .expect("cloud local-inventory CLI must launch for admission validation");

    assert_eq!(output.status.code(), Some(2), "args: {args:?}");
    assert!(output.stdout.is_empty(), "args: {args:?}");
    assert_eq!(
        output.stderr,
        format!("{expected}\n").as_bytes(),
        "args: {args:?}"
    );
}

#[test]
fn parser_rejects_missing_values_non_numbers_and_duplicate_options_before_domain_work() {
    let (_target_dir, binary) = build_feature_gated_binary();

    assert_rejected(&binary, &["--cloud-root"], "--cloud-root 값이 필요함");
    assert_rejected(
        &binary,
        &["--cloud-root", "/cloud", "--relative-subpath"],
        "--relative-subpath 값이 필요함",
    );

    for flag in [
        "--min-allocated-mib",
        "--max-entries",
        "--max-results",
        "--max-depth",
        "--max-duration-ms",
        "--max-issues",
    ] {
        assert_rejected(
            &binary,
            &["--all-roots", flag],
            &format!("{flag} 값이 필요함"),
        );
        assert_rejected(
            &binary,
            &["--all-roots", flag, "not-a-number"],
            &format!("{flag}는 정수여야 함"),
        );
        assert_rejected(
            &binary,
            &["--all-roots", flag, "1", flag, "2"],
            &format!("{flag}는 한 번만 지정할 수 있음"),
        );
    }

    assert_rejected(
        &binary,
        &["--cloud-root", "/first", "--cloud-root", "/second"],
        "--cloud-root는 한 번만 지정할 수 있음",
    );
    assert_rejected(
        &binary,
        &[
            "--cloud-root",
            "/cloud",
            "--relative-subpath",
            "first",
            "--relative-subpath",
            "second",
        ],
        "--relative-subpath는 한 번만 지정할 수 있음",
    );
    assert_rejected(
        &binary,
        &["--all-roots", "--all-roots"],
        "--all-roots는 한 번만 지정할 수 있음",
    );
}

#[test]
fn parser_rejects_conflicting_and_unsafe_root_selection_before_home_or_provider_discovery() {
    let (_target_dir, binary) = build_feature_gated_binary();

    assert_rejected(
        &binary,
        &[],
        "--cloud-root 또는 --all-roots 값이 필요함",
    );
    assert_rejected(
        &binary,
        &["--cloud-root", "/cloud", "--all-roots"],
        "--cloud-root와 --all-roots는 함께 사용할 수 없음",
    );
    assert_rejected(
        &binary,
        &["--cloud-root", "relative-cloud"],
        "--cloud-root는 절대 경로여야 함",
    );
    assert_rejected(
        &binary,
        &[
            "--cloud-root",
            "/cloud",
            "--relative-subpath",
            "../escape",
        ],
        "--relative-subpath는 안전한 상대 경로여야 함",
    );
    assert_rejected(
        &binary,
        &[
            "--cloud-root",
            "/cloud",
            "--relative-subpath",
            "/absolute",
        ],
        "--relative-subpath는 안전한 상대 경로여야 함",
    );
    assert_rejected(
        &binary,
        &[
            "--cloud-root",
            "/cloud",
            "--relative-subpath",
            "",
        ],
        "--relative-subpath는 안전한 상대 경로여야 함",
    );
    for relative in [".", "./Archive"] {
        assert_rejected(
            &binary,
            &["--cloud-root", "/cloud", "--relative-subpath", relative],
            "--relative-subpath는 안전한 상대 경로여야 함",
        );
    }
    assert_rejected(
        &binary,
        &["--all-roots", "--relative-subpath", "Archive"],
        "--relative-subpath는 --all-roots와 함께 사용할 수 없음",
    );
}
