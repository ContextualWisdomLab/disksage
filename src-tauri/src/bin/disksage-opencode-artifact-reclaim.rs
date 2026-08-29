//! Headless read-only planner for exact unreferenced OpenCode tool-output artifacts.
//!
//! Mutation is deliberately unavailable until the library has replacement-resistant Trash
//! identity and durable authenticated purge lineage. This CLI therefore exposes only planning.

use disksage_lib::opencode_artifact_reclaim;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

const USAGE: &str = "usage: disksage-opencode-artifact-reclaim [--home ABSOLUTE_PATH]";
const MUTATION_UNAVAILABLE: &str = "OpenCode artifact mutation is unavailable";

#[derive(Debug)]
struct Args {
    home: PathBuf,
}

fn validate_home_authority(process_home: &Path, requested_home: &Path) -> Result<PathBuf, String> {
    if !requested_home.is_absolute() {
        return Err("--home must be absolute".into());
    }
    let process_home =
        fs::canonicalize(process_home).map_err(|_| "HOME unavailable".to_string())?;
    let requested_home = fs::canonicalize(requested_home)
        .map_err(|_| "--home must match process HOME".to_string())?;
    if requested_home != process_home {
        return Err("--home must match process HOME".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata =
            fs::symlink_metadata(&requested_home).map_err(|_| "HOME unavailable".to_string())?;
        if !metadata.is_dir() || metadata.uid() != unsafe { libc::geteuid() } {
            return Err("HOME must be owned by current user".into());
        }
    }
    Ok(requested_home)
}

fn parse<I, S>(raw: I) -> Result<Args, String>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let raw = raw
        .into_iter()
        .map(|value| {
            value
                .into()
                .into_string()
                .map_err(|_| "argument must be valid UTF-8".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let process_home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "HOME unavailable".to_string())?;
    let mut requested_home = process_home.clone();
    let mut index = 0;
    while index < raw.len() {
        let flag = &raw[index];
        if flag == "--execute" || flag == "--purge-quarantined" {
            return Err(MUTATION_UNAVAILABLE.into());
        }
        index += 1;
        let value = raw
            .get(index)
            .ok_or_else(|| format!("{flag} requires a value"))?;
        match flag.as_str() {
            "--home" => requested_home = PathBuf::from(value),
            _ => return Err(format!("unknown option: {flag}")),
        }
        index += 1;
    }
    Ok(Args {
        home: validate_home_authority(&process_home, &requested_home)?,
    })
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

fn run() -> Result<(), String> {
    let args = parse(std::env::args_os().skip(1))?;
    let plan = opencode_artifact_reclaim::plan(&args.home, now_ms())?;
    println!(
        "{}",
        serde_json::to_string_pretty(&plan).map_err(|_| "plan serialization failed".to_string())?
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("disksage-opencode-artifact-reclaim: {error}\n{USAGE}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mutation_flags_are_rejected_before_domain_work() {
        assert_eq!(parse(["--execute"]).unwrap_err(), MUTATION_UNAVAILABLE);
        assert_eq!(
            parse(["--purge-quarantined"]).unwrap_err(),
            MUTATION_UNAVAILABLE
        );
    }

    #[test]
    fn explicit_home_must_match_process_home() {
        let process_home = std::env::var_os("HOME").expect("test process HOME");
        let alternate_home = tempfile::tempdir().unwrap();
        assert_ne!(alternate_home.path(), PathBuf::from(process_home));
        let error = parse([
            "--home".to_string(),
            alternate_home.path().to_string_lossy().into_owned(),
        ])
        .unwrap_err();
        assert_eq!(error, "--home must match process HOME");
    }

    #[test]
    fn disabled_mutations_are_not_advertised_or_accepted() {
        assert!(!USAGE.contains("--execute"));
        assert!(!USAGE.contains("--purge-quarantined"));
        assert_eq!(parse(["--execute"]).unwrap_err(), MUTATION_UNAVAILABLE);
        assert_eq!(
            parse(["--purge-quarantined"]).unwrap_err(),
            MUTATION_UNAVAILABLE
        );
    }
}
