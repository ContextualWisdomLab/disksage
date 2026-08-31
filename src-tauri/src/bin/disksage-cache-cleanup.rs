//! Headless execution entry point for the narrow, observed cache policy.
//!
//! Without `--execute` this command is read-only. With it, the library path moves only inactive,
//! identity-bound children of the npm, pnpm, Adobe, Edge, uv, and Trivy cache roots to OS Trash.

use disksage_lib::cache_cleanup::{
    clean_catalog_cache_headless, clean_regenerable_caches_headless, plan_catalog_cache_headless,
    proven_cache_trash_candidates, prune_uv_cache_headless, purge_proven_cache_trash,
};
use std::ffi::OsString;
use std::path::PathBuf;

const USAGE: &str = "Usage: disksage-cache-cleanup [--execute] [--cache-id CATALOG_ID [--target-object-id OBJECT_ID] | --purge-proven-cache-trash | --prune-uv-cache] [--journal-path PATH]\n\
Without --execute it reports the command is a no-op. With --execute it moves only observed,\n\
inactive regenerable cache children to OS Trash. --purge-proven-cache-trash permanently removes\n\
only structurally proven cache directories already in OS Trash. --prune-uv-cache runs uv's native\n\
in-use-aware dangling cache prune without force. --cache-id plans or cleans one fixed catalog root.";

#[derive(Debug, PartialEq, Eq)]
struct Args {
    execute: bool,
    purge_proven_cache_trash: bool,
    prune_uv_cache: bool,
    cache_id: Option<String>,
    target_object_id: Option<String>,
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
    let mut prune_uv_cache = false;
    let mut cache_id = None;
    let mut target_object_id = None;
    let mut journal_path = default_journal_path()?;
    let mut args = first_arg.into_iter().chain(args);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--execute") => execute = true,
            Some("--purge-proven-cache-trash") => purge_proven_cache_trash = true,
            Some("--prune-uv-cache") => prune_uv_cache = true,
            Some("--cache-id") => {
                let value = args
                    .next()
                    .ok_or_else(|| "--cache-id requires CATALOG_ID".to_string())?
                    .into_string()
                    .map_err(|_| "--cache-id requires UTF-8 CATALOG_ID".to_string())?;
                if cache_id.replace(value).is_some() {
                    return Err("--cache-id may be supplied once".into());
                }
            }
            Some("--target-object-id") => {
                let value = args
                    .next()
                    .ok_or_else(|| "--target-object-id requires OBJECT_ID".to_string())?
                    .into_string()
                    .map_err(|_| "--target-object-id requires UTF-8 OBJECT_ID".to_string())?;
                if target_object_id.replace(value).is_some() {
                    return Err("--target-object-id may be supplied once".into());
                }
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
    if usize::from(purge_proven_cache_trash)
        + usize::from(prune_uv_cache)
        + usize::from(cache_id.is_some())
        > 1
    {
        return Err("cache cleanup actions are mutually exclusive".into());
    }
    if target_object_id.is_some() && cache_id.is_none() {
        return Err("--target-object-id requires --cache-id".into());
    }
    Ok(Some(Args {
        execute,
        purge_proven_cache_trash,
        prune_uv_cache,
        cache_id,
        target_object_id,
        journal_path,
    }))
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
        if let Some(cache_id) = args.cache_id.as_deref() {
            println!("{}", plan_catalog_cache_headless(cache_id)?);
            return Ok(());
        }
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
                "notice": "pass --execute to perform the guarded OS-Trash operation"
            })
        );
        return Ok(());
    }
    if let Some(parent) = args.journal_path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    if args.purge_proven_cache_trash {
        let results = purge_proven_cache_trash(&home_directory()?, &args.journal_path, now_ms())?;
        println!(
            "{}",
            serde_json::json!({
                "executed": true,
                "purge_proven_cache_trash": true,
                "journal_path": args.journal_path,
                "results": results
            })
        );
        return Ok(());
    }
    if args.prune_uv_cache {
        let result = prune_uv_cache_headless(&args.journal_path, now_ms())?;
        println!(
            "{}",
            serde_json::json!({
                "executed": true,
                "prune_uv_cache": true,
                "journal_path": args.journal_path,
                "result": result
            })
        );
        return Ok(());
    }
    if let Some(cache_id) = args.cache_id.as_deref() {
        let results = clean_catalog_cache_headless(
            cache_id,
            args.target_object_id.as_deref(),
            &args.journal_path,
            now_ms(),
        )?;
        println!(
            "{}",
            serde_json::json!({
                "executed": true,
                "cache_id": cache_id,
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
        assert!(args.purge_proven_cache_trash);
        assert!(!args.prune_uv_cache);
        assert!(args.cache_id.is_none());
        assert!(args.target_object_id.is_none());
    }

    #[test]
    fn uv_prune_is_explicit_and_exclusive() {
        let args = parse_args([OsString::from("--prune-uv-cache")])
            .unwrap()
            .unwrap();
        assert!(args.prune_uv_cache);
        assert!(parse_args([
            OsString::from("--prune-uv-cache"),
            OsString::from("--purge-proven-cache-trash"),
        ])
        .is_err());
    }

    #[test]
    fn catalog_cache_id_is_explicit_and_exclusive() {
        let args = parse_args([OsString::from("--cache-id"), OsString::from("os-temp")])
            .unwrap()
            .unwrap();
        assert_eq!(args.cache_id.as_deref(), Some("os-temp"));
        assert!(args.target_object_id.is_none());
        assert!(parse_args([
            OsString::from("--cache-id"),
            OsString::from("os-temp"),
            OsString::from("--prune-uv-cache"),
        ])
        .is_err());
        assert!(parse_args([
            OsString::from("--target-object-id"),
            OsString::from("unix:1:2"),
        ])
        .is_err());
        let selected = parse_args([
            OsString::from("--cache-id"),
            OsString::from("os-temp"),
            OsString::from("--target-object-id"),
            OsString::from("unix:1:2"),
        ])
        .unwrap()
        .unwrap();
        assert_eq!(selected.target_object_id.as_deref(), Some("unix:1:2"));
    }
}
