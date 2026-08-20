//! Provider OAuth CLI entrypoint with a successful terminal help contract.
//!
//! The full OAuth lifecycle implementation remains in the adjacent non-binary include. This thin
//! entrypoint consumes host arguments as `OsString`, rejects undecodable host input before the
//! legacy string parser can panic, and intercepts only a sole `--help` or `-h` request so help
//! exits successfully on stdout. Every domain action and other invalid request continues through
//! the existing fail-closed implementation unchanged.

#[cfg(not(coverage))]
use std::ffi::{OsStr, OsString};
#[cfg(not(coverage))]
use std::path::PathBuf;

mod implementation {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/provider_oauth_cli_impl.rs.inc"
    ));

    #[cfg(not(coverage))]
    pub(super) fn usage_text() -> String {
        usage()
    }

    #[cfg(not(coverage))]
    pub(super) fn invoke_main() {
        main();
    }
}

#[cfg(not(coverage))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TerminalParse {
    Help,
    Run,
}

#[cfg(not(coverage))]
fn is_help_argument(value: &OsStr) -> bool {
    value == OsStr::new("--help") || value == OsStr::new("-h")
}

#[cfg(not(coverage))]
fn parse_terminal_args(
    args: &[OsString],
    _environment_home: Option<PathBuf>,
) -> Result<TerminalParse, String> {
    if args.iter().any(|value| value.to_str().is_none()) {
        return Err("argument-encoding-invalid".into());
    }
    match args {
        [flag] if is_help_argument(flag) => Ok(TerminalParse::Help),
        values if values.iter().any(|value| is_help_argument(value)) => {
            Err("help must be used alone".into())
        }
        _ => Ok(TerminalParse::Run),
    }
}

#[cfg(not(coverage))]
fn main() {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    match parse_terminal_args(&args, std::env::var_os("HOME").map(PathBuf::from)) {
        Ok(TerminalParse::Help) => println!("{}", implementation::usage_text()),
        Ok(TerminalParse::Run) => implementation::invoke_main(),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}

#[cfg(coverage)]
fn main() {}
