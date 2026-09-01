//! Black-box process contract for the shipped Git-worktree audit CLI.
//!
//! Help is a terminal discovery action: it must succeed without repository, HOME, Git, or
//! filesystem-domain work. Invalid and malformed host input remains a bounded non-zero failure.
//!
//! Cargo builds the shipped binary alongside this integration test, so the process contract runs
//! in the default native test suite without a nested target directory.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

const EXPECTED_USAGE: &str = "usage: disksage-git-worktree-audit --repository-root ABSOLUTE_PATH --reference-ref REF [--reference-ref REF ...] [--include-closed-pull-requests] [--stale-open-pull-request-cutoff-ms N] [--private-output NEW_ABSOLUTE_JSON_PATH] [--command-timeout-ms N] [--size-scan-timeout-ms N] [--max-worktrees N] [--max-entries-per-worktree N] [--max-active-pids N]";
const OID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn binary_path() -> &'static Path {
    static BINARY_PATH: OnceLock<PathBuf> = OnceLock::new();
    BINARY_PATH
        .get_or_init(|| PathBuf::from(env!("CARGO_BIN_EXE_disksage-git-worktree-audit")))
        .as_path()
}

fn command() -> Command {
    Command::new(binary_path())
}

fn assert_exact_failure(arguments: &[&str], expected: &str) {
    let output = command()
        .args(arguments)
        .output()
        .expect("Git worktree audit binary should start");
    assert_eq!(output.status.code(), Some(2), "arguments: {arguments:?}");
    assert!(output.stdout.is_empty(), "arguments: {arguments:?}");
    assert_eq!(
        String::from_utf8(output.stderr).expect("stderr should remain UTF-8"),
        format!("DiskSage Git worktree audit: {expected}\n"),
        "arguments: {arguments:?}"
    );
}

fn git(repository: &Path, arguments: &[&str]) {
    let output = Command::new("git")
        .current_dir(repository)
        .args(arguments)
        .output()
        .expect("Git should be available to the worktree process contract");
    assert!(
        output.status.success(),
        "git {arguments:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn initialized_repository() -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().expect("temporary directory should be created");
    let repository = temp.path().join("private-customer-repository");
    fs::create_dir(&repository).expect("repository directory should be created");
    git(&repository, &["init", "-q"]);
    git(&repository, &["config", "user.email", "coverage@example.invalid"]);
    git(&repository, &["config", "user.name", "DiskSage Test"]);
    fs::write(repository.join("tracked.txt"), b"tracked\n").expect("tracked fixture should be written");
    git(&repository, &["add", "tracked.txt"]);
    git(&repository, &["commit", "-q", "-m", "fixture"]);
    (temp, repository)
}

#[test]
fn sole_help_flags_are_terminal_success_without_domain_environment() {
    for flag in ["--help", "-h"] {
        let output = command()
            .env_remove("HOME")
            .env_remove("USERPROFILE")
            .env_remove("APPDATA")
            .env_remove("XDG_DATA_HOME")
            .env("PATH", "")
            .arg(flag)
            .output()
            .expect("Git worktree audit binary should start");

        assert_eq!(output.status.code(), Some(0), "help flag: {flag}");
        assert_eq!(
            String::from_utf8(output.stdout).expect("help stdout should remain UTF-8"),
            format!("{EXPECTED_USAGE}\n"),
            "help flag: {flag}"
        );
        assert!(output.stderr.is_empty(), "help flag: {flag}");
    }
}

#[test]
fn help_mixed_with_runtime_input_stays_a_bounded_failure() {
    let sensitive_path = "/private/customer/repository";
    let output = command()
        .args(["--help", "--repository-root", sensitive_path])
        .output()
        .expect("Git worktree audit binary should start");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("stderr should remain UTF-8");
    assert_eq!(
        stderr,
        "DiskSage Git worktree audit: help-cannot-be-combined-with-runtime-input\n"
    );
    assert!(!stderr.contains(sensitive_path));
    assert!(!stderr.contains("panicked") && !stderr.contains("thread 'main'"));
}

#[test]
fn primary_worktree_audit_keeps_machine_json_path_redacted_and_read_only() {
    let (_temp, repository) = initialized_repository();

    let output = command()
        .arg("--repository-root")
        .arg(&repository)
        .args(["--reference-ref", "HEAD"])
        .output()
        .expect("Git worktree audit binary should start");

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).expect("audit stdout should remain UTF-8 JSON");
    let report: serde_json::Value = serde_json::from_str(&stdout).expect("audit stdout should be JSON");
    assert_eq!(report["schema_kind"], "disksage.git-worktree-audit/v4");
    assert_eq!(report["version"], 4);
    assert_eq!(report["worktree_count"], 1);
    assert_eq!(report["removal_candidate_count"], 0);
    assert_eq!(report["preserved_count"], 1);
    assert_eq!(report["filesystem_mutation_executed"], false);
    assert_eq!(report["local_paths_redacted"], true);
    assert_eq!(report["branch_names_redacted"], true);
    assert!(
        !stdout.contains(&repository.to_string_lossy().to_string()),
        "public machine JSON must not expose the selected repository path"
    );
}

#[cfg(not(unix))]
#[test]
fn private_output_fails_closed_when_secure_unix_mode_is_unavailable() {
    let (temp, repository) = initialized_repository();
    let private_output = temp.path().join("private-report.json");

    let output = command()
        .arg("--repository-root")
        .arg(&repository)
        .args(["--reference-ref", "HEAD", "--private-output"])
        .arg(&private_output)
        .output()
        .expect("Git worktree audit binary should start");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).expect("stderr should remain UTF-8"),
        "DiskSage Git worktree audit: git-worktree-private-output-secure-mode-unsupported\n"
    );
    assert!(!private_output.exists());
}

#[test]
fn unknown_and_missing_arguments_are_exact_bounded_failures() {
    assert_exact_failure(&["--opaque-option=customer-secret"], "unknown-argument");
    assert_exact_failure(&["--repository-root"], "--repository-root 값이 필요함");
    assert_exact_failure(
        &["--repository-root", "/repository", "--reference-ref"],
        "--reference-ref 값이 필요함",
    );
    assert_exact_failure(
        &[
            "--repository-root",
            "/repository",
            "--reference-ref",
            OID,
            "--private-output",
        ],
        "--private-output 값이 필요함",
    );
    for flag in [
        "--command-timeout-ms",
        "--size-scan-timeout-ms",
        "--max-worktrees",
        "--max-entries-per-worktree",
        "--max-active-pids",
    ] {
        assert_exact_failure(
            &["--repository-root", "/repository", "--reference-ref", OID, flag],
            &format!("{flag} 값이 필요함"),
        );
    }
}

#[test]
fn unsafe_paths_and_missing_reference_authority_fail_before_domain_work() {
    assert_exact_failure(
        &["--repository-root", "relative/repository", "--reference-ref", OID],
        "--repository-root는 절대 경로여야 함",
    );
    assert_exact_failure(
        &[
            "--repository-root",
            "/definitely/not/a/real/repository",
            "--reference-ref",
            OID,
            "--private-output",
            "relative/report.json",
        ],
        "--private-output은 절대 경로여야 함",
    );
    assert_exact_failure(
        &["--repository-root", "/definitely/not/a/real/repository"],
        "--reference-ref 값이 하나 이상 필요함",
    );
}

#[test]
fn duplicate_singleton_options_fail_before_git_or_filesystem_work() {
    let cases: &[&[&str]] = &[
        &[
            "--repository-root",
            "/first/repository",
            "--repository-root",
            "/second/repository",
            "--reference-ref",
            OID,
        ],
        &[
            "--repository-root",
            "/repository",
            "--reference-ref",
            OID,
            "--private-output",
            "/first.json",
            "--private-output",
            "/second.json",
        ],
        &[
            "--repository-root",
            "/repository",
            "--reference-ref",
            OID,
            "--command-timeout-ms",
            "10",
            "--command-timeout-ms",
            "20",
        ],
        &[
            "--repository-root",
            "/repository",
            "--reference-ref",
            OID,
            "--size-scan-timeout-ms",
            "10",
            "--size-scan-timeout-ms",
            "20",
        ],
        &[
            "--repository-root",
            "/repository",
            "--reference-ref",
            OID,
            "--max-worktrees",
            "10",
            "--max-worktrees",
            "20",
        ],
        &[
            "--repository-root",
            "/repository",
            "--reference-ref",
            OID,
            "--max-entries-per-worktree",
            "10",
            "--max-entries-per-worktree",
            "20",
        ],
        &[
            "--repository-root",
            "/repository",
            "--reference-ref",
            OID,
            "--max-active-pids",
            "1",
            "--max-active-pids",
            "2",
        ],
    ];

    for arguments in cases {
        assert_exact_failure(arguments, "duplicate-option");
    }
}

#[test]
fn out_of_range_limits_fail_before_git_or_filesystem_work() {
    let cases = [
        ("--command-timeout-ms", "0", "git-worktree-command-timeout-out-of-bounds"),
        ("--command-timeout-ms", "3600001", "git-worktree-command-timeout-out-of-bounds"),
        ("--size-scan-timeout-ms", "0", "git-worktree-size-timeout-out-of-bounds"),
        ("--size-scan-timeout-ms", "600001", "git-worktree-size-timeout-out-of-bounds"),
        ("--max-worktrees", "0", "git-worktree-count-limit-out-of-bounds"),
        ("--max-worktrees", "10001", "git-worktree-count-limit-out-of-bounds"),
        ("--max-entries-per-worktree", "0", "git-worktree-entry-limit-out-of-bounds"),
        ("--max-entries-per-worktree", "20000001", "git-worktree-entry-limit-out-of-bounds"),
        ("--max-active-pids", "0", "git-worktree-active-pid-limit-out-of-bounds"),
        ("--max-active-pids", "4097", "git-worktree-active-pid-limit-out-of-bounds"),
    ];

    for (flag, value, expected) in cases {
        assert_exact_failure(
            &[
                "--repository-root",
                "/definitely/not/a/real/repository",
                "--reference-ref",
                OID,
                flag,
                value,
            ],
            expected,
        );
    }
}

#[cfg(unix)]
#[test]
fn non_utf8_option_shaped_input_is_bounded_and_non_reflective() {
    use std::os::unix::ffi::OsStringExt;

    let opaque = OsString::from_vec(vec![b'-', b'-', b'o', b'p', b'a', b'q', b'u', b'e', 0xff]);
    let output = command()
        .arg(opaque)
        .output()
        .expect("Git worktree audit binary should start");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).expect("stderr should remain UTF-8"),
        "DiskSage Git worktree audit: invalid-argument-encoding\n"
    );
}
