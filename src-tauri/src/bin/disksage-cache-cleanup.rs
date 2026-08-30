//! Headless execution entry point for the narrow, observed cache policy.
//!
//! Without `--execute` this command is read-only. With it, the library path moves only inactive,
//! identity-bound children of admitted regenerable cache roots to OS Trash. The npx-only scope is
//! reported explicitly in both dry-run and execution receipts.

use disksage_lib::cache_cleanup::{
    clean_catalog_cache_headless, clean_inactive_npx_environments_headless,
    clean_regenerable_caches_headless, proven_cache_trash_candidates, purge_proven_cache_trash,
};
use disksage_lib::cache_cleanup_preview::preview_catalog_cache_headless;
use std::ffi::OsString;
use std::path::PathBuf;

const USAGE: &str = "Usage: disksage-cache-cleanup [--execute] [--cache-id ID [--permanent-cache] | --npx-only | --purge-proven-cache-trash] [--journal-path PATH]\n\
Without --execute it reports the command is a no-op. With --execute it moves only observed,\n\
inactive regenerable cache children to OS Trash. --npx-only limits that reversible operation to\n\
inactive npx environments. --purge-proven-cache-trash permanently removes only structurally\n\
proven cache directories already in OS Trash.";

#[derive(Debug, PartialEq, Eq)]
struct Args {
    execute: bool,
    npx_only: bool,
    cache_id: Option<String>,
    permanent_cache: bool,
    purge_proven_cache_trash: bool,
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
    let mut npx_only = false;
    let mut cache_id = None;
    let mut permanent_cache = false;
    let mut purge_proven_cache_trash = false;
    let mut journal_path = default_journal_path()?;
    let mut args = first_arg.into_iter().chain(args);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--execute") => execute = true,
            Some("--npx-only") => npx_only = true,
            Some("--cache-id") => {
                if cache_id.is_some() {
                    return Err("--cache-id may be supplied only once".into());
                }
                cache_id = Some(
                    args.next()
                        .and_then(|value| value.into_string().ok())
                        .ok_or_else(|| "--cache-id requires UTF-8 ID".to_string())?,
                );
            }
            Some("--permanent-cache") => permanent_cache = true,
            Some("--purge-proven-cache-trash") => purge_proven_cache_trash = true,
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
    if usize::from(npx_only)
        + usize::from(purge_proven_cache_trash)
        + usize::from(cache_id.is_some())
        > 1
    {
        return Err("cache cleanup modes are mutually exclusive".into());
    }
    if permanent_cache && cache_id.is_none() {
        return Err("--permanent-cache requires --cache-id".into());
    }
    // `--permanent-cache` is an explicit opt-in rather than a parser-level disable. The parser
    // only admits it with a named cache; the library remains responsible for the irreversible
    // allowlist plus manifest and active-use revalidation at the mutation boundary.
    Ok(Some(Args {
        execute,
        npx_only,
        cache_id,
        permanent_cache,
        purge_proven_cache_trash,
        journal_path,
    }))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn execution_receipt(args: &Args, results: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "executed": true,
        "npx_only": args.npx_only,
        "journal_path": args.journal_path,
        "purge_proven_cache_trash": args.purge_proven_cache_trash,
        "cache_id": args.cache_id.as_deref(),
        "permanent_cache": args.permanent_cache,
        "results": results
    })
}

fn run_with_args(raw_args: impl IntoIterator<Item = OsString>) -> Result<(), String> {
    let Some(args) = parse_args(raw_args)? else {
        println!("{USAGE}");
        return Ok(());
    };
    if !args.execute {
        let cache_targets = if let Some(cache_id) = args.cache_id.as_deref() {
            serde_json::to_value(preview_catalog_cache_headless(cache_id)?)
                .map_err(|error| error.to_string())?
        } else {
            serde_json::Value::Array(Vec::new())
        };
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
                "npx_only": args.npx_only,
                "journal_path": args.journal_path,
                "purge_proven_cache_trash": args.purge_proven_cache_trash,
                "cache_id": args.cache_id,
                "permanent_cache": args.permanent_cache,
                "cache_targets": cache_targets,
                "proven_cache_trash": cache_trash,
                "notice": "pass --execute to perform the guarded OS-Trash operation"
            })
        );
        return Ok(());
    }
    if let Some(parent) = args.journal_path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    if args.purge_proven_cache_trash {
        let results = serde_json::to_value(purge_proven_cache_trash(
            &home_directory()?,
            &args.journal_path,
            now_ms(),
        )?)
        .map_err(|error| error.to_string())?;
        println!("{}", execution_receipt(&args, results));
        return Ok(());
    }
    let evidence = if let Some(cache_id) = args.cache_id.as_deref() {
        serde_json::to_value(clean_catalog_cache_headless(
            cache_id,
            &args.journal_path,
            now_ms(),
            args.permanent_cache,
        )?)
        .map_err(|error| error.to_string())?
    } else if args.npx_only {
        serde_json::to_value(clean_inactive_npx_environments_headless(
            &args.journal_path,
            now_ms(),
        )?)
        .map_err(|error| error.to_string())?
    } else {
        clean_regenerable_caches_headless(&args.journal_path, now_ms())?
    };
    println!("{}", execution_receipt(&args, evidence));
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

    #[test]
    fn help_is_non_mutating() {
        assert!(parse_args([OsString::from("--help")]).unwrap().is_none());
    }

    #[test]
    fn help_must_be_used_alone() {
        let error =
            parse_args([OsString::from("--help"), OsString::from("--execute")]).unwrap_err();
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
    fn purge_cache_trash_flag_is_explicit() {
        let args = parse_args([OsString::from("--purge-proven-cache-trash")])
            .unwrap()
            .unwrap();
        assert!(!args.execute);
        assert!(!args.npx_only);
        assert!(args.purge_proven_cache_trash);
    }

    #[test]
    fn npx_only_scope_is_explicit_and_exclusive() {
        let args = parse_args([OsString::from("--execute"), OsString::from("--npx-only")])
            .unwrap()
            .unwrap();
        assert!(args.execute);
        assert!(args.npx_only);
        assert!(parse_args([
            OsString::from("--npx-only"),
            OsString::from("--purge-proven-cache-trash"),
        ])
        .is_err());
    }

    #[test]
    fn named_cache_mode_is_explicit_and_exclusive() {
        let args = parse_args([
            OsString::from("--cache-id"),
            OsString::from("gradle-cache"),
            OsString::from("--permanent-cache"),
        ])
        .unwrap()
        .unwrap();
        assert_eq!(args.cache_id.as_deref(), Some("gradle-cache"));
        assert!(args.permanent_cache);
        assert!(parse_args([OsString::from("--permanent-cache")]).is_err());
        assert!(parse_args([
            OsString::from("--cache-id"),
            OsString::from("gradle-cache"),
            OsString::from("--npx-only"),
        ])
        .is_err());
        assert!(parse_args([
            OsString::from("--cache-id"),
            OsString::from("gradle-cache"),
            OsString::from("--cache-id"),
            OsString::from("gradle-wrapper-cache"),
        ])
        .is_err());
    }

    #[test]
    fn permanent_cache_mode_requires_explicit_execution_and_cache_identity() {
        let args = parse_args([
            OsString::from("--execute"),
            OsString::from("--cache-id"),
            OsString::from("gradle-cache"),
            OsString::from("--permanent-cache"),
        ])
        .unwrap()
        .unwrap();
        assert!(args.execute);
        assert!(args.permanent_cache);
        assert_eq!(args.cache_id.as_deref(), Some("gradle-cache"));
    }

    #[test]
    fn execution_receipt_preserves_selected_cache_mode() {
        let args = Args {
            execute: true,
            npx_only: false,
            cache_id: Some("gradle-cache".into()),
            permanent_cache: false,
            purge_proven_cache_trash: false,
            journal_path: PathBuf::from("/tmp/disksage-journal.jsonl"),
        };
        let receipt = execution_receipt(&args, serde_json::json!([{"ok": true}]));
        assert_eq!(receipt["cache_id"], "gradle-cache");
        assert_eq!(receipt["permanent_cache"], false);
        assert_eq!(receipt["executed"], true);
    }
}
