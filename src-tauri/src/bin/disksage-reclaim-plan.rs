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

fn parse_args(raw_args: impl IntoIterator<Item = OsString>) -> Result<Args, String> {
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
            Some("-h" | "--help") => {
                println!("{USAGE}");
                return Ok(Args {
                    operation,
                    pretty,
                    paths,
                });
            }
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

    Ok(Args {
        operation,
        pretty,
        paths,
    })
}

fn run() -> Result<(), String> {
    let args = parse_args(std::env::args_os().skip(1))?;
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

fn main() {
    if let Err(error) = run() {
        eprintln!("disksage-reclaim-plan: {error}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_options_and_preserves_path_arguments() {
        let parsed = parse_args([
            OsString::from("--operation"),
            OsString::from("delete"),
            OsString::from("--pretty"),
            OsString::from("/tmp/example"),
        ])
        .unwrap();

        assert_eq!(parsed.operation, PlannedOperation::Delete);
        assert!(parsed.pretty);
        assert_eq!(parsed.paths, [PathBuf::from("/tmp/example")]);
    }

    #[test]
    fn double_dash_preserves_option_like_paths() {
        let parsed = parse_args([
            OsString::from("--"),
            OsString::from("--not-an-option"),
        ])
        .unwrap();

        assert_eq!(parsed.paths, [PathBuf::from("--not-an-option")]);
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_path_is_preserved_for_fail_closed_plan_validation() {
        use std::os::unix::ffi::OsStringExt;

        let path = OsString::from_vec(vec![b'/', b't', b'm', b'p', b'/', 0x80]);
        let parsed = parse_args([path.clone()]).unwrap();

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