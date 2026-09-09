//! Headless, path-free report for the local macOS CloudDocs sync queue.

use disksage_lib::icloud_sync_health::{default_cloud_docs_db_dir, probe_icloud_sync_health};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const USAGE: &str = "usage: disksage-icloud-sync-health [--db-dir ABSOLUTE_CLOUDDOCS_DB_DIR] [--output ABSOLUTE_NEW_FILE.json]\n\
다음 단계: 차단 상태와 근거 시각을 확인하세요. 이 명령은 동기화 데이터베이스를 변경하지 않습니다.";

#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    db_dir: PathBuf,
    output: Option<PathBuf>,
}

fn parse_args(args: &[String], home: &Path) -> Result<Args, String> {
    let mut parsed = Args {
        db_dir: default_cloud_docs_db_dir(home),
        output: None,
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
            "--output" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--output requires an absolute new file".to_string())?;
                let output = PathBuf::from(value);
                if !output.is_absolute() {
                    return Err("--output must be absolute".into());
                }
                if parsed.output.replace(output).is_some() {
                    return Err("--output may be supplied once".into());
                }
            }
            "--help" | "-h" => {
                return Err(USAGE.into());
            }
            _unknown => return Err("icloud-sync-health-unknown-argument".into()),
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

fn write_create_new(path: &Path, encoded: &[u8]) -> Result<(), String> {
    #[cfg(unix)]
    let mut file = {
        use std::os::unix::fs::OpenOptionsExt;
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .map_err(|_| "icloud-sync-health-output-create-failed".to_string())?
    };
    #[cfg(not(unix))]
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| "icloud-sync-health-output-create-failed".to_string())?;
    file.write_all(encoded)
        .and_then(|_| file.sync_all())
        .map_err(|_| "icloud-sync-health-output-write-failed".to_string())
}

fn command_line_args() -> Result<Vec<String>, String> {
    std::env::args_os()
        .skip(1)
        .map(|argument| {
            argument
                .into_string()
                .map_err(|_| "icloud-sync-health-invalid-utf8-argument".to_string())
        })
        .collect()
}

fn run() -> Result<(), String> {
    let cli_args = command_line_args()?;
    if matches!(cli_args.as_slice(), [flag] if flag == "--help" || flag == "-h") {
        println!("{USAGE}");
        return Ok(());
    }

    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "HOME is unavailable".to_string())?;
    let args = parse_args(&cli_args, &home)?;
    let report = probe_icloud_sync_health(&args.db_dir, now_ms()?)?;
    let encoded = serde_json::to_vec_pretty(&report)
        .map_err(|_| "icloud-sync-health-json-invalid".to_string())?;
    if let Some(path) = args.output.as_deref() {
        write_create_new(path, &encoded)?;
    }
    println!(
        "{}",
        std::str::from_utf8(&encoded).map_err(|_| "icloud-sync-health-json-invalid".to_string())?
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        let exit_code = if error == "icloud-sync-health-invalid-utf8-argument" {
            2
        } else {
            1
        };
        eprintln!("DiskSage iCloud sync health: {error}");
        std::process::exit(exit_code);
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
        assert!(defaults.output.is_none());
        let explicit = parse_args(
            &["--db-dir".into(), "/private/db".into()],
            Path::new("/home/test"),
        )
        .unwrap();
        assert_eq!(explicit.db_dir, PathBuf::from("/private/db"));
        let output = parse_args(
            &["--output".into(), "/private/new.json".into()],
            Path::new("/home/test"),
        )
        .unwrap();
        assert_eq!(output.output, Some(PathBuf::from("/private/new.json")));
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
        assert!(parse_args(
            &["--output".into(), "relative.json".into()],
            Path::new("/home/test")
        )
        .is_err());
    }

    #[test]
    #[cfg(unix)]
    fn output_is_create_new_mode_0600() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("health.json");
        write_create_new(&path, b"{}").unwrap();

        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(write_create_new(&path, b"changed").is_err());
        assert_eq!(std::fs::read(path).unwrap(), b"{}");
    }
}
