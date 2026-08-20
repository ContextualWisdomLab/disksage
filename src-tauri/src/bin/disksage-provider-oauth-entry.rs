//! Platform-aware entrypoint for the provider OAuth operational CLI.
//!
//! The domain implementation stays in `disksage-provider-oauth.rs`. This entry owns only host
//! argument decoding, terminal help, and platform home-directory selection before delegating to
//! the existing parser and OAuth execution boundary.

use std::path::PathBuf;

mod implementation {
    include!("disksage-provider-oauth.rs");

    #[cfg(not(coverage))]
    pub(super) fn usage_text() -> String {
        usage()
    }

    #[cfg(not(coverage))]
    pub(super) fn run_with_environment_home(
        args: &[String],
        environment_home: Option<PathBuf>,
    ) -> Result<(), String> {
        let parsed = parse_args(args, environment_home)?;
        let output = execute(parsed)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&output)
                .map_err(|_| "provider-oauth-output-serialization-failed".to_string())?
        );
        Ok(())
    }
}

/// Resolve the platform home authority supplied to the existing OAuth parser.
///
/// This initial extraction intentionally preserves the predecessor behavior: only `HOME` is used.
/// The function is pure so the Windows authority contract can be exercised on every CI host before
/// changing shipped behavior.
pub(crate) fn environment_home_from(
    home: Option<PathBuf>,
    user_profile: Option<PathBuf>,
    windows: bool,
) -> Option<PathBuf> {
    let _ = (user_profile, windows);
    home
}

#[cfg(not(coverage))]
fn command_line_args() -> Result<Vec<String>, String> {
    std::env::args_os()
        .skip(1)
        .map(|argument| {
            argument
                .into_string()
                .map_err(|_| "provider-oauth-invalid-utf8-argument".to_string())
        })
        .collect()
}

#[cfg(not(coverage))]
fn run() -> Result<(), String> {
    let args = command_line_args()?;
    if matches!(args.as_slice(), [flag] if flag == "--help" || flag == "-h") {
        println!("{}", implementation::usage_text());
        return Ok(());
    }

    let environment_home = environment_home_from(
        std::env::var_os("HOME").map(PathBuf::from),
        std::env::var_os("USERPROFILE").map(PathBuf::from),
        cfg!(windows),
    );
    implementation::run_with_environment_home(&args, environment_home)
}

#[cfg(not(coverage))]
fn main() {
    if let Err(error) = run() {
        let exit_code = if error == "provider-oauth-invalid-utf8-argument" {
            2
        } else {
            1
        };
        eprintln!("{error}");
        std::process::exit(exit_code);
    }
}

#[cfg(coverage)]
fn main() {}
