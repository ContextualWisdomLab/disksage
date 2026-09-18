//! Log archive CLI (lossless zstd). Dry-run by default; pass `--execute` to compress.
//! `--older-than-days 0` disables the age gate (min-stable still applies).

use disksage_lib::log_archive::{
    archive_logs, resolve_zstd_bin, LogArchiveOptions, DEFAULT_EXCLUDE_SUFFIXES,
    DEFAULT_MIN_STABLE_SECS,
};
use std::path::{Component, Path, PathBuf};

const USAGE: &str = "usage: disksage-log-archive --root ABSOLUTE_PATH --older-than-days N (0=no age gate) [--execute] [--journal-path ABSOLUTE_PATH] [--min-stable-secs N] [--zstd-bin ABSOLUTE_PATH]";

#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    root: PathBuf,
    older_than_days: u64,
    execute: bool,
    journal_path: PathBuf,
    min_stable_secs: u64,
    zstd_bin: Option<PathBuf>,
}

fn absolute_without_parent(path: &Path) -> bool {
    path.is_absolute()
        && !path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
}

fn home_directory() -> Result<PathBuf, String> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .ok_or_else(|| "home-directory-unavailable".to_string())
}

fn default_journal_path() -> Result<PathBuf, String> {
    let home = home_directory()?;
    #[cfg(target_os = "macos")]
    let path = home
        .join("Library")
        .join("Application Support")
        .join("com.contextualwisdomlab.disksage")
        .join("journal.jsonl");
    #[cfg(target_os = "windows")]
    let path = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .filter(|value| value.is_absolute())
        .ok_or_else(|| "app-data-directory-unavailable".to_string())?
        .join("com.contextualwisdomlab.disksage")
        .join("journal.jsonl");
    #[cfg(all(unix, not(target_os = "macos")))]
    let path = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|value| value.is_absolute())
        .unwrap_or_else(|| home.join(".local").join("share"))
        .join("com.contextualwisdomlab.disksage")
        .join("journal.jsonl");
    Ok(path)
}

fn value(args: &[String], index: &mut usize, flag: &str) -> Result<String, String> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn number(args: &[String], index: &mut usize, flag: &str) -> Result<u64, String> {
    value(args, index, flag)?
        .parse()
        .map_err(|_| format!("{flag} must be an integer"))
}

fn parse_args(args: &[String]) -> Result<Option<Args>, String> {
    if args.len() == 1 && matches!(args[0].as_str(), "--help" | "-h") {
        return Ok(None);
    }

    let mut root = None;
    let mut older_than_days = None;
    let mut execute = false;
    let mut journal_path = None;
    let mut min_stable_secs = DEFAULT_MIN_STABLE_SECS;
    let mut zstd_bin = None;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--root" => {
                let path = PathBuf::from(value(args, &mut index, "--root")?);
                if !absolute_without_parent(&path) {
                    return Err("--root must be an absolute path without parent segments".into());
                }
                root = Some(path);
            }
            "--older-than-days" => {
                let days = number(args, &mut index, "--older-than-days")?;
                if days > 3_650 {
                    return Err(
                        "--older-than-days must be between 0 and 3650 (0 disables age gate)".into(),
                    );
                }
                older_than_days = Some(days);
            }
            "--execute" => execute = true,
            "--journal-path" => {
                let path = PathBuf::from(value(args, &mut index, "--journal-path")?);
                if !absolute_without_parent(&path) {
                    return Err(
                        "--journal-path must be an absolute path without parent segments".into(),
                    );
                }
                journal_path = Some(path);
            }
            "--min-stable-secs" => {
                min_stable_secs = number(args, &mut index, "--min-stable-secs")?;
                if min_stable_secs == 0 {
                    return Err("--min-stable-secs must be >= 1".into());
                }
            }
            "--zstd-bin" => {
                let path = PathBuf::from(value(args, &mut index, "--zstd-bin")?);
                if !absolute_without_parent(&path) {
                    return Err("--zstd-bin must be an absolute path without parent segments".into());
                }
                zstd_bin = Some(path);
            }
            "--help" | "-h" => return Err(format!("--help must be used alone\n{USAGE}")),
            unknown => return Err(format!("unknown argument: {unknown}\n{USAGE}")),
        }
        index += 1;
    }

    let root = root.ok_or_else(|| format!("--root is required\n{USAGE}"))?;
    let older_than_days =
        older_than_days.ok_or_else(|| format!("--older-than-days is required\n{USAGE}"))?;
    Ok(Some(Args {
        root,
        older_than_days,
        execute,
        journal_path: journal_path.unwrap_or(default_journal_path()?),
        min_stable_secs,
        zstd_bin,
    }))
}

fn main() {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let args = match parse_args(&raw) {
        Ok(None) => {
            println!("{USAGE}");
            return;
        }
        Ok(Some(args)) => args,
        Err(error) => {
            eprintln!("disksage-log-archive: {error}");
            std::process::exit(2);
        }
    };

    let zstd_bin = match args.zstd_bin {
        Some(path) => path,
        None => match resolve_zstd_bin() {
            Ok(path) => path,
            Err(error) => {
                eprintln!("disksage-log-archive: {error}");
                std::process::exit(2);
            }
        },
    };

    let options = LogArchiveOptions {
        root: args.root,
        older_than_days: args.older_than_days,
        execute: args.execute,
        journal_path: args.journal_path,
        min_stable_secs: args.min_stable_secs,
        zstd_bin,
        exclude_suffixes: DEFAULT_EXCLUDE_SUFFIXES
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
    };

    match archive_logs(&options) {
        Ok(report) => match serde_json::to_string_pretty(&report) {
            Ok(encoded) => println!("{encoded}"),
            Err(error) => {
                eprintln!("disksage-log-archive: encode-failed:{error}");
                std::process::exit(2);
            }
        },
        Err(error) => {
            eprintln!("disksage-log-archive: {error}");
            std::process::exit(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_is_non_mutating() {
        assert!(parse_args(&["--help".into()]).unwrap().is_none());
    }

    #[test]
    fn requires_root_and_age() {
        let error = parse_args(&[]).unwrap_err();
        assert!(error.contains("--root is required"));
        let error = parse_args(&[
            "--root".into(),
            "/tmp".into(),
            "--older-than-days".into(),
            "0".into(),
        ])
        .unwrap_err();
        assert!(error.contains("between 1 and 3650"));
    }
}
