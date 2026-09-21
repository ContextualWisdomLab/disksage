//! Fail-closed `cargo clean` for a project `target/` with measured reclaim bytes.
//!
//! Help is terminal discovery: a sole `--help`/`-h` succeeds without invoking cargo.
//! Help combined with any other argument is a bounded failure and must not clean.

use std::ffi::OsString;

use disksage_lib::cargo_target_reclaim::{clean_cargo_target, ledger_reclaim_bytes};
use std::path::PathBuf;
use std::process::ExitCode;

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

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    match parse_args(&args) {
        Ok(ParseOutcome::Help) => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Ok(ParseOutcome::Run(project_dir)) => match clean_cargo_target(&project_dir) {
            Ok(result) => {
                let reclaim = ledger_reclaim_bytes(&result);
                println!(
                    "{{\n  \"cargo_path\": {:?},\n  \"project_dir\": {:?},\n  \"target_dir\": {:?},\n  \"bytes_before\": {},\n  \"bytes_after\": {},\n  \"observed_reduction_bytes\": {},\n  \"ledger_reclaim_bytes\": {},\n  \"status_code\": {},\n  \"executed\": {}\n}}",
                    result.cargo_path,
                    result.project_dir,
                    result.target_dir,
                    result.bytes_before,
                    result.bytes_after,
                    result.observed_reduction_bytes,
                    reclaim,
                    result.status_code,
                    result.executed
                );
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("DiskSage cargo-target-clean: {error}");
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
}
