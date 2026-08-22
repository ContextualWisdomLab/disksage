//! Command-line entry point for read-only reclaim evidence planning.
//!
//! The parser preserves operating-system paths without forcing Unicode conversion. The command
//! produces local evidence only and never moves, deletes, or otherwise mutates supplied paths.

use disksage_lib::reclaim::{plan_reclaim_with_options, PlannedOperation, ReclaimPlanOptions};
use std::ffi::OsString;
use std::path::PathBuf;

const USAGE: &str = "Usage: disksage-reclaim-plan [--operation trash|delete] [--pretty] [--check-active-use] PATH...\n\
Builds read-only logical/allocation evidence. It never moves or deletes files.";

/// Parsed arguments for one reclaim-plan execution.
#[derive(Debug, PartialEq, Eq)]
struct Args {
    /// Destructive lifecycle whose potential consequences are being described.
    operation: PlannedOperation,
    /// Whether the JSON result should use human-readable indentation.
    pretty: bool,
    /// Whether to include bounded process/file-use evidence for each root.
    check_active_use: bool,
    /// Filesystem roots to inspect without mutation.
    paths: Vec<PathBuf>,
}

/// Terminal result of command-line parsing.
#[derive(Debug, PartialEq, Eq)]
enum ParseResult {
    /// Continue with a normal evidence-planning execution.
    Run(Args),
    /// Print usage and exit successfully without scanning any path.
    Help,
}

/// Detect option-shaped native arguments without requiring them to be valid Unicode.
///
/// Lossy conversion is used only for the leading `-` classification and is never reflected in a
/// diagnostic. This keeps invalid-Unicode option tokens fail-closed on Windows as well as Unix,
/// while non-option native paths remain available to the planner unchanged.
fn native_argument_is_option_like(argument: &OsString) -> bool {
    argument.to_string_lossy().starts_with('-')
}

/// Parses bounded options while preserving non-option values as native operating-system strings.
fn parse_args(raw_args: impl IntoIterator<Item = OsString>) -> Result<ParseResult, String> {
    let raw_args: Vec<OsString> = raw_args.into_iter().collect();
    if raw_args.len() == 1
        && matches!(raw_args[0].to_str(), Some("-h") | Some("--help"))
    {
        return Ok(ParseResult::Help);
    }

    let mut operation = PlannedOperation::Trash;
    let mut operation_seen = false;
    let mut pretty = false;
    let mut pretty_seen = false;
    let mut check_active_use = false;
    let mut check_active_use_seen = false;
    let mut paths = Vec::new();
    let mut args = raw_args.into_iter();

    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--operation") => {
                if operation_seen {
                    return Err("--operation may be supplied once".to_string());
                }
                operation_seen = true;
                let value = args
                    .next()
                    .ok_or_else(|| "--operation requires trash or delete".to_string())?;
                let value = value.to_str().ok_or_else(|| {
                    "--operation requires a valid UTF-8 value: trash or delete".to_string()
                })?;
                operation = value.parse()?;
            }
            Some("--pretty") => {
                if pretty_seen {
                    return Err("--pretty may be supplied once".to_string());
                }
                pretty_seen = true;
                pretty = true;
            }
            Some("--check-active-use") => {
                if check_active_use_seen {
                    return Err("--check-active-use may be supplied once".to_string());
                }
                check_active_use_seen = true;
                check_active_use = true;
            }
            Some("-h" | "--help") => return Err(format!("help must be used alone\n{USAGE}")),
            Some("--") => {
                paths.extend(args.map(PathBuf::from));
                break;
            }
            Some(value) if value.starts_with('-') => {
                return Err(format!("unknown option\n{USAGE}"));
            }
            None if native_argument_is_option_like(&arg) => {
                return Err(format!("unknown option\n{USAGE}"));
            }
            None => paths.push(PathBuf::from(arg)),
            Some(_) => paths.push(PathBuf::from(arg)),
        }
    }

    Ok(ParseResult::Run(Args {
        operation,
        pretty,
        check_active_use,
        paths,
    }))
}

/// Executes one parsed argument stream and writes either usage or a JSON evidence document.
fn run_with_args(raw_args: impl IntoIterator<Item = OsString>) -> Result<(), String> {
    let args = match parse_args(raw_args)? {
        ParseResult::Help => {
            println!("{USAGE}");
            return Ok(());
        }
        ParseResult::Run(args) => args,
    };
    let plan = plan_reclaim_with_options(
        &args.paths,
        args.operation,
        ReclaimPlanOptions {
            include_active_use: args.check_active_use,
        },
    )?;
    let json = if args.pretty {
        serde_json::to_string_pretty(&plan)
    } else {
        serde_json::to_string(&plan)
    }
    .map_err(|error| error.to_string())?;
    println!("{json}");
    Ok(())
}

/// Executes the process argument stream without lossy Unicode conversion.
fn run() -> Result<(), String> {
    run_with_args(std::env::args_os().skip(1))
}

/// Reports a stable error to stderr and uses exit code 2 for invalid requests.
fn main() {
    if let Err(error) = run() {
        eprintln!("disksage-reclaim-plan: {error}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expect_run(parsed: ParseResult) -> Args {
        match parsed {
            ParseResult::Run(args) => args,
            ParseResult::Help => panic!("expected runnable arguments"),
        }
    }

    #[test]
    fn parses_options_and_preserves_path_arguments() {
        let parsed = expect_run(
            parse_args([
                OsString::from("--operation"),
                OsString::from("delete"),
                OsString::from("--pretty"),
                OsString::from("--check-active-use"),
                OsString::from("/tmp/example"),
            ])
            .unwrap(),
        );

        assert_eq!(parsed.operation, PlannedOperation::Delete);
        assert!(parsed.pretty);
        assert!(parsed.check_active_use);
        assert_eq!(parsed.paths, [PathBuf::from("/tmp/example")]);
    }

    #[test]
    fn option_shape_classification_is_platform_independent() {
        assert!(native_argument_is_option_like(&OsString::from("--opaque")));
        assert!(!native_argument_is_option_like(&OsString::from("relative-path")));
    }

    #[test]
    fn double_dash_preserves_option_like_paths() {
        let parsed = expect_run(
            parse_args([OsString::from("--"), OsString::from("--not-an-option")]).unwrap(),
        );

        assert_eq!(parsed.paths, [PathBuf::from("--not-an-option")]);
    }

    #[test]
    fn help_is_a_successful_terminal_parse_result() {
        assert_eq!(
            parse_args([OsString::from("--help")]).unwrap(),
            ParseResult::Help
        );
        assert!(run_with_args([OsString::from("-h")]).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_path_is_preserved_for_fail_closed_plan_validation() {
        use std::os::unix::ffi::OsStringExt;

        let path = OsString::from_vec(vec![b'/', b't', b'm', b'p', b'/', 0x80]);
        let parsed = expect_run(parse_args([path.clone()]).unwrap());

        assert_eq!(parsed.paths, [PathBuf::from(path)]);
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_option_shaped_argument_is_rejected_without_becoming_a_path() {
        use std::os::unix::ffi::OsStringExt;

        let option = OsString::from_vec(vec![
            b'-', b'-', b'o', b'p', b'a', b'q', b'u', b'e', 0x80,
        ]);
        let error = parse_args([option]).unwrap_err();

        assert!(error.starts_with("unknown option"), "{error}");
    }

    #[cfg(windows)]
    #[test]
    fn invalid_unicode_option_shaped_argument_is_rejected_on_windows() {
        use std::os::windows::ffi::OsStringExt;

        let option = OsString::from_wide(&[
            b'-' as u16,
            b'-' as u16,
            b'o' as u16,
            b'p' as u16,
            b'a' as u16,
            b'q' as u16,
            b'u' as u16,
            b'e' as u16,
            0xD800,
        ]);
        assert!(option.to_str().is_none(), "fixture must be invalid Unicode");
        let error = parse_args([option]).unwrap_err();

        assert!(error.starts_with("unknown option"), "{error}");
        assert!(!error.contains("opaque"), "diagnostic must not reflect payload");
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_operation_value_is_rejected_without_panicking() {
        use std::os::unix::ffi::OsStringExt;

        let error = parse_args([
            OsString::from("--operation"),
            OsString::from_vec(vec![0x80]),
        ])
        .unwrap_err();

        assert!(error.contains("valid UTF-8"));
    }
}
