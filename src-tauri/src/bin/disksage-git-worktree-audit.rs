//! Read-only Git worktree safety audit with a path-redacted public summary.

use std::ffi::OsString;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(test)]
use disksage_lib::git_worktree::MAX_REFERENCE_BYTES;
use disksage_lib::git_worktree::{
    audit_git_worktrees, public_summary, validate_reference, GitWorktreeAuditOptions,
};
use disksage_lib::private_evidence::{write_private_json_create_new, PrivateEvidenceReceipt};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    repository_root: PathBuf,
    retention_references: Vec<String>,
    private_output: Option<PathBuf>,
    options: GitWorktreeAuditOptions,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ParseOutcome {
    Run(Args),
    Help,
}

fn usage() -> &'static str {
    "usage: disksage-git-worktree-audit --repository-root ABSOLUTE_PATH --reference-ref REF [--reference-ref REF ...] [--private-output NEW_ABSOLUTE_JSON_PATH] [--command-timeout-ms N] [--size-scan-timeout-ms N] [--max-worktrees N] [--max-entries-per-worktree N] [--max-active-pids N]"
}

fn value(args: &[OsString], index: &mut usize, flag: &str) -> Result<OsString, String> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| format!("{flag} 값이 필요함"))
}

fn utf8_value(args: &[OsString], index: &mut usize, flag: &str) -> Result<String, String> {
    value(args, index, flag)?
        .into_string()
        .map_err(|_| "invalid-argument-encoding".to_string())
}

fn parse_number<T: std::str::FromStr>(
    args: &[OsString],
    index: &mut usize,
    flag: &str,
) -> Result<T, String> {
    utf8_value(args, index, flag)?
        .parse()
        .map_err(|_| format!("{flag}는 올바른 정수여야 함"))
}

fn mark_singleton(seen: &mut bool) -> Result<(), String> {
    if std::mem::replace(seen, true) {
        return Err("duplicate-option".into());
    }
    Ok(())
}

fn parse_args(args: &[OsString]) -> Result<ParseOutcome, String> {
    if args.len() == 1 && matches!(args[0].to_str(), Some("--help") | Some("-h")) {
        return Ok(ParseOutcome::Help);
    }

    let mut repository_root = None;
    let mut retention_references = Vec::new();
    let mut private_output = None;
    let mut options = GitWorktreeAuditOptions::default();
    let mut seen_repository_root = false;
    let mut seen_private_output = false;
    let mut seen_command_timeout = false;
    let mut seen_size_scan_timeout = false;
    let mut seen_max_worktrees = false;
    let mut seen_max_entries = false;
    let mut seen_max_active_pids = false;
    let mut index = 0usize;
    while index < args.len() {
        let flag = args[index]
            .to_str()
            .ok_or_else(|| "invalid-argument-encoding".to_string())?;
        match flag {
            "--repository-root" => {
                mark_singleton(&mut seen_repository_root)?;
                repository_root = Some(PathBuf::from(value(
                    args,
                    &mut index,
                    "--repository-root",
                )?));
            }
            "--reference-ref" => {
                let reference = utf8_value(args, &mut index, "--reference-ref")?;
                validate_reference(&reference)?;
                retention_references.push(reference);
            }
            "--private-output" => {
                mark_singleton(&mut seen_private_output)?;
                private_output = Some(PathBuf::from(value(
                    args,
                    &mut index,
                    "--private-output",
                )?));
            }
            "--command-timeout-ms" => {
                mark_singleton(&mut seen_command_timeout)?;
                options.command_timeout_ms =
                    parse_number(args, &mut index, "--command-timeout-ms")?;
            }
            "--size-scan-timeout-ms" => {
                mark_singleton(&mut seen_size_scan_timeout)?;
                options.size_scan_timeout_ms =
                    parse_number(args, &mut index, "--size-scan-timeout-ms")?;
            }
            "--max-worktrees" => {
                mark_singleton(&mut seen_max_worktrees)?;
                options.max_worktrees = parse_number(args, &mut index, "--max-worktrees")?;
            }
            "--max-entries-per-worktree" => {
                mark_singleton(&mut seen_max_entries)?;
                options.max_entries_per_worktree =
                    parse_number(args, &mut index, "--max-entries-per-worktree")?;
            }
            "--max-active-pids" => {
                mark_singleton(&mut seen_max_active_pids)?;
                options.max_active_pids = parse_number(args, &mut index, "--max-active-pids")?;
            }
            "--help" | "-h" => return Err("help-cannot-be-combined-with-runtime-input".into()),
            _ => return Err("unknown-argument".into()),
        }
        index += 1;
    }
    let repository_root =
        repository_root.ok_or_else(|| "--repository-root 값이 필요함".to_string())?;
    if !repository_root.is_absolute() {
        return Err("--repository-root는 절대 경로여야 함".into());
    }
    if private_output
        .as_ref()
        .is_some_and(|path| !path.is_absolute())
    {
        return Err("--private-output은 절대 경로여야 함".into());
    }
    if retention_references.is_empty() {
        return Err("--reference-ref 값이 하나 이상 필요함".into());
    }
    Ok(ParseOutcome::Run(Args {
        repository_root,
        retention_references,
        private_output,
        options,
    }))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .unwrap_or(0)
}

fn map_private_output_error(error: String) -> String {
    match error.as_str() {
        "private-evidence-secure-mode-unsupported" => {
            "git-worktree-private-output-secure-mode-unsupported".into()
        }
        "private-evidence-parent-writable-by-others" => {
            "git-worktree-private-output-parent-writable-by-others".into()
        }
        "private-evidence-inside-source-root" => {
            "git-worktree-private-output-inside-repository".into()
        }
        "private-evidence-create-failed" => "git-worktree-private-output-create-failed".into(),
        "private-evidence-write-failed"
        | "private-evidence-parent-sync-failed"
        | "private-evidence-mode-invalid" => "git-worktree-private-output-write-failed".into(),
        "private-evidence-parent-missing"
        | "private-evidence-parent-unavailable"
        | "private-evidence-parent-unsafe"
        | "private-evidence-source-root-unavailable"
        | "private-evidence-name-invalid" => "git-worktree-private-output-parent-unsafe".into(),
        "private-evidence-json-invalid" | "private-evidence-too-large" => {
            "git-worktree-private-output-invalid".into()
        }
        _ => "git-worktree-private-output-failed".into(),
    }
}

fn write_private_report(
    source_root: &PathBuf,
    path: &PathBuf,
    report: &impl serde::Serialize,
) -> Result<PrivateEvidenceReceipt, String> {
    write_private_json_create_new(source_root, path, report).map_err(map_private_output_error)
}

fn run_with_args(args: Args, observed_at_ms: u64) -> Result<serde_json::Value, String> {
    let report = audit_git_worktrees(
        &args.repository_root,
        &args.retention_references,
        args.options,
        observed_at_ms,
    )?;
    let mut summary =
        serde_json::to_value(public_summary(&report)).map_err(|error| error.to_string())?;
    if let Some(private_output) = &args.private_output {
        let receipt = write_private_report(&args.repository_root, private_output, &report)?;
        summary
            .as_object_mut()
            .ok_or_else(|| "git-worktree-public-summary-not-object".to_string())?
            .insert(
                "private_report".into(),
                serde_json::json!({
                    "written": receipt.written,
                    "bytes": receipt.bytes,
                    "sha256": receipt.sha256,
                    "unix_mode": receipt.unix_mode,
                    "create_new": receipt.create_new,
                    "contains_sensitive_local_paths_and_branches": receipt.contains_sensitive_local_paths,
                    "is_approval": receipt.is_approval,
                }),
            );
    }
    Ok(summary)
}

fn run() -> Result<(), String> {
    let raw: Vec<OsString> = std::env::args_os().skip(1).collect();
    let args = match parse_args(&raw)? {
        ParseOutcome::Help => {
            println!("{}", usage());
            return Ok(());
        }
        ParseOutcome::Run(args) => args,
    };
    let output = run_with_args(args, now_ms())?;
    println!(
        "{}",
        serde_json::to_string_pretty(&output).map_err(|error| error.to_string())?
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("DiskSage Git worktree audit: {error}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_args(values: &[&str]) -> Args {
        let raw: Vec<OsString> = values
            .iter()
            .map(|value| OsString::from(*value))
            .collect();
        match parse_args(&raw).unwrap() {
            ParseOutcome::Run(args) => args,
            ParseOutcome::Help => panic!("runtime arguments must not parse as help"),
        }
    }

    #[test]
    fn parser_requires_absolute_root_and_reference_and_defaults_read_only() {
        let args = run_args(&[
            "--repository-root",
            "/tmp/repository",
            "--reference-ref",
            "origin/develop",
        ]);
        assert_eq!(args.retention_references, vec!["origin/develop"]);
        assert_eq!(args.options, GitWorktreeAuditOptions::default());
        assert!(args.private_output.is_none());

        assert!(parse_args(&[]).is_err());
        let relative: Vec<OsString> = [
            "--repository-root",
            "relative",
            "--reference-ref",
            "develop",
            "--reference-ref",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ]
        .into_iter()
        .map(OsString::from)
        .collect();
        assert!(parse_args(&relative).is_err());
        let relative_output: Vec<OsString> = [
            "--repository-root",
            "/tmp/repository",
            "--reference-ref",
            "develop",
            "--reference-ref",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "--private-output",
            "relative.json",
        ]
        .into_iter()
        .map(OsString::from)
        .collect();
        assert!(parse_args(&relative_output).is_err());
    }

    #[test]
    fn parser_accepts_all_bounded_audit_options() {
        let args = run_args(&[
            "--repository-root",
            "/tmp/repository",
            "--reference-ref",
            "develop",
            "--reference-ref",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "--private-output",
            "/tmp/private.json",
            "--command-timeout-ms",
            "2000",
            "--size-scan-timeout-ms",
            "3000",
            "--max-worktrees",
            "10",
            "--max-entries-per-worktree",
            "1000",
            "--max-active-pids",
            "8",
        ]);
        assert_eq!(args.options.command_timeout_ms, 2000);
        assert_eq!(args.options.size_scan_timeout_ms, 3000);
        assert_eq!(args.options.max_worktrees, 10);
        assert_eq!(args.options.max_entries_per_worktree, 1000);
        assert_eq!(args.options.max_active_pids, 8);
        assert_eq!(args.retention_references.len(), 2);
    }

    #[test]
    fn parser_separates_terminal_help_from_invalid_mixed_help() {
        for flag in ["--help", "-h"] {
            let raw = vec![OsString::from(flag)];
            assert_eq!(parse_args(&raw).unwrap(), ParseOutcome::Help);
        }
        let mixed = vec![
            OsString::from("--help"),
            OsString::from("--repository-root"),
            OsString::from("/tmp/repository"),
        ];
        assert_eq!(
            parse_args(&mixed).unwrap_err(),
            "help-cannot-be-combined-with-runtime-input"
        );
    }

    #[cfg(unix)]
    #[test]
    fn parser_preserves_native_repository_paths_without_utf8_conversion() {
        use std::os::unix::ffi::{OsStrExt, OsStringExt};

        let native_path = OsString::from_vec(vec![b'/', b't', b'm', b'p', b'/', b'r', 0xff]);
        let raw = vec![
            OsString::from("--repository-root"),
            native_path.clone(),
            OsString::from("--reference-ref"),
            OsString::from("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        ];
        let ParseOutcome::Run(args) = parse_args(&raw).unwrap() else {
            panic!("native path invocation must parse as a runtime request")
        };
        assert_eq!(
            args.repository_root.as_os_str().as_bytes(),
            native_path.as_os_str().as_bytes()
        );
    }

    #[test]
    fn reference_admission_matches_the_library_contract_before_domain_work() {
        assert!(validate_reference("HEAD").is_ok());
        assert!(validate_reference("a/b").is_ok());
        assert!(validate_reference(&"a".repeat(MAX_REFERENCE_BYTES)).is_ok());
        for invalid in [
            String::new(),
            "-dangerous-option".into(),
            "control\nreference".into(),
            "a".repeat(MAX_REFERENCE_BYTES + 1),
        ] {
            assert_eq!(
                validate_reference(&invalid).unwrap_err(),
                "git-worktree-reference-invalid"
            );
        }
    }

    #[test]
    fn private_output_error_mapping_remains_cli_bounded() {
        assert_eq!(
            map_private_output_error("private-evidence-secure-mode-unsupported".into()),
            "git-worktree-private-output-secure-mode-unsupported"
        );
        assert_eq!(
            map_private_output_error("private-evidence-parent-writable-by-others".into()),
            "git-worktree-private-output-parent-writable-by-others"
        );
        assert_eq!(
            map_private_output_error("private-evidence-inside-source-root".into()),
            "git-worktree-private-output-inside-repository"
        );
        assert_eq!(
            map_private_output_error("private-evidence-create-failed".into()),
            "git-worktree-private-output-create-failed"
        );
    }

    #[cfg(unix)]
    #[test]
    fn private_output_reuses_repository_private_evidence_boundary() {
        let source = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        let output = private.path().join("audit.json");
        let receipt = write_private_report(
            &source.path().to_path_buf(),
            &output,
            &serde_json::json!({"private": true}),
        )
        .unwrap();
        assert!(receipt.written);
        assert_eq!(receipt.unix_mode, "0600");
        assert!(receipt.contains_sensitive_local_paths);
        assert!(write_private_report(
            &source.path().to_path_buf(),
            &output,
            &serde_json::json!({"private": false}),
        )
        .is_err());
    }
}
