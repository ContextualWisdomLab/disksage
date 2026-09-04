//! Black-box process contracts for DiskSage Maven operational CLIs.

use std::process::Command;

/// Require one help flag to terminate successfully with the exact stable usage text.
fn assert_help_success(binary: &str, flag: &str, expected_usage: &str) {
    let output = Command::new(binary)
        .env_remove("HOME")
        .arg(flag)
        .output()
        .expect("DiskSage Maven CLI must launch for its help contract");

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
fn assert_invalid_argument_is_bounded(binary: &str) {
    let output = Command::new(binary)
        .env_remove("HOME")
        .arg("--opaque-option=not-shown")
        .output()
        .expect("DiskSage Maven CLI must launch for invalid argument validation");

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

/// Require a mixed help and invalid request to remain a bounded failure.
fn assert_help_does_not_hide_invalid_argument(binary: &str) {
    let output = Command::new(binary)
        .env_remove("HOME")
        .args(["--help", "--opaque-option=not-shown"])
        .output()
        .expect("DiskSage Maven CLI must launch for mixed help validation");

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

/// Require duplicate audit options to fail before a repository scan can begin.
fn assert_audit_duplicate_bound_is_rejected(binary: &str) {
    let repository = tempfile::tempdir().expect("empty Maven repository fixture must be created");
    let output = Command::new(binary)
        .env_remove("HOME")
        .arg("--repository-root")
        .arg(repository.path())
        .args(["--max-entries", "1", "--max-entries", "2"])
        .output()
        .expect("DiskSage Maven audit CLI must launch for duplicate-option validation");

    assert!(
        !output.status.success(),
        "duplicate --max-entries must be rejected instead of silently using the last value"
    );
    assert!(
        output.stdout.is_empty(),
        "duplicate-option failure must not emit a successful Maven audit document"
    );
    let stderr = String::from_utf8(output.stderr).expect("CLI diagnostics must remain valid UTF-8");
    assert!(
        !stderr.is_empty(),
        "duplicate-option failure must remain visible"
    );
}

/// Produce the exact candidate-set fingerprint for an empty repository through the real audit CLI.
fn empty_repository_fingerprint(audit_binary: &str, repository: &std::path::Path) -> String {
    let output = Command::new(audit_binary)
        .env_remove("HOME")
        .arg("--repository-root")
        .arg(repository)
        .output()
        .expect("DiskSage Maven audit CLI must launch for prune-fixture preparation");
    assert!(
        output.status.success(),
        "empty repository audit must succeed before the prune duplicate-option check: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("audit stdout must remain valid JSON");
    report["candidate_set_fingerprint"]
        .as_str()
        .expect("audit report must contain its candidate-set fingerprint")
        .to_string()
}

/// Require duplicate prune options to fail before a fingerprint-valid dry-run can execute.
fn assert_prune_duplicate_bound_is_rejected(audit_binary: &str, prune_binary: &str) {
    let repository = tempfile::tempdir().expect("empty Maven repository fixture must be created");
    let fingerprint = empty_repository_fingerprint(audit_binary, repository.path());
    let output = Command::new(prune_binary)
        .env_remove("HOME")
        .arg("--repository-root")
        .arg(repository.path())
        .args([
            "--expected-candidate-set-fingerprint",
            fingerprint.as_str(),
            "--max-entries",
            "1",
            "--max-entries",
            "2",
        ])
        .output()
        .expect("DiskSage Maven prune CLI must launch for duplicate-option validation");

    assert!(
        !output.status.success(),
        "duplicate --max-entries must be rejected instead of executing a fingerprint-valid dry-run"
    );
    assert!(
        output.stdout.is_empty(),
        "duplicate-option failure must not emit a successful Maven prune document"
    );
    let stderr = String::from_utf8(output.stderr).expect("CLI diagnostics must remain valid UTF-8");
    assert!(
        !stderr.is_empty(),
        "duplicate-option failure must remain visible"
    );
}

#[cfg(unix)]
fn assert_non_utf8_argument_is_bounded(binary: &str) {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let opaque = OsString::from_vec(vec![b'-', b'-', b'o', b'p', b'a', b'q', b'u', b'e', 0xff]);
    let output = Command::new(binary)
        .env_remove("HOME")
        .arg(opaque)
        .output()
        .expect("DiskSage Maven CLI must launch for non-UTF-8 argument validation");

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

/// Prove the Maven audit command's exact help and bounded invalid-input contract.
#[test]
fn maven_cache_audit_help_is_successful_and_invalid_arguments_are_bounded() {
    let binary = env!("CARGO_BIN_EXE_disksage-maven-cache-audit");
    let expected_usage = "usage: disksage-maven-cache-audit --repository-root ABSOLUTE_PATH [--output NEW_ABSOLUTE_JSON_PATH] [--max-entries N] [--max-candidates N] [--max-issues N]\n다음 단계: 후보와 후보 집합 지문을 검토하세요. 이 명령은 캐시를 제거하지 않습니다.";
    assert_help_success(binary, "--help", expected_usage);
    assert_help_success(binary, "-h", expected_usage);
    assert_invalid_argument_is_bounded(binary);
    assert_help_does_not_hide_invalid_argument(binary);
    assert_audit_duplicate_bound_is_rejected(binary);
    #[cfg(unix)]
    assert_non_utf8_argument_is_bounded(binary);
}

/// Prove the Maven prune command's exact help and bounded invalid-input contract.
#[test]
fn maven_cache_prune_help_is_successful_and_invalid_arguments_are_bounded() {
    let audit_binary = env!("CARGO_BIN_EXE_disksage-maven-cache-audit");
    let binary = env!("CARGO_BIN_EXE_disksage-maven-cache-prune");
    let expected_usage = "usage: disksage-maven-cache-prune --repository-root ABSOLUTE_PATH --expected-candidate-set-fingerprint HEX [--apply] [--max-entries N] [--output NEW_ABSOLUTE_JSON_PATH]\n다음 단계: 먼저 --apply 없이 결과와 지문을 확인한 뒤, 일치하는 계획에만 --apply를 사용하세요.";
    assert_help_success(binary, "--help", expected_usage);
    assert_help_success(binary, "-h", expected_usage);
    assert_invalid_argument_is_bounded(binary);
    assert_help_does_not_hide_invalid_argument(binary);
    assert_prune_duplicate_bound_is_rejected(audit_binary, binary);
    #[cfg(unix)]
    assert_non_utf8_argument_is_bounded(binary);
}
