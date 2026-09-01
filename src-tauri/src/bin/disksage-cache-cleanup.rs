//! Headless execution entry point for the narrow, observed cache policy.
//!
//! Without `--execute` this command is read-only. With it, the library path moves only inactive,
//! identity-bound children of the npm, pnpm, Adobe, Edge, uv, and Trivy cache roots to OS Trash.

use disksage_lib::cache_cleanup::{
    clean_regenerable_caches_headless, proven_cache_trash_candidates, purge_proven_cache_trash,
    CacheTrashCandidate,
};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

const APPROVAL_MANIFEST_MAX_BYTES: u64 = 8 * 1024 * 1024;
const USAGE: &str = "Usage: disksage-cache-cleanup [--execute] [--purge-proven-cache-trash] \
[--approved-cache-trash-candidates PATH] [--journal-path PATH]\n\
Without --execute it reports the command is a no-op. With --execute it moves only observed,\n\
inactive regenerable cache children to OS Trash. Irreversible --purge-proven-cache-trash execution\n\
requires an absolute --approved-cache-trash-candidates JSON path captured from a prior dry-run.";

#[derive(Debug, PartialEq, Eq)]
struct Args {
    execute: bool,
    purge_proven_cache_trash: bool,
    approved_cache_trash_candidates: Option<PathBuf>,
    journal_path: PathBuf,
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

fn parse_args(raw_args: impl IntoIterator<Item = OsString>) -> Result<Option<Args>, String> {
    let mut args = raw_args.into_iter();
    let first_arg = args.next();
    if matches!(
        first_arg.as_ref().and_then(|arg| arg.to_str()),
        Some("-h" | "--help")
    ) {
        if args.next().is_none() {
            return Ok(None);
        }
        return Err(format!("--help must be used alone\n{USAGE}"));
    }

    let mut execute = false;
    let mut purge_proven_cache_trash = false;
    let mut approved_cache_trash_candidates = None;
    let mut journal_path = default_journal_path()?;
    let mut args = first_arg.into_iter().chain(args);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--execute") => execute = true,
            Some("--purge-proven-cache-trash") => purge_proven_cache_trash = true,
            Some("--approved-cache-trash-candidates") => {
                let path = PathBuf::from(
                    args.next().ok_or_else(|| {
                        "--approved-cache-trash-candidates requires PATH".to_string()
                    })?,
                );
                if !path.is_absolute() {
                    return Err("--approved-cache-trash-candidates must be absolute".into());
                }
                approved_cache_trash_candidates = Some(path);
            }
            Some("--journal-path") => {
                journal_path = PathBuf::from(
                    args.next()
                        .ok_or_else(|| "--journal-path requires PATH".to_string())?,
                );
                if !journal_path.is_absolute() {
                    return Err("--journal-path must be absolute".into());
                }
            }
            Some("-h" | "--help") => return Err(format!("--help must be used alone\n{USAGE}")),
            Some(value) => return Err(format!("unknown option: {value}\n{USAGE}")),
            None => return Err(format!("invalid UTF-8 option\n{USAGE}")),
        }
    }
    if approved_cache_trash_candidates.is_some() && !(execute && purge_proven_cache_trash) {
        return Err(
            "--approved-cache-trash-candidates requires --execute --purge-proven-cache-trash"
                .into(),
        );
    }
    if execute && purge_proven_cache_trash && approved_cache_trash_candidates.is_none() {
        return Err(
            "--execute --purge-proven-cache-trash requires --approved-cache-trash-candidates PATH"
                .into(),
        );
    }
    Ok(Some(Args {
        execute,
        purge_proven_cache_trash,
        approved_cache_trash_candidates,
        journal_path,
    }))
}

fn load_approved_cache_trash_candidates(path: &Path) -> Result<Vec<CacheTrashCandidate>, String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| "cache-trash-approval-manifest-stat-failed".to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("cache-trash-approval-manifest-must-be-regular-file".into());
    }
    if metadata.len() > APPROVAL_MANIFEST_MAX_BYTES {
        return Err("cache-trash-approval-manifest-too-large".into());
    }
    let bytes = std::fs::read(path)
        .map_err(|_| "cache-trash-approval-manifest-read-failed".to_string())?;
    serde_json::from_slice(&bytes)
        .map_err(|_| "cache-trash-approval-manifest-invalid-json".to_string())
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn run_with_args(raw_args: impl IntoIterator<Item = OsString>) -> Result<(), String> {
    let Some(args) = parse_args(raw_args)? else {
        println!("{USAGE}");
        return Ok(());
    };
    if !args.execute {
        let cache_trash = if args.purge_proven_cache_trash {
            serde_json::to_value(proven_cache_trash_candidates(&home_directory()?))
                .map_err(|error| error.to_string())?
        } else {
            serde_json::Value::Array(Vec::new())
        };
        println!(
            "{}",
            serde_json::json!({
                "executed": false,
                "journal_path": args.journal_path,
                "purge_proven_cache_trash": args.purge_proven_cache_trash,
                "proven_cache_trash": cache_trash,
                "approval_contract": "review and persist the exact proven_cache_trash array, then pass that JSON array with --approved-cache-trash-candidates when executing the irreversible purge",
                "notice": "pass --execute to perform a guarded operation"
            })
        );
        return Ok(());
    }
    if let Some(parent) = args.journal_path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    if args.purge_proven_cache_trash {
        let approval_path = args
            .approved_cache_trash_candidates
            .as_ref()
            .ok_or("cache-trash-approval-manifest-required")?;
        let approved = load_approved_cache_trash_candidates(approval_path)?;
        let results = purge_proven_cache_trash(
            &home_directory()?,
            &approved,
            &args.journal_path,
            now_ms(),
        )?;
        println!(
            "{}",
            serde_json::json!({
                "executed": true,
                "purge_proven_cache_trash": true,
                "approved_cache_trash_candidates": approval_path,
                "approved_candidate_count": approved.len(),
                "journal_path": args.journal_path,
                "results": results
            })
        );
        return Ok(());
    }
    let evidence = clean_regenerable_caches_headless(&args.journal_path, now_ms())?;
    println!(
        "{}",
        serde_json::json!({
            "executed": true,
            "journal_path": args.journal_path,
            "results": evidence
        })
    );
    Ok(())
}

fn main() {
    if let Err(error) = run_with_args(std::env::args_os().skip(1)) {
        eprintln!("disksage-cache-cleanup: {error}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn help_is_non_mutating() {
        assert!(parse_args([OsString::from("--help")]).unwrap().is_none());
    }

    #[test]
    fn help_must_be_used_alone() {
        let error = parse_args([
            OsString::from("--help"),
            OsString::from("--execute"),
        ])
        .unwrap_err();
        assert!(error.starts_with("--help must be used alone"));
    }

    #[test]
    fn relative_journal_path_is_rejected() {
        let error = parse_args([
            OsString::from("--journal-path"),
            OsString::from("journal.jsonl"),
        ])
        .unwrap_err();
        assert_eq!(error, "--journal-path must be absolute");
    }

    #[test]
    fn purge_cache_trash_requires_exact_approval_manifest_for_execution() {
        let error = parse_args([
            OsString::from("--execute"),
            OsString::from("--purge-proven-cache-trash"),
        ])
        .unwrap_err();
        assert!(error.contains("requires --approved-cache-trash-candidates PATH"));

        let args = parse_args([
            OsString::from("--execute"),
            OsString::from("--purge-proven-cache-trash"),
            OsString::from("--approved-cache-trash-candidates"),
            OsString::from("/tmp/approved-cache-trash.json"),
        ])
        .unwrap()
        .unwrap();
        assert!(args.execute);
        assert!(args.purge_proven_cache_trash);
        assert_eq!(
            args.approved_cache_trash_candidates,
            Some(PathBuf::from("/tmp/approved-cache-trash.json"))
        );
    }

    #[test]
    fn approval_manifest_flag_cannot_be_used_outside_irreversible_purge() {
        let error = parse_args([
            OsString::from("--approved-cache-trash-candidates"),
            OsString::from("/tmp/approved-cache-trash.json"),
        ])
        .unwrap_err();
        assert!(error.contains("requires --execute --purge-proven-cache-trash"));
    }

    #[test]
    fn approval_manifest_reader_rejects_symlink_and_oversize_inputs() {
        let tmp = tempfile::tempdir().unwrap();
        let manifest = tmp.path().join("approval.json");
        fs::write(&manifest, b"[]").unwrap();
        assert!(load_approved_cache_trash_candidates(&manifest)
            .unwrap()
            .is_empty());

        let oversized = tmp.path().join("oversized.json");
        let file = fs::File::create(&oversized).unwrap();
        file.set_len(APPROVAL_MANIFEST_MAX_BYTES + 1).unwrap();
        assert_eq!(
            load_approved_cache_trash_candidates(&oversized).unwrap_err(),
            "cache-trash-approval-manifest-too-large"
        );

        #[cfg(unix)]
        {
            let link = tmp.path().join("approval-link.json");
            std::os::unix::fs::symlink(&manifest, &link).unwrap();
            assert_eq!(
                load_approved_cache_trash_candidates(&link).unwrap_err(),
                "cache-trash-approval-manifest-must-be-regular-file"
            );
        }
    }
}
