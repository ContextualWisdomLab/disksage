//! Fail-closed `cargo clean` for a project `target/` with measured reclaim bytes.
//!
//! Help is terminal discovery: a sole `--help`/`-h` succeeds without invoking cargo.
//! Help combined with any other argument is a bounded failure and must not clean.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use disksage_lib::cargo_target_reclaim::{clean_cargo_target, ledger_reclaim_bytes, CargoTargetCleanResult};
use serde_json::json;

const USAGE: &str = "Usage: disksage-cargo-target-clean --project-dir ABSOLUTE_PATH\n\
Runs cargo clean with an absolute cargo executable. Exit 2 on tool/spawn/nonzero failure.\n\
Prints JSON with bytes_before/after and observed_reduction_bytes (0 when unchanged).";

#[derive(Debug, PartialEq, Eq)]
enum ParseOutcome {
    Help,
    Run(PathBuf),
}

fn parse_args(args: &[OsString]) -> Result<ParseOutcome, String> {
    if args.len() == 1 && (args[0] == "-h" || args[0] == "--help") {
        return Ok(ParseOutcome::Help);
    }
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        return Err("help-cannot-be-combined-with-runtime-input".into());
    }

    let mut project_dir: Option<PathBuf> = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].to_str() {
            Some("--project-dir") => {
                let Some(value) = args.get(index + 1) else {
                    return Err("--project-dir requires PATH".into());
                };
                index += 1;
                if project_dir.replace(PathBuf::from(value)).is_some() {
                    return Err("--project-dir may be supplied once".into());
                }
            }
            Some(other) => {
                return Err(format!("unknown option: {other}\n{USAGE}"));
            }
            None => return Err(format!("invalid UTF-8 option\n{USAGE}")),
        }
        index += 1;
    }
    project_dir
        .ok_or_else(|| format!("--project-dir is required\n{USAGE}"))
        .map(ParseOutcome::Run)
}

fn utf8_path<'a>(label: &str, path: &'a Path) -> Result<&'a str, String> {
    path.to_str()
        .ok_or_else(|| format!("cargo-target-json-non-utf8-path:{label}"))
}

fn serialize_result(result: &CargoTargetCleanResult) -> Result<String, String> {
    let cargo_path = utf8_path("cargo_path", &result.cargo_path)?;
    let project_dir = utf8_path("project_dir", &result.project_dir)?;
    let target_dir = utf8_path("target_dir", &result.target_dir)?;
    let reclaim = ledger_reclaim_bytes(result);

    serde_json::to_string_pretty(&json!({
        "cargo_path": cargo_path,
        "project_dir": project_dir,
        "target_dir": target_dir,
        "bytes_before": result.bytes_before,
        "bytes_after": result.bytes_after,
        "observed_reduction_bytes": result.observed_reduction_bytes,
        "ledger_reclaim_bytes": reclaim,
        "status_code": result.status_code,
        "executed": result.executed,
    }))
    .map_err(|error| format!("cargo-target-json-serialize-failed:{error}"))
}

/// Returns whether an owner error is the complete versioned partial-clean receipt contract.
fn is_partial_clean_receipt(error: &str) -> bool {
    let Ok(receipt) = serde_json::from_str::<serde_json::Value>(error) else {
        return false;
    };
    receipt.get("schema_version").and_then(serde_json::Value::as_u64) == Some(1)
        && receipt.get("code").and_then(serde_json::Value::as_str)
            == Some("cargo-target-partial-clean-failed")
        && receipt.get("completion").and_then(serde_json::Value::as_str) == Some("partial")
        && receipt
            .get("entries_removed")
            .and_then(serde_json::Value::as_u64)
            .is_some_and(|count| count > 0)
        && receipt
            .get("cause")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|cause| !cause.is_empty())
}

/// Preserves validated partial-clean receipts as pure JSON while retaining legacy text errors.
fn render_clean_error(error: &str) -> String {
    if is_partial_clean_receipt(error) {
        error.to_owned()
    } else {
        format!("DiskSage cargo-target-clean: {error}")
    }
}

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    match parse_args(&args) {
        Ok(ParseOutcome::Help) => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Ok(ParseOutcome::Run(project_dir)) => match clean_cargo_target(&project_dir) {
            Ok(result) => match serialize_result(&result) {
                Ok(json) => {
                    println!("{json}");
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    eprintln!("DiskSage cargo-target-clean: {error}");
                    ExitCode::from(2)
                }
            },
            Err(error) => {
                eprintln!("{}", render_clean_error(&error));
                ExitCode::from(2)
            }
        },
        Err(error) => {
            eprintln!("DiskSage cargo-target-clean: {error}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(values: &[&str]) -> Vec<OsString> {
        values.iter().map(|value| OsString::from(*value)).collect()
    }

    #[test]
    fn sole_help_flags_parse_as_terminal_help() {
        for flag in ["--help", "-h"] {
            assert_eq!(parse_args(&os(&[flag])).unwrap(), ParseOutcome::Help);
        }
    }

    #[test]
    fn help_mixed_with_project_dir_is_bounded_and_does_not_keep_the_path() {
        for args in [
            vec!["--help", "--project-dir", "/private/customer/project"],
            vec!["--project-dir", "/private/customer/project", "-h"],
        ] {
            let err = parse_args(&os(&args)).unwrap_err();
            assert_eq!(err, "help-cannot-be-combined-with-runtime-input");
            assert!(!err.contains("/private/customer/project"));
        }
    }

    #[test]
    fn missing_project_dir_does_not_parse_as_help() {
        let err = parse_args(&[]).unwrap_err();
        assert!(err.starts_with("--project-dir is required"));
    }

    #[test]
    fn result_output_is_valid_json_with_escaped_paths_and_zero_unproven_credit() {
        let result = CargoTargetCleanResult {
            cargo_path: PathBuf::from("/tmp/cargo\"quoted"),
            project_dir: PathBuf::from("/tmp/project\\segment"),
            target_dir: PathBuf::from("/tmp/project\\segment/target"),
            bytes_before: 5,
            bytes_after: 2,
            observed_reduction_bytes: 3,
            status_code: 0,
            executed: true,
        };
        let output = serialize_result(&result).expect("serialize result");
        let parsed: serde_json::Value = serde_json::from_str(&output).expect("valid JSON");
        assert_eq!(parsed["observed_reduction_bytes"], 3);
        assert_eq!(parsed["ledger_reclaim_bytes"], 0);
        assert_eq!(parsed["cargo_path"], "/tmp/cargo\"quoted");
    }

    #[test]
    fn structured_partial_clean_receipt_remains_pure_json_at_buyer_boundary() {
        let receipt = serde_json::json!({
            "schema_version": 1,
            "code": "cargo-target-partial-clean-failed",
            "completion": "partial",
            "entries_removed": 1,
            "cause": "cargo-target-capability-unlinkat-failed:Permission denied",
        })
        .to_string();

        let rendered = render_clean_error(&receipt);
        let parsed: serde_json::Value = serde_json::from_str(&rendered)
            .expect("buyer stderr contract must remain machine-readable JSON");
        assert_eq!(parsed["code"], "cargo-target-partial-clean-failed");
        assert_eq!(parsed["completion"], "partial");
        assert_eq!(parsed["entries_removed"], 1);
    }

    #[test]
    fn incomplete_partial_receipt_keeps_the_human_readable_prefix() {
        let incomplete = r#"{"schema_version":1,"code":"cargo-target-partial-clean-failed","completion":"partial"}"#;
        assert_eq!(
            render_clean_error(incomplete),
            format!("DiskSage cargo-target-clean: {incomplete}")
        );
    }

    #[test]
    fn ordinary_clean_errors_keep_the_human_readable_prefix() {
        assert_eq!(
            render_clean_error("cargo-target-lsof-unavailable"),
            "DiskSage cargo-target-clean: cargo-target-lsof-unavailable"
        );
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_path_fails_serialization_explicitly() {
        use std::os::unix::ffi::OsStringExt;

        let result = CargoTargetCleanResult {
            cargo_path: PathBuf::from(OsString::from_vec(vec![b'/', b't', b'm', b'p', b'/', 0xff])),
            project_dir: PathBuf::from("/tmp/project"),
            target_dir: PathBuf::from("/tmp/project/target"),
            bytes_before: 0,
            bytes_after: 0,
            observed_reduction_bytes: 0,
            status_code: 0,
            executed: true,
        };
        assert_eq!(
            serialize_result(&result).unwrap_err(),
            "cargo-target-json-non-utf8-path:cargo_path"
        );
    }
}