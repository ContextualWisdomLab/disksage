//! Command-line entry point for bounded local-volume snapshots and comparisons.
//!
//! The command reads one local volume plus an optional immutable baseline snapshot. It emits JSON
//! evidence only and never removes, truncates, or otherwise mutates filesystem content.

use disksage_lib::volume_pressure::{compare_snapshots, snapshot_volume, LocalVolumeSnapshot};
use std::ffi::OsString;
use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

/// Maximum accepted baseline JSON size, including the bounded-read recheck.
const MAX_BASELINE_BYTES: u64 = 64 * 1024;
const USAGE: &str = "Usage: disksage-volume-snapshot [--path PATH] \
[--baseline SNAPSHOT_JSON --logical-removed-bytes BYTES]";

/// Parsed arguments for one local-volume snapshot or comparison operation.
#[derive(Debug, Eq, PartialEq)]
struct Args {
    /// Local filesystem path whose allocation evidence will be observed.
    path: PathBuf,
    /// Optional prior snapshot used for a deterministic before/after comparison.
    baseline: Option<PathBuf>,
    /// Optional logical-removal claim paired with a supplied baseline.
    logical_removed_bytes: Option<u64>,
}

/// Runs the CLI and maps help, validation, and evidence outcomes to stable exit codes.
fn main() -> ExitCode {
    match run(std::env::args_os().skip(1)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) if error == "help" => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error:{error}");
            ExitCode::from(2)
        }
    }
}

/// Parses one argument stream, observes the current volume, and writes bounded JSON evidence.
fn run<I, S>(args: I) -> Result<(), String>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let args = parse_args(args)?;
    let current = snapshot_volume(&args.path, now_ms()?)?;
    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    match args.baseline {
        Some(path) => {
            let baseline = read_baseline(path)?;
            let comparison = compare_snapshots(&baseline, &current, args.logical_removed_bytes)?;
            serde_json::to_writer_pretty(&mut output, &comparison)
                .map_err(|_| "local-volume-output-encode-failed")?;
        }
        None => {
            serde_json::to_writer_pretty(&mut output, &current)
                .map_err(|_| "local-volume-output-encode-failed")?;
        }
    }
    writeln!(&mut output).map_err(|_| "local-volume-output-write-failed")?;
    Ok(())
}

/// Parses bounded native arguments without reflecting unknown or malformed values.
fn parse_args<I, S>(args: I) -> Result<Args, String>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let values: Vec<OsString> = args.into_iter().map(Into::into).collect();
    if values.len() == 1 && matches!(values[0].to_str(), Some("--help" | "-h")) {
        return Err("help".into());
    }

    let mut path = None;
    let mut baseline = None;
    let mut logical_removed_bytes = None;
    let mut values = values.into_iter();
    while let Some(flag) = values.next() {
        match flag.to_str() {
            Some("--help" | "-h") => {
                return Err(format!("local-volume-help-must-be-used-alone\n{USAGE}"));
            }
            Some("--path") => {
                if path.is_some() {
                    return Err("local-volume-path-duplicate".into());
                }
                path = Some(PathBuf::from(
                    values.next().ok_or("local-volume-path-value-missing")?,
                ));
            }
            Some("--baseline") => {
                if baseline.is_some() {
                    return Err("local-volume-baseline-duplicate".into());
                }
                baseline = Some(PathBuf::from(
                    values.next().ok_or("local-volume-baseline-value-missing")?,
                ));
            }
            Some("--logical-removed-bytes") => {
                if logical_removed_bytes.is_some() {
                    return Err("local-volume-logical-removed-duplicate".into());
                }
                let raw_bytes = values
                    .next()
                    .ok_or("local-volume-logical-removed-value-missing")?;
                logical_removed_bytes = Some(
                    raw_bytes
                        .to_str()
                        .ok_or("local-volume-logical-removed-invalid")?
                        .parse::<u64>()
                        .map_err(|_| "local-volume-logical-removed-invalid")?,
                );
            }
            Some(_) | None => return Err("local-volume-argument-unknown".into()),
        }
    }
    if baseline.is_none() && logical_removed_bytes.is_some() {
        return Err("local-volume-logical-removed-requires-baseline".into());
    }
    Ok(Args {
        path: path.unwrap_or_else(|| PathBuf::from(".")),
        baseline,
        logical_removed_bytes,
    })
}

/// Reads and validates one bounded regular-file baseline without following symbolic links.
fn read_baseline(path: PathBuf) -> Result<LocalVolumeSnapshot, String> {
    let metadata =
        std::fs::symlink_metadata(&path).map_err(|_| "local-volume-baseline-unavailable")?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("local-volume-baseline-not-regular-file".into());
    }
    if metadata.len() > MAX_BASELINE_BYTES {
        return Err("local-volume-baseline-too-large".into());
    }
    let file = File::open(path).map_err(|_| "local-volume-baseline-unavailable")?;
    let mut encoded = Vec::new();
    file.take(MAX_BASELINE_BYTES + 1)
        .read_to_end(&mut encoded)
        .map_err(|_| "local-volume-baseline-read-failed")?;
    if encoded.len() as u64 > MAX_BASELINE_BYTES {
        return Err("local-volume-baseline-too-large".into());
    }
    let snapshot =
        serde_json::from_slice(&encoded).map_err(|_| "local-volume-baseline-json-invalid")?;
    disksage_lib::volume_pressure::validate_snapshot(&snapshot)?;
    Ok(snapshot)
}

/// Returns a positive Unix-epoch millisecond timestamp for evidence identity.
fn now_ms() -> Result<u64, String> {
    let observed_at_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "local-volume-clock-unavailable")?
        .as_millis();
    u64::try_from(observed_at_ms)
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| "local-volume-clock-unavailable".into())
}
