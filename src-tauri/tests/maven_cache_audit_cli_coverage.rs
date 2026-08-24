//! Executable-boundary coverage for the read-only Maven cache audit CLI.
//!
//! This coverage line deliberately does not freeze `--help` behavior because PR #214 owns the
//! Maven CLI help contract. It exercises only stable invalid-input, stdout-report, private
//! create-new output, and repeat-write refusal behavior through the shipped binary.

use std::process::{Command, Output};

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_disksage-maven-cache-audit"))
}

fn run(args: &[&str]) -> Output {
    cli().args(args).output().expect("Maven audit CLI must start")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn cli_rejects_incomplete_unsafe_and_unbounded_arguments_fail_closed() {
    let missing_root = run(&[]);
    assert_eq!(missing_root.status.code(), Some(2));
    assert!(stderr(&missing_root).contains("--repository-root 값이 필요함"));

    let missing_root_value = run(&["--repository-root"]);
    assert_eq!(missing_root_value.status.code(), Some(2));
    assert!(stderr(&missing_root_value).contains("--repository-root 값이 필요함"));

    let missing_output_value = run(&["--repository-root", "/tmp", "--output"]);
    assert_eq!(missing_output_value.status.code(), Some(2));
    assert!(stderr(&missing_output_value).contains("--output 값이 필요함"));

    let relative_root = run(&["--repository-root", "relative/repository"]);
    assert_eq!(relative_root.status.code(), Some(2));
    assert!(stderr(&relative_root).contains("--repository-root는 절대 경로여야 함"));

    let relative_output = run(&[
        "--repository-root",
        "/tmp",
        "--output",
        "relative.json",
    ]);
    assert_eq!(relative_output.status.code(), Some(2));
    assert!(stderr(&relative_output).contains("--output은 절대 경로여야 함"));

    for flag in ["--max-entries", "--max-candidates", "--max-issues"] {
        let invalid = run(&["--repository-root", "/tmp", flag, "not-a-number"]);
        assert_eq!(invalid.status.code(), Some(2));
        assert!(stderr(&invalid).contains(&format!("{flag}는 정수여야 함")));
    }

    let zero_entries = run(&[
        "--repository-root",
        "/tmp",
        "--max-entries",
        "0",
    ]);
    assert_eq!(zero_entries.status.code(), Some(2));
    assert!(stderr(&zero_entries).contains("--max-entries는 1 이상이어야 함"));

    let unknown = run(&["--unknown"]);
    assert_eq!(unknown.status.code(), Some(2));
    assert!(stderr(&unknown).contains("알 수 없는 인자: --unknown"));
}

#[test]
fn cli_emits_read_only_stdout_report_and_private_create_new_file() {
    let temp = tempfile::tempdir().unwrap();
    let repository = temp.path().join("repository");
    std::fs::create_dir(&repository).unwrap();
    let repository_text = repository.to_string_lossy().into_owned();

    let stdout_report = cli()
        .args([
            "--repository-root",
            repository_text.as_str(),
            "--max-entries",
            "100",
            "--max-candidates",
            "5",
            "--max-issues",
            "5",
        ])
        .output()
        .expect("stdout audit must start");
    assert!(stdout_report.status.success(), "{}", stderr(&stdout_report));
    assert!(stdout_report.stderr.is_empty());
    let report: serde_json::Value = serde_json::from_slice(&stdout_report.stdout).unwrap();
    assert_eq!(report["schema_kind"], "disksage.maven-cache-audit/v1");
    assert_eq!(report["repository_root"], repository_text);
    assert_eq!(report["remote_recoverable_directories"], 0);
    assert_eq!(report["held_directories"], 0);
    assert_eq!(report["provider_write_executed"], false);
    assert_eq!(report["scan_truncated"], false);

    let output_path = temp.path().join("audit-report.json");
    let output_text = output_path.to_string_lossy().into_owned();
    let first = cli()
        .args([
            "--repository-root",
            repository_text.as_str(),
            "--output",
            output_text.as_str(),
            "--max-entries",
            "100",
            "--max-candidates",
            "5",
            "--max-issues",
            "5",
        ])
        .output()
        .expect("file audit must start");
    assert!(first.status.success(), "{}", stderr(&first));
    assert!(first.stderr.is_empty());

    let summary: serde_json::Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(summary["schema_kind"], "disksage.maven-cache-audit/v1");
    assert_eq!(summary["output"], output_text);
    assert_eq!(summary["remote_recoverable_directories"], 0);
    assert_eq!(summary["held_directories"], 0);
    assert_eq!(summary["provider_write_executed"], false);

    let published = std::fs::read(&output_path).unwrap();
    let published_report: serde_json::Value = serde_json::from_slice(&published).unwrap();
    assert_eq!(published_report["schema_kind"], "disksage.maven-cache-audit/v1");
    assert_eq!(published_report["repository_root"], repository_text);
    assert_eq!(published_report["provider_write_executed"], false);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&output_path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    let before = published;
    let repeated = run(&[
        "--repository-root",
        repository_text.as_str(),
        "--output",
        output_text.as_str(),
    ]);
    assert_eq!(repeated.status.code(), Some(2));
    assert!(stderr(&repeated).contains("maven-cache-audit-output-create-failed"));
    assert_eq!(std::fs::read(&output_path).unwrap(), before);
}
