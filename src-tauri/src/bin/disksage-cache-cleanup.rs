//! Headless execution entry point for the narrow, observed cache policy.
//!
//! Without `--execute` this command is read-only. With it, the library path moves only inactive,
//! identity-bound children of the npm, pnpm, Adobe, Edge, uv, and Trivy cache roots to OS Trash.

use disksage_lib::cache_cleanup::{clean_regenerable_caches_headless, proven_cache_trash_snapshot};
use std::ffi::OsString;
use std::path::PathBuf;

const PERMANENT_CACHE_TRASH_DELETE_UNAVAILABLE: &str =
    "cache-trash-identity-bound-permanent-delete-unavailable";
const USAGE: &str = "Usage: disksage-cache-cleanup [--execute] [--purge-proven-cache-trash] [--journal-path PATH]\n\
Without --execute it reports the command is a no-op. With --execute it moves only observed,\n\
inactive regenerable cache children to OS Trash. --purge-proven-cache-trash is read-only evidence;\n\
permanent in-app deletion remains unavailable until the final syscall is object-bound.";

fn read_only_notice(purge_proven_cache_trash: bool) -> &'static str {
    if purge_proven_cache_trash {
        "proven cache-Trash review is read-only; empty the native Trash manually to reclaim space; --execute cannot enable permanent deletion"
    } else {
        "pass --execute to move guarded cache children to OS Trash"
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Args {
    execute: bool,
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
    let mut purge_proven_cache_trash = false;
    let mut journal_path = default_journal_path()?;
    let mut seen_execute = false;
    let mut seen_purge_proven_cache_trash = false;
    let mut seen_journal_path = false;
    let mut args = first_arg.into_iter().chain(args);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--execute") => {
                if seen_execute {
                    return Err("--execute may be supplied once".into());
                }
                seen_execute = true;
                execute = true;
            }
            Some("--purge-proven-cache-trash") => {
                if seen_purge_proven_cache_trash {
                    return Err("--purge-proven-cache-trash may be supplied once".into());
                }
                seen_purge_proven_cache_trash = true;
                purge_proven_cache_trash = true;
            }
            Some("--journal-path") => {
                if seen_journal_path {
                    return Err("--journal-path may be supplied once".into());
                }
                seen_journal_path = true;
                journal_path = PathBuf::from(
                    args.next()
                        .ok_or_else(|| "--journal-path requires PATH".to_string())?,
                );
                if !journal_path.is_absolute() {
                    return Err("--journal-path must be absolute".into());
                }
            }
            Some("-h" | "--help") => return Err(format!("--help must be used alone\n{USAGE}")),
            Some(_) => return Err(format!("cache-cleanup-invalid-argument\n{USAGE}")),
            None => return Err(format!("invalid UTF-8 option\n{USAGE}")),
        }
    }
    Ok(Some(Args {
        execute,
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

fn run_with_args(raw_args: impl IntoIterator<Item = OsString>) -> Result<(), String> {
    let Some(args) = parse_args(raw_args)? else {
        println!("{USAGE}");
        return Ok(());
    };
    if !args.execute {
        let notice = read_only_notice(args.purge_proven_cache_trash);
        let (cache_trash, cache_trash_snapshot) = if args.purge_proven_cache_trash {
            let snapshot = proven_cache_trash_snapshot(&home_directory()?);
            let candidates =
                serde_json::to_value(&snapshot.candidates).map_err(|error| error.to_string())?;
            let snapshot = serde_json::to_value(snapshot).map_err(|error| error.to_string())?;
            (candidates, snapshot)
        } else {
            (
                serde_json::Value::Array(Vec::new()),
                serde_json::Value::Null,
            )
        };
        println!(
            "{}",
            serde_json::json!({
                "executed": false,
                "journal_path": args.journal_path,
                "purge_proven_cache_trash": args.purge_proven_cache_trash,
                "proven_cache_trash": cache_trash,
                "proven_cache_trash_snapshot": cache_trash_snapshot,
                "notice": notice
            })
        );
        return Ok(());
    }
    if args.purge_proven_cache_trash {
        return Err(PERMANENT_CACHE_TRASH_DELETE_UNAVAILABLE.into());
    }
    if let Some(parent) = args.journal_path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
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
    fn duplicate_authority_singletons_are_rejected() {
        let duplicate_execute = parse_args([
            OsString::from("--execute"),
            OsString::from("--execute"),
        ])
        .unwrap_err();
        assert_eq!(duplicate_execute, "--execute may be supplied once");

        let duplicate_purge = parse_args([
            OsString::from("--purge-proven-cache-trash"),
            OsString::from("--purge-proven-cache-trash"),
        ])
        .unwrap_err();
        assert_eq!(
            duplicate_purge,
            "--purge-proven-cache-trash may be supplied once"
        );
    }

    #[test]
    fn unknown_argument_is_not_reflected() {
        let payload = "--unknown-with-sensitive-value";
        let error = parse_args([OsString::from(payload)]).unwrap_err();
        assert!(error.contains("cache-cleanup-invalid-argument"));
        assert!(!error.contains(payload));
    }

    #[test]
    fn purge_cache_trash_flag_is_explicit() {
        let args = parse_args([OsString::from("--purge-proven-cache-trash")])
            .unwrap()
            .unwrap();
        assert!(!args.execute);
        assert!(args.purge_proven_cache_trash);
    }

    #[test]
    fn read_only_notice_matches_the_requested_action() {
        assert!(read_only_notice(false).contains("pass --execute"));
        assert!(read_only_notice(true).contains("read-only"));
        assert!(read_only_notice(true).contains("empty the native Trash"));
        assert!(!read_only_notice(true).contains("pass --execute to move"));
    }
}
