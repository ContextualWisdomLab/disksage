//! Command-line entry point for read-only Podman reclaim evidence.
//!
//! The command accepts only bounded operational options, preserves a native filesystem path for
//! the Podman executable, and never starts, stops, removes, prunes, or trims Podman resources.

use disksage_lib::podman_reclaim::{
    probe_podman_reclaim, DEFAULT_PODMAN_MACHINE, DEFAULT_PROBE_TIMEOUT,
};
use std::path::PathBuf;
use std::time::Duration;

const USAGE: &str = "Usage: disksage-podman-reclaim-plan [--machine NAME] [--podman-bin PATH] [--timeout-seconds N] [--pretty]\n\
Builds read-only Podman guest/raw allocation evidence. It never prunes, removes, trims, or stops anything.";

/// Returns the next required argument as UTF-8 without reflecting malformed input.
fn next_utf8_argument(
    args: &mut impl Iterator<Item = std::ffi::OsString>,
    missing_message: &str,
    invalid_message: &str,
) -> Result<String, String> {
    args.next()
        .ok_or_else(|| missing_message.to_string())?
        .into_string()
        .map_err(|_| invalid_message.to_string())
}

/// Parses the process argument stream and prints one read-only Podman evidence document.
fn run() -> Result<(), String> {
    let raw_args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    if raw_args.len() == 1 && matches!(raw_args[0].to_str(), Some("-h") | Some("--help")) {
        println!("{USAGE}");
        return Ok(());
    }

    let mut machine = DEFAULT_PODMAN_MACHINE.to_string();
    let mut machine_seen = false;
    let mut podman_bin = PathBuf::from("podman");
    let mut podman_bin_seen = false;
    let mut timeout = DEFAULT_PROBE_TIMEOUT;
    let mut timeout_seen = false;
    let mut pretty = false;
    let mut pretty_seen = false;
    let mut args = raw_args.into_iter();
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--machine") => {
                if machine_seen {
                    return Err("--machine may be supplied once".to_string());
                }
                machine_seen = true;
                machine = next_utf8_argument(
                    &mut args,
                    "--machine requires a name",
                    "--machine requires a UTF-8 name",
                )?;
            }
            Some("--podman-bin") => {
                if podman_bin_seen {
                    return Err("--podman-bin may be supplied once".to_string());
                }
                podman_bin_seen = true;
                podman_bin = PathBuf::from(
                    args.next()
                        .ok_or_else(|| "--podman-bin requires a path".to_string())?,
                );
            }
            Some("--timeout-seconds") => {
                if timeout_seen {
                    return Err("--timeout-seconds may be supplied once".to_string());
                }
                timeout_seen = true;
                let seconds = next_utf8_argument(
                    &mut args,
                    "--timeout-seconds requires an integer",
                    "--timeout-seconds requires a UTF-8 integer",
                )?
                .parse::<u64>()
                .map_err(|_| "--timeout-seconds requires an integer".to_string())?;
                if !(1..=60).contains(&seconds) {
                    return Err("--timeout-seconds must be between 1 and 60".to_string());
                }
                timeout = Duration::from_secs(seconds);
            }
            Some("--pretty") => {
                if pretty_seen {
                    return Err("--pretty may be supplied once".to_string());
                }
                pretty_seen = true;
                pretty = true;
            }
            Some("-h" | "--help") => return Err(format!("help must be used alone\n{USAGE}")),
            Some(_) => return Err(format!("unknown option\n{USAGE}")),
            None => return Err(format!("unknown option (non-UTF-8)\n{USAGE}")),
        }
    }

    let plan = probe_podman_reclaim(&podman_bin, &machine, timeout);
    let json = if pretty {
        serde_json::to_string_pretty(&plan)
    } else {
        serde_json::to_string(&plan)
    }
    .map_err(|error| error.to_string())?;
    println!("{json}");
    Ok(())
}

/// Runs the CLI and reports bounded argument or evidence failures with exit code 2.
fn main() {
    if let Err(error) = run() {
        eprintln!("disksage-podman-reclaim-plan: {error}");
        std::process::exit(2);
    }
}
