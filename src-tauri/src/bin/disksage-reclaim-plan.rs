use disksage_lib::reclaim::{plan_reclaim, PlannedOperation};
use std::ffi::OsString;
use std::path::PathBuf;

const USAGE: &str = "Usage: disksage-reclaim-plan [--operation trash|delete] [--pretty] PATH...\n\
Builds read-only logical/allocation evidence. It never moves or deletes files.";

#[derive(Debug, PartialEq, Eq)]
struct Args {
    operation: PlannedOperation,
    pretty: bool,
    paths: Vec<PathBuf>,
}

#[derive(Debug, PartialEq, Eq)]
enum ParseResult {
    Run(Args),
    Help,
}

fn parse_args(raw_args: impl IntoIterator<Item = OsString>) -> Result<ParseResult, String> {
    let mut operation = PlannedOperation::Trash;
    let mut pretty = false;
    let mut paths = Vec::new();
    let mut args = raw_args.into_iter();

    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--operation") => {
                let value = args
                    .next()
                    .ok_or_else(|| "--operation requires trash or delete".to_string())?;
                let value = value.to_str().ok_or_else(|| {
                    "--operation requires a valid UTF-8 value: trash or delete".to_string()
                })?;
                operation = value.parse()?;
            }
            Some("--pretty") => pretty = true,
            Some("-h" | "--help") => return Ok(ParseResult::Help),
            Some("--") => {
                paths.extend(args.map(PathBuf::from));
                break;
            }
            Some(value) if value.starts_with('-') => {
                return Err(format!("unknown option: {value}\n{USAGE}"));
            }
            _ => paths.push(PathBuf::from(arg)),
        }
    }

    Ok(ParseResult::Run(Args {
        operation,
        pretty,
        paths,
    }))
}

fn run_with_args(raw_args: impl IntoIterator<Item = OsString>) -> Result<(), String> {
    let args = match parse_args(raw_args)? {
        ParseResult::Help => {
            println!("{USAGE}");
            return Ok(());
        }
        ParseResult::Run(args) => args,
    };
    let plan = plan_reclaim(&args.paths, args.operation)?;
    let json = if args.pretty {
        serde_json::to_string_pretty(&plan)
    } else {
        serde_json::to_string(&plan)
    }
    .map_err(|error| error.to_string())?;
    println!("{json}");
    Ok(())
}

fn run() -> Result<(), String> {
    run_with_args(std::env::args_os().skip(1))
}

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
                OsString::from("/tmp/example"),
            ])
            .unwrap(),
        );

        assert_eq!(parsed.operation, PlannedOperation::Delete);
        assert!(parsed.pretty);
        assert_eq!(parsed.paths, [PathBuf::from("/tmp/example")]);
    }

    #[test]
    fn double_dash_preserves_option_like_paths() {
        let parsed = expect_run(
            parse_args([
                OsString::from("--"),
                OsString::from("--not-an-option"),
            ])
            .unwrap(),
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