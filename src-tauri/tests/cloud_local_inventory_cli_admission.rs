//! Black-box admission regressions for the shipped cloud local-inventory CLI.
//!
//! The rejection cases terminate during argument parsing, before home discovery, provider
//! discovery, filesystem traversal, watchdog work, or JSON report production. The success cases
//! additionally exercise the shipped read-only batch and single-root runtime paths without
//! requiring a real provider account.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

fn binary_path() -> &'static Path {
    static BINARY_PATH: OnceLock<PathBuf> = OnceLock::new();
    BINARY_PATH
        .get_or_init(|| {
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
                .expect("cloud local-inventory CLI must be buildable for admission tests");
            assert!(status.success(), "feature-gated CLI build must succeed");

            let binary = target_dir.join("debug").join(format!(
                "disksage-cloud-local-inventory{}",
                std::env::consts::EXE_SUFFIX
            ));
            assert!(binary.is_file(), "feature-gated CLI binary must exist");
            binary
        })
        .as_path()
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
    let binary = binary_path();

    assert_rejected(binary, &["--cloud-root"], "--cloud-root 값이 필요함");
    assert_rejected(
        binary,
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
            binary,
            &["--all-roots", flag],
            &format!("{flag} 값이 필요함"),
        );
        assert_rejected(
            binary,
            &["--all-roots", flag, "not-a-number"],
            &format!("{flag}는 정수여야 함"),
        );
        assert_rejected(
            binary,
            &["--all-roots", flag, "1", flag, "2"],
            &format!("{flag}는 한 번만 지정할 수 있음"),
        );
    }

    assert_rejected(
        binary,
        &["--cloud-root", "/first", "--cloud-root", "/second"],
        "--cloud-root는 한 번만 지정할 수 있음",
    );
    assert_rejected(
        binary,
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
        binary,
        &["--all-roots", "--all-roots"],
        "--all-roots는 한 번만 지정할 수 있음",
    );
}

#[test]
fn parser_rejects_conflicting_and_unsafe_root_selection_before_home_or_provider_discovery() {
    let binary = binary_path();

    assert_rejected(
        binary,
        &[],
        "--cloud-root 또는 --all-roots 값이 필요함",
    );
    assert_rejected(
        binary,
        &["--cloud-root", "/cloud", "--all-roots"],
        "--cloud-root와 --all-roots는 함께 사용할 수 없음",
    );
    assert_rejected(
        binary,
        &["--cloud-root", "relative-cloud"],
        "--cloud-root는 절대 경로여야 함",
    );
    assert_rejected(
        binary,
        &[
            "--cloud-root",
            "/cloud",
            "--relative-subpath",
            "../escape",
        ],
        "--relative-subpath는 안전한 상대 경로여야 함",
    );
    assert_rejected(
        binary,
        &[
            "--cloud-root",
            "/cloud",
            "--relative-subpath",
            "/absolute",
        ],
        "--relative-subpath는 안전한 상대 경로여야 함",
    );
    assert_rejected(
        binary,
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
            binary,
            &["--cloud-root", "/cloud", "--relative-subpath", relative],
            "--relative-subpath는 안전한 상대 경로여야 함",
        );
    }
    assert_rejected(
        binary,
        &["--all-roots", "--relative-subpath", "Archive"],
        "--relative-subpath는 --all-roots와 함께 사용할 수 없음",
    );
}

#[test]
fn empty_home_and_synthetic_onedrive_return_bounded_read_only_json_evidence() {
    let binary = binary_path();

    let invalid_home = Command::new(binary)
        .env("HOME", "")
        .env("USERPROFILE", "")
        .arg("--all-roots")
        .output()
        .expect("cloud local-inventory CLI must launch for empty-home admission");
    assert_eq!(invalid_home.status.code(), Some(2));
    assert!(invalid_home.stdout.is_empty());
    assert_eq!(
        invalid_home.stderr,
        "HOME/USERPROFILE을 찾을 수 없음\n".as_bytes()
    );

    let home = tempfile::tempdir().expect("isolated empty home must be created");
    let output = Command::new(binary)
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .arg("--all-roots")
        .output()
        .expect("cloud local-inventory CLI must launch for empty-home batch inventory");

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    assert!(output.stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("success output must be valid JSON");
    assert_eq!(report["version"], 1);
    assert_eq!(report["discovered_roots"], 0);
    assert_eq!(report["reported_roots"], 0);
    assert_eq!(report["failed_roots"], 0);
    assert_eq!(report["candidate_count"], 0);
    assert_eq!(report["allocated_candidate_bytes"], 0);
    assert_eq!(report["evidence_complete"], false);
    assert_eq!(report["reports"].as_array().map(Vec::len), Some(0));
    assert_eq!(report["failures"].as_array().map(Vec::len), Some(0));
    let notices = report["notices"]
        .as_array()
        .expect("batch report notices must be an array");
    for expected in [
        "metadata-only-content-not-opened",
        "batch-inventory-does-not-authorize-eviction",
        "no-cloud-roots-discovered",
    ] {
        assert!(
            notices.iter().any(|notice| notice.as_str() == Some(expected)),
            "missing notice {expected}: {notices:?}"
        );
    }

    let onedrive = home.path().join("OneDrive");
    std::fs::create_dir(&onedrive).expect("synthetic OneDrive root must be created");
    let single = Command::new(binary)
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .arg("--cloud-root")
        .arg(&onedrive)
        .args([
            "--min-allocated-mib",
            "0",
            "--max-entries",
            "16",
            "--max-results",
            "16",
            "--max-depth",
            "2",
            "--max-duration-ms",
            "2000",
            "--max-issues",
            "16",
        ])
        .output()
        .expect("cloud local-inventory CLI must launch for synthetic OneDrive inventory");

    assert!(single.status.success(), "stderr: {}", String::from_utf8_lossy(&single.stderr));
    assert!(single.stderr.is_empty());
    let single_report: serde_json::Value =
        serde_json::from_slice(&single.stdout).expect("single-root output must be valid JSON");
    assert_eq!(single_report["version"], 2);
    assert_eq!(single_report["provider"], "onedrive");
    assert_eq!(single_report["account_scope"], "unknown");
    assert_eq!(
        single_report["cloud_root_id"],
        onedrive.to_string_lossy().as_ref()
    );
    assert_eq!(
        single_report["cloud_root"],
        onedrive.to_string_lossy().as_ref()
    );
    assert_eq!(single_report["allocated_candidate_bytes"], 0);
    assert_eq!(single_report["evidence_complete"], true);
    assert_eq!(single_report["issues_truncated"], false);
    assert_eq!(single_report["results_truncated"], false);
    assert_eq!(single_report["candidates"].as_array().map(Vec::len), Some(0));
    assert_eq!(single_report["stop_reasons"].as_array().map(Vec::len), Some(0));
    let single_notices = single_report["notices"]
        .as_array()
        .expect("single-root notices must be an array");
    for expected in [
        "metadata-only-content-not-opened",
        "embedded-production-metadata-not-inspected",
        "provider-sync-not-attested",
        "inventory-does-not-authorize-eviction",
    ] {
        assert!(
            single_notices
                .iter()
                .any(|notice| notice.as_str() == Some(expected)),
            "missing notice {expected}: {single_notices:?}"
        );
    }
}
