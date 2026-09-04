//! Native host adapter for the shipped provider OAuth CLI.
//!
//! The domain CLI implementation remains in `disksage-provider-oauth.rs`. This adapter owns only
//! process-host concerns: lossless argument admission, terminal help semantics, and platform home
//! discovery before delegating to the existing fail-closed parser and executor.

#[cfg(not(coverage))]
use std::ffi::{OsStr, OsString};
#[cfg(not(coverage))]
use std::path::PathBuf;

#[cfg(not(coverage))]
#[path = "../home_resolution.rs"]
mod home_resolution;

#[cfg(not(coverage))]
mod implementation {
    include!("disksage-provider-oauth.rs");

    pub(super) fn usage_text() -> String {
        usage()
    }

    pub(super) fn run_with_environment(
        args: Vec<String>,
        environment_home: Option<PathBuf>,
    ) -> Result<(), String> {
        let parsed = parse_args(&args, environment_home)?;
        let output = execute(parsed)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&output)
                .map_err(|_| "provider-oauth-output-serialization-failed".to_string())?
        );
        Ok(())
    }
}

#[cfg(not(coverage))]
#[derive(Debug, Clone, PartialEq, Eq)]
enum TerminalArgs {
    Help,
    Run(Vec<String>),
}

#[cfg(not(coverage))]
fn is_help(value: &OsStr) -> bool {
    value == OsStr::new("--help") || value == OsStr::new("-h")
}

#[cfg(not(coverage))]
fn parse_terminal_args(args: Vec<OsString>) -> Result<TerminalArgs, String> {
    if args.iter().any(|value| value.to_str().is_none()) {
        return Err("argument-encoding-invalid".into());
    }
    match args.as_slice() {
        [only] if is_help(only) => return Ok(TerminalArgs::Help),
        values if values.iter().any(|value| is_help(value)) => {
            return Err("help must be used alone".into());
        }
        _ => {}
    }
    Ok(TerminalArgs::Run(
        args.into_iter()
            .map(|value| {
                value
                    .into_string()
                    .expect("non-UTF-8 arguments were rejected before domain parsing")
            })
            .collect(),
    ))
}

#[cfg(all(not(coverage), windows))]
fn environment_home() -> Option<PathBuf> {
    home_resolution::select_absolute_home([
        std::env::var_os("HOME").map(PathBuf::from),
        std::env::var_os("USERPROFILE").map(PathBuf::from),
        home_resolution::windows_home_drive_path(),
    ])
    .ok()
}

#[cfg(all(not(coverage), not(windows)))]
fn environment_home() -> Option<PathBuf> {
    home_resolution::select_absolute_home([std::env::var_os("HOME").map(PathBuf::from)]).ok()
}

#[cfg(not(coverage))]
fn main() {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let result = match parse_terminal_args(args) {
        Ok(TerminalArgs::Help) => {
            println!("{}", implementation::usage_text());
            Ok(())
        }
        Ok(TerminalArgs::Run(args)) => implementation::run_with_environment(args, environment_home()),
        Err(error) => Err(error),
    };
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(coverage)]
fn main() {}

#[cfg(all(test, not(coverage)))]
mod tests {
    use super::*;

    #[test]
    fn terminal_parser_separates_help_from_domain_arguments() {
        assert_eq!(
            parse_terminal_args(vec![OsString::from("--help")]).unwrap(),
            TerminalArgs::Help
        );
        assert_eq!(
            parse_terminal_args(vec![OsString::from("--help"), OsString::from("--list")])
                .unwrap_err(),
            "help must be used alone"
        );
        assert_eq!(
            parse_terminal_args(vec![OsString::from("--list")]).unwrap(),
            TerminalArgs::Run(vec!["--list".to_string()])
        );
    }
}
