use disksage_lib::{cloud, stale_git_clone};
use std::path::PathBuf;

const USAGE: &str = "usage: disksage-stale-git-clone (--repository-root ABSOLUTE_PATH | --scan-root ABSOLUTE_PATH [--max-depth 1..16] [--concurrency 1..32] [--max-repositories N]) [--open-age-days N] [--apply --plan-fingerprint HEX --confirmation-phrase PHRASE --rationale TEXT]";
const REMOVAL_UNAVAILABLE: &str = "stale-git-clone-removal-identity-bound-trash-unavailable";

#[derive(Debug, PartialEq, Eq)]
struct Args {
    repository_root: PathBuf,
    scan_root: Option<PathBuf>,
    concurrency: usize,
    max_repositories: usize,
    max_depth: usize,
    open_age_days: u64,
    apply: bool,
}

fn value(args: &[String], index: &mut usize, flag: &str) -> Result<String, String> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut repository_root = None;
    let mut scan_root = None;
    let mut concurrency = 8;
    let mut max_repositories = 10_000;
    let mut max_depth = 1;
    let mut open_age_days = 90;
    let mut apply = false;
    let mut plan_fingerprint = None;
    let mut confirmation_phrase = None;
    let mut rationale = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--repository-root" => {
                repository_root = Some(PathBuf::from(value(args, &mut index, "--repository-root")?))
            }
            "--scan-root" => {
                scan_root = Some(PathBuf::from(value(args, &mut index, "--scan-root")?))
            }
            "--concurrency" => {
                concurrency = value(args, &mut index, "--concurrency")?
                    .parse()
                    .map_err(|_| USAGE.to_string())?
            }
            "--max-repositories" => {
                max_repositories = value(args, &mut index, "--max-repositories")?
                    .parse()
                    .map_err(|_| USAGE.to_string())?
            }
            "--max-depth" => {
                max_depth = value(args, &mut index, "--max-depth")?
                    .parse()
                    .map_err(|_| USAGE.to_string())?
            }
            "--open-age-days" => {
                open_age_days = value(args, &mut index, "--open-age-days")?
                    .parse()
                    .map_err(|_| "--open-age-days requires an integer".to_string())?
            }
            "--apply" => apply = true,
            "--plan-fingerprint" => {
                plan_fingerprint = Some(value(args, &mut index, "--plan-fingerprint")?)
            }
            "--confirmation-phrase" => {
                confirmation_phrase = Some(value(args, &mut index, "--confirmation-phrase")?)
            }
            "--rationale" => rationale = Some(value(args, &mut index, "--rationale")?),
            "--help" | "-h" => return Err(USAGE.into()),
            _ => return Err(USAGE.into()),
        }
        index += 1;
    }
    if repository_root.is_some() == scan_root.is_some() {
        return Err("exactly one of --repository-root or --scan-root is required".into());
    }
    let repository_root = repository_root.or_else(|| scan_root.clone()).unwrap();
    if !repository_root.is_absolute() || scan_root.as_ref().is_some_and(|path| !path.is_absolute())
    {
        return Err("repository and scan roots must be absolute".into());
    }
    if scan_root.is_some() && apply {
        return Err("batch scan is read-only".into());
    }
    let execution_values =
        plan_fingerprint.is_some() || confirmation_phrase.is_some() || rationale.is_some();
    if apply
        && !(plan_fingerprint.is_some() && confirmation_phrase.is_some() && rationale.is_some())
    {
        return Err("--apply requires fingerprint, confirmation phrase, and rationale".into());
    }
    if !apply && execution_values {
        return Err("execution arguments require --apply".into());
    }
    Ok(Args {
        repository_root,
        scan_root,
        concurrency,
        max_repositories,
        max_depth,
        open_age_days,
        apply,
    })
}

fn run() -> Result<(), String> {
    let args = parse_args(&std::env::args().skip(1).collect::<Vec<_>>())?;
    if args.apply {
        return Err(REMOVAL_UNAVAILABLE.into());
    }
    let now_ms = cloud::system_now_ms();
    let json = if let Some(scan_root) = &args.scan_root {
        serde_json::to_value(stale_git_clone::plan_stale_git_clones(
            scan_root,
            args.open_age_days,
            now_ms,
            args.max_repositories,
            args.concurrency,
            args.max_depth,
        )?)
    } else {
        serde_json::to_value(stale_git_clone::plan_stale_git_clone(
            &args.repository_root,
            args.open_age_days,
            now_ms,
        )?)
    }
    .map_err(|error| error.to_string())?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json).map_err(|error| error.to_string())?
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("disksage-stale-git-clone: {error}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_is_dry_run_by_default_and_rejects_partial_execution_authority() {
        let args = parse_args(&["--repository-root".into(), "/tmp/repo".into()]).unwrap();
        assert!(!args.apply);
        assert_eq!(args.open_age_days, 90);
        assert!(args.scan_root.is_none());
        assert!(parse_args(&[
            "--repository-root".into(),
            "/tmp/repo".into(),
            "--apply".into(),
            "--rationale".into(),
            "reviewed".into(),
        ])
        .is_err());
    }
}
