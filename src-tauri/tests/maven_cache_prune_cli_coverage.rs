//! Executable-boundary coverage for the Maven cache prune CLI.
//!
//! These tests exercise the shipped command-line parser, fail-closed error surface, dry-run
//! execution, private create-new report publication, and repeat-write refusal without deleting
//! Maven artifacts or enabling apply authority.

use disksage_lib::maven_cache::{audit_maven_repository, MavenCacheAuditOptions};
use std::process::{Command, Output};

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_disksage-maven-cache-prune"))
}

fn run(args: &[&str]) -> Output {
    cli().args(args).output().expect("Maven prune CLI must start")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn cli_rejects_incomplete_or_unsafe_arguments_fail_closed() {
    let help = run(&["--help"]);
    assert_eq!(help.status.code(), Some(2));
    assert!(stderr(&help).contains("usage: disksage-maven-cache-prune"));

    let missing_root_value = run(&["--repository-root"]);
    assert_eq!(missing_root_value.status.code(), Some(2));
    assert!(stderr(&missing_root_value).contains("--repository-root 값이 필요함"));

    let unknown = run(&["--unknown"]);
    assert_eq!(unknown.status.code(), Some(2));
    assert!(stderr(&unknown).contains("알 수 없는 인자: --unknown"));

    let relative_root = run(&[
        "--repository-root",
        "relative/repository",
        "--expected-candidate-set-fingerprint",
        &"a".repeat(64),
    ]);
    assert_eq!(relative_root.status.code(), Some(2));
    assert!(stderr(&relative_root).contains("--repository-root는 절대 경로여야 함"));

    let invalid_limit = run(&[
        "--repository-root",
        "/tmp",
        "--expected-candidate-set-fingerprint",
        &"a".repeat(64),
        "--max-entries",
        "not-a-number",
    ]);
    assert_eq!(invalid_limit.status.code(), Some(2));
    assert!(stderr(&invalid_limit).contains("--max-entries는 정수여야 함"));

    let zero_limit = run(&[
        "--repository-root",
        "/tmp",
        "--expected-candidate-set-fingerprint",
        &"a".repeat(64),
        "--max-entries",
        "0",
    ]);
    assert_eq!(zero_limit.status.code(), Some(2));
    assert!(stderr(&zero_limit).contains("--max-entries는 1 이상이어야 함"));

    let relative_output = run(&[
        "--repository-root",
        "/tmp",
        "--expected-candidate-set-fingerprint",
        &"a".repeat(64),
        "--output",
        "relative.json",
    ]);
    assert_eq!(relative_output.status.code(), Some(2));
    assert!(stderr(&relative_output).contains("--output은 절대 경로여야 함"));
}

#[test]
fn cli_dry_run_publishes_private_create_new_evidence_and_refuses_overwrite() {
    let temp = tempfile::tempdir().unwrap();
    let repository = temp.path().join("repository");
    std::fs::create_dir(&repository).unwrap();

    let audit = audit_maven_repository(
        &repository,
        MavenCacheAuditOptions {
            max_entries: 100,
            max_candidates: usize::MAX,
            max_issues: 20,
        },
        1,
    )
    .unwrap();
    assert!(!audit.truncated);
    let output_path = temp.path().join("prune-report.json");
    let repository_text = repository.to_string_lossy().into_owned();
    let output_text = output_path.to_string_lossy().into_owned();

    let first = cli()
        .args([
            "--repository-root",
            repository_text.as_str(),
            "--expected-candidate-set-fingerprint",
            audit.candidate_set_fingerprint.as_str(),
            "--max-entries",
            "100",
            "--output",
            output_text.as_str(),
        ])
        .output()
        .expect("dry-run CLI must start");
    assert!(first.status.success(), "{}", stderr(&first));

    let summary: serde_json::Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(summary["schema_kind"], "disksage.maven-cache-prune/v1");
    assert_eq!(summary["output"], output_text);
    assert_eq!(summary["candidate_directories"], 0);
    assert_eq!(summary["candidate_bytes"], 0);
    assert_eq!(summary["removed_directories"], 0);
    assert_eq!(summary["removed_bytes"], 0);
    assert_eq!(summary["skipped_directories"], 0);
    assert_eq!(summary["apply_requested"], false);
    assert_eq!(summary["filesystem_mutation_executed"], false);
    assert_eq!(summary["complete"], true);

    let published = std::fs::read(&output_path).unwrap();
    let report: serde_json::Value = serde_json::from_slice(&published).unwrap();
    assert_eq!(report["schema_kind"], "disksage.maven-cache-prune/v1");
    assert_eq!(
        report["observed_candidate_set_fingerprint"],
        audit.candidate_set_fingerprint
    );
    assert_eq!(report["apply_requested"], false);
    assert_eq!(report["filesystem_mutation_executed"], false);
    assert_eq!(report["complete"], true);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&output_path).unwrap().permissions().mode() & 0o777, 0o600);
    }

    let before = published;
    let second = cli()
        .args([
            "--repository-root",
            repository_text.as_str(),
            "--expected-candidate-set-fingerprint",
            audit.candidate_set_fingerprint.as_str(),
            "--max-entries",
            "100",
            "--output",
            output_text.as_str(),
        ])
        .output()
        .expect("repeat dry-run CLI must start");
    assert_eq!(second.status.code(), Some(2));
    assert!(stderr(&second).contains("maven-cache-prune-output-create-failed"));
    assert_eq!(std::fs::read(&output_path).unwrap(), before);
}
