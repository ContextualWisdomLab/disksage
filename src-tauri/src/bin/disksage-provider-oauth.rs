//! Provider OAuth CLI entrypoint with a successful terminal help contract.
//!
//! The full OAuth lifecycle implementation remains in the adjacent implementation module. This
//! thin entrypoint intercepts only a sole `--help` or `-h` request so help exits successfully on
//! stdout; every domain action and invalid request continues through the existing fail-closed
//! implementation unchanged.

#[cfg(not(coverage))]
use std::path::PathBuf;

mod implementation {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/bin/disksage-provider-oauth-impl.rs"
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
fn parse_terminal_args(
    args: &[String],
    _environment_home: Option<PathBuf>,
) -> Result<TerminalParse, String> {
    match args {
        [flag] if flag == "--help" || flag == "-h" => Ok(TerminalParse::Help),
        values if values.iter().any(|value| value == "--help" || value == "-h") => {
            Err("help must be used alone".into())
        }
        _ => Ok(TerminalParse::Run),
    }
}

#[cfg(not(coverage))]
fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
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
