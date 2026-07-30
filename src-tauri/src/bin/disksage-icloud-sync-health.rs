//! Headless, path-free report for the local macOS CloudDocs sync queue.

use disksage_lib::icloud_sync_health::{default_cloud_docs_db_dir, probe_icloud_sync_health};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    db_dir: PathBuf,
}

fn parse_args(args: &[String], home: &Path) -> Result<Args, String> {
    let mut parsed = Args {
        db_dir: default_cloud_docs_db_dir(home),
    };
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--db-dir" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--db-dir requires an absolute path".to_string())?;
                parsed.db_dir = PathBuf::from(value);
            }
            "--help" | "-h" => {
                return Err(
                    "usage: disksage-icloud-sync-health [--db-dir ABSOLUTE_CLOUDDOCS_DB_DIR]"
                        .into(),
                );
            }
            flag => return Err(format!("unknown argument: {flag}")),
        }
        index += 1;
    }
    if !parsed.db_dir.is_absolute() {
        return Err("--db-dir must be absolute".into());
    }
    Ok(parsed)
}

fn now_ms() -> Result<u64, String> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "system-clock-before-unix-epoch".to_string())?;
    u64::try_from(duration.as_millis()).map_err(|_| "system-time-overflow".into())
}

fn run() -> Result<(), String> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "HOME is unavailable".to_string())?;
    let args = parse_args(&std::env::args().skip(1).collect::<Vec<_>>(), &home)?;
    let report = probe_icloud_sync_health(&args.db_dir, now_ms()?)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&report)
            .map_err(|_| "icloud-sync-health-json-invalid".to_string())?
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("DiskSage iCloud sync health: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_defaults_to_cloud_docs_and_accepts_absolute_override() {
        let defaults = parse_args(&[], Path::new("/home/test")).unwrap();
        assert_eq!(
            defaults.db_dir,
            PathBuf::from("/home/test/Library/Application Support/CloudDocs/session/db")
        );
        let explicit = parse_args(
            &["--db-dir".into(), "/private/db".into()],
            Path::new("/home/test"),
        )
        .unwrap();
        assert_eq!(explicit.db_dir, PathBuf::from("/private/db"));
    }

    #[test]
    fn parser_rejects_unknown_missing_and_relative_values() {
        assert!(parse_args(&["--wat".into()], Path::new("/home/test")).is_err());
        assert!(parse_args(&["--db-dir".into()], Path::new("/home/test")).is_err());
        assert!(parse_args(
            &["--db-dir".into(), "relative".into()],
            Path::new("/home/test")
        )
        .is_err());
    }
}
