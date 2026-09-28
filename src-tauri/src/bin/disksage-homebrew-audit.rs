//! Read-only Homebrew stale / orphan software audit. This command never uninstalls packages.

use std::collections::BTreeSet;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use disksage_lib::homebrew_audit::{
    audit_homebrew, HomebrewAuditOptions, HomebrewAuditReport, DEFAULT_COMMAND_TIMEOUT_MS,
    DEFAULT_STALE_AFTER_DAYS,
};

const USAGE: &str = "usage: disksage-homebrew-audit [--repo-root ABSOLUTE_PATH ...] [--stale-after-days N] [--name NAME ...] [--output NEW_ABSOLUTE_JSON_PATH] [--command-timeout-ms N]";

#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    repository_roots: Vec<PathBuf>,
    stale_after_days: u64,
    names: BTreeSet<String>,
    output: Option<PathBuf>,
    command_timeout_ms: u64,
}

fn absolute_without_parent(path: &Path) -> bool {
    path.is_absolute()
        && !path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
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
    let mut repository_roots = Vec::new();
    let mut stale_after_days = DEFAULT_STALE_AFTER_DAYS;
    let mut names = BTreeSet::new();
    let mut output = None;
    let mut command_timeout_ms = DEFAULT_COMMAND_TIMEOUT_MS;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--repo-root" => {
                let path = PathBuf::from(value(args, &mut index, "--repo-root")?);
                if !absolute_without_parent(&path) || !path.is_dir() {
                    return Err("--repo-root must be an existing absolute directory".into());
                }
                repository_roots.push(path);
            }
            "--stale-after-days" => {
                stale_after_days = number(args, &mut index, "--stale-after-days")?;
                if stale_after_days == 0 || stale_after_days > 3_650 {
                    return Err("--stale-after-days must be between 1 and 3650".into());
                }
            }
            "--name" => {
                names.insert(value(args, &mut index, "--name")?);
            }
            "--output" => {
                let path = PathBuf::from(value(args, &mut index, "--output")?);
                if !absolute_without_parent(&path) {
                    return Err("--output must be an absolute path without parent segments".into());
                }
                output = Some(path);
            }
            "--command-timeout-ms" => {
                command_timeout_ms = number(args, &mut index, "--command-timeout-ms")?;
                if command_timeout_ms < 1_000 {
                    return Err("--command-timeout-ms must be at least 1000".into());
                }
            }
            "--help" | "-h" => return Err(format!("--help must be used alone\n{USAGE}")),
            unknown => return Err(format!("unknown argument: {unknown}\n{USAGE}")),
        }
        index += 1;
    }
    Ok(Some(Args {
        repository_roots,
        stale_after_days,
        names,
        output,
        command_timeout_ms,
    }))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|value| u64::try_from(value.as_millis()).ok())
        .unwrap_or(0)
}

fn write_new_private_json(path: &Path, encoded: &[u8]) -> Result<(), String> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|_| "homebrew-audit-output-create-failed".to_string())?;
    file.write_all(encoded)
        .map_err(|_| "homebrew-audit-output-write-failed".to_string())?;
    file.sync_all()
        .map_err(|_| "homebrew-audit-output-sync-failed".to_string())
}

fn main() {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let args = match parse_args(&raw) {
        Ok(Some(args)) => args,
        Ok(None) => {
            println!("{USAGE}");
            std::process::exit(0);
        }
        Err(error) => {
            eprintln!("DiskSage Homebrew audit: {error}");
            std::process::exit(2);
        }
    };

    let report = match audit_homebrew(
        HomebrewAuditOptions {
            stale_after_days: args.stale_after_days,
            command_timeout_ms: args.command_timeout_ms,
            repository_roots: args.repository_roots,
            name_filter: args.names,
            ..HomebrewAuditOptions::default()
        },
        now_ms(),
    ) {
        Ok(report) => report,
        Err(error) => {
            eprintln!("DiskSage Homebrew audit: {error}");
            std::process::exit(1);
        }
    };

    let encoded = match serde_json::to_vec_pretty(&report) {
        Ok(value) => value,
        Err(_) => {
            eprintln!("DiskSage Homebrew audit: homebrew-audit-encode-failed");
            std::process::exit(1);
        }
    };

    if let Some(path) = args.output {
        if let Err(error) = write_new_private_json(&path, &encoded) {
            eprintln!("DiskSage Homebrew audit: {error}");
            std::process::exit(1);
        }
        print_summary(&report);
    } else {
        let text = String::from_utf8(encoded).unwrap_or_else(|_| "{}".into());
        println!("{text}");
    }
}

fn print_summary(report: &HomebrewAuditReport) {
    println!(
        "homebrew-audit packages={} evidence_complete={} counts={:?}",
        report.packages.len(),
        report.evidence_complete,
        report.classification_counts
    );
    for package in &report.packages {
        println!(
            "{} {:?} {:?} reasons={:?} bytes={:?} last_use_ms={:?}",
            package.name,
            package.kind,
            package.classification,
            package.reason_codes,
            package.installed_bytes,
            package.last_use.observed_at_ms
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_parses_as_terminal_discovery() {
        assert_eq!(parse_args(&["--help".into()]).unwrap(), None);
        assert_eq!(parse_args(&["-h".into()]).unwrap(), None);
    }

    #[test]
    fn rejects_relative_repo_root() {
        let error = parse_args(&["--repo-root".into(), "relative".into()]).unwrap_err();
        assert!(error.contains("absolute"));
    }
}
