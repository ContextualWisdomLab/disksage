//! Read-only, bounded duplicate audit with path-redacted stdout.

use disksage_lib::duplicate_audit::{
    audit_duplicates, summarize_duplicate_audit, DuplicateAuditOptions,
};
#[cfg(test)]
use disksage_lib::duplicate_audit::{
    DEFAULT_DUPLICATE_MAX_DURATION_MS, DEFAULT_DUPLICATE_MAX_ENTRIES,
    DEFAULT_DUPLICATE_MAX_FILES_TO_HASH, DEFAULT_DUPLICATE_MAX_HASH_BYTES,
    DEFAULT_DUPLICATE_MAX_SIZE_GROUPS, DEFAULT_DUPLICATE_MIN_FILE_BYTES,
    DEFAULT_DUPLICATE_PREFIX_BYTES,
};
use serde::Serialize;
use sha2::Digest;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_PRIVATE_REPORT_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    root: PathBuf,
    private_output: Option<PathBuf>,
    options: DuplicateAuditOptions,
}

#[derive(Debug, Serialize)]
struct PrivateOutputSummary {
    written: bool,
    create_new: bool,
    contains_sensitive_local_paths: bool,
    is_approval: bool,
    is_execution: bool,
    bytes: usize,
    sha256: String,
    unix_mode: String,
}

#[derive(Debug, Serialize)]
struct CliSummary {
    #[serde(flatten)]
    audit: disksage_lib::duplicate_audit::DuplicateAuditSummary,
    private_output: PrivateOutputSummary,
}

fn usage() -> &'static str {
    "DiskSage duplicate audit: usage: disksage-duplicate-audit \
     --root ABSOLUTE_PATH \
     [--private-output NEW_ABSOLUTE_JSON_PATH] \
     [--min-file-mib N] [--prefix-kib N] [--max-entries N] \
     [--max-duration-ms N] [--max-files-to-hash N] [--max-size-groups N] \
     [--max-hash-gib N]"
}

fn value(raw: &[String], index: &mut usize, flag: &str) -> Result<String, String> {
    *index += 1;
    raw.get(*index)
        .cloned()
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn parse_positive<T: std::str::FromStr>(
    raw: &[String],
    index: &mut usize,
    flag: &str,
) -> Result<T, String>
where
    <T as std::str::FromStr>::Err: std::fmt::Display,
{
    value(raw, index, flag)?
        .parse::<T>()
        .map_err(|error| format!("{flag} must be a positive integer: {error}"))
}

fn absolute_without_parent(path: &Path) -> bool {
    path.is_absolute()
        && !path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
}

fn parse_args(raw: &[String]) -> Result<Args, String> {
    let mut root = None;
    let mut private_output = None;
    let mut options = DuplicateAuditOptions::default();
    let mut seen = std::collections::BTreeSet::new();
    let mut index = 0usize;
    while index < raw.len() {
        let flag = raw[index].as_str();
        if !seen.insert(flag.to_string()) {
            return Err(format!("{flag} may only be specified once"));
        }
        match flag {
            "--root" => root = Some(PathBuf::from(value(raw, &mut index, flag)?)),
            "--private-output" => {
                private_output = Some(PathBuf::from(value(raw, &mut index, flag)?))
            }
            "--min-file-mib" => {
                let value: u64 = parse_positive(raw, &mut index, flag)?;
                if !(1..=1024 * 1024).contains(&value) {
                    return Err("--min-file-mib is out of bounds".into());
                }
                options.min_file_bytes = value.saturating_mul(1024 * 1024);
            }
            "--prefix-kib" => {
                let value: usize = parse_positive(raw, &mut index, flag)?;
                if !(1..=16 * 1024).contains(&value) {
                    return Err("--prefix-kib is out of bounds".into());
                }
                options.prefix_bytes = value.saturating_mul(1024);
            }
            "--max-entries" => {
                let value: usize = parse_positive(raw, &mut index, flag)?;
                if !(1..=2_000_000).contains(&value) {
                    return Err("--max-entries is out of bounds".into());
                }
                options.max_entries = value;
            }
            "--max-duration-ms" => {
                let value: u64 = parse_positive(raw, &mut index, flag)?;
                if !(1..=15 * 60_000).contains(&value) {
                    return Err("--max-duration-ms is out of bounds".into());
                }
                options.max_duration_ms = value;
            }
            "--max-files-to-hash" => {
                let value: usize = parse_positive(raw, &mut index, flag)?;
                if !(2..=200_000).contains(&value) {
                    return Err("--max-files-to-hash is out of bounds".into());
                }
                options.max_files_to_hash = value;
            }
            "--max-size-groups" => {
                let value: usize = parse_positive(raw, &mut index, flag)?;
                if !(1..=100_000).contains(&value) {
                    return Err("--max-size-groups is out of bounds".into());
                }
                options.max_size_groups = value;
            }
            "--max-hash-gib" => {
                let value: u64 = parse_positive(raw, &mut index, flag)?;
                if !(1..=1024).contains(&value) {
                    return Err("--max-hash-gib is out of bounds".into());
                }
                options.max_hash_bytes = value.saturating_mul(1024 * 1024 * 1024);
            }
            "--help" | "-h" => return Err(usage().into()),
            unknown => return Err(format!("unknown argument: {unknown}")),
        }
        index += 1;
    }
    let root = root.ok_or_else(|| "--root is required".to_string())?;
    if !absolute_without_parent(&root) {
        return Err("--root must be an absolute path without parent traversal".into());
    }
    if let Some(path) = &private_output {
        if !absolute_without_parent(path) {
            return Err(
                "--private-output must be an absolute path without parent traversal".into(),
            );
        }
    }
    Ok(Args {
        root,
        private_output,
        options,
    })
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn write_private_report(
    path: Option<&Path>,
    encoded: &[u8],
) -> Result<PrivateOutputSummary, String> {
    let Some(path) = path else {
        return Ok(PrivateOutputSummary {
            written: false,
            create_new: true,
            contains_sensitive_local_paths: true,
            is_approval: false,
            is_execution: false,
            bytes: 0,
            sha256: String::new(),
            unix_mode: "not-written".into(),
        });
    };
    if encoded.len() > MAX_PRIVATE_REPORT_BYTES {
        return Err("duplicate-audit-private-report-too-large".into());
    }
    let parent = path
        .parent()
        .filter(|parent| parent.is_dir())
        .ok_or_else(|| "duplicate-audit-private-output-parent-unavailable".to_string())?;
    if std::fs::symlink_metadata(parent)
        .map_err(|_| "duplicate-audit-private-output-parent-unavailable")?
        .file_type()
        .is_symlink()
    {
        return Err("duplicate-audit-private-output-parent-unsafe".into());
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|_| "duplicate-audit-private-output-create-new-failed".to_string())?;
    file.write_all(encoded)
        .and_then(|_| file.sync_all())
        .map_err(|_| "duplicate-audit-private-output-write-failed".to_string())?;
    let digest = sha2::Sha256::digest(encoded);
    Ok(PrivateOutputSummary {
        written: true,
        create_new: true,
        contains_sensitive_local_paths: true,
        is_approval: false,
        is_execution: false,
        bytes: encoded.len(),
        sha256: hex_lower(&digest),
        unix_mode: if cfg!(unix) {
            "0600"
        } else {
            "platform-default"
        }
        .into(),
    })
}

fn system_now_ms() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "system-time-before-unix-epoch".to_string())
        .and_then(|duration| {
            u64::try_from(duration.as_millis())
                .map_err(|_| "system-time-milliseconds-overflow".to_string())
        })
}

fn run() -> Result<(), String> {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let args = parse_args(&raw)?;
    let report = audit_duplicates(&args.root, &args.options, system_now_ms()?)?;
    let private_json =
        serde_json::to_vec_pretty(&report).map_err(|error| format!("json: {error}"))?;
    let private_output = write_private_report(args.private_output.as_deref(), &private_json)?;
    let summary = CliSummary {
        audit: summarize_duplicate_audit(&report),
        private_output,
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&summary).map_err(|error| format!("json: {error}"))?
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_requires_absolute_root_and_uses_bounded_defaults() {
        let args = parse_args(&["--root".into(), "/tmp/source".into()]).unwrap();
        assert_eq!(args.root, PathBuf::from("/tmp/source"));
        assert_eq!(
            args.options.min_file_bytes,
            DEFAULT_DUPLICATE_MIN_FILE_BYTES
        );
        assert_eq!(args.options.prefix_bytes, DEFAULT_DUPLICATE_PREFIX_BYTES);
        assert_eq!(args.options.max_entries, DEFAULT_DUPLICATE_MAX_ENTRIES);
        assert_eq!(
            args.options.max_duration_ms,
            DEFAULT_DUPLICATE_MAX_DURATION_MS
        );
        assert_eq!(
            args.options.max_files_to_hash,
            DEFAULT_DUPLICATE_MAX_FILES_TO_HASH
        );
        assert_eq!(
            args.options.max_size_groups,
            DEFAULT_DUPLICATE_MAX_SIZE_GROUPS
        );
        assert_eq!(
            args.options.max_hash_bytes,
            DEFAULT_DUPLICATE_MAX_HASH_BYTES
        );
        assert!(parse_args(&[]).is_err());
        assert!(parse_args(&["--root".into(), "relative".into()]).is_err());
    }

    #[test]
    fn parser_accepts_all_bounds_and_rejects_duplicates() {
        let args = parse_args(&[
            "--root".into(),
            "/tmp/source".into(),
            "--private-output".into(),
            "/tmp/private.json".into(),
            "--min-file-mib".into(),
            "2".into(),
            "--prefix-kib".into(),
            "128".into(),
            "--max-entries".into(),
            "500".into(),
            "--max-duration-ms".into(),
            "1000".into(),
            "--max-files-to-hash".into(),
            "50".into(),
            "--max-size-groups".into(),
            "20".into(),
            "--max-hash-gib".into(),
            "3".into(),
        ])
        .unwrap();
        assert_eq!(args.options.min_file_bytes, 2 * 1024 * 1024);
        assert_eq!(args.options.prefix_bytes, 128 * 1024);
        assert_eq!(args.options.max_entries, 500);
        assert_eq!(args.options.max_duration_ms, 1000);
        assert_eq!(args.options.max_files_to_hash, 50);
        assert_eq!(args.options.max_size_groups, 20);
        assert_eq!(args.options.max_hash_bytes, 3 * 1024 * 1024 * 1024);
        assert!(parse_args(&[
            "--root".into(),
            "/tmp/source".into(),
            "--root".into(),
            "/tmp/other".into()
        ])
        .is_err());
    }

    #[test]
    fn private_output_is_create_new_and_never_overwritten() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("private.json");
        let summary = write_private_report(Some(&path), b"{\"safe\":true}").unwrap();
        assert!(summary.written);
        assert_eq!(summary.unix_mode, "0600");
        assert!(write_private_report(Some(&path), b"replacement").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"{\"safe\":true}");
    }
}
