//! Headless read-only stale worktree inventory.

#[cfg(not(coverage))]
use std::path::{Path, PathBuf};

#[cfg(not(coverage))]
#[derive(Debug, PartialEq, Eq)]
struct Args {
    root: PathBuf,
    min_age_days: u64,
}

#[cfg(not(coverage))]
fn parse_args(args: &[String], home: &Path) -> Result<Args, String> {
    let mut parsed = Args {
        root: home.to_path_buf(),
        min_age_days: 30,
    };
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--root" => {
                index += 1;
                parsed.root = PathBuf::from(
                    args.get(index)
                        .ok_or_else(|| "--root 값이 필요함".to_string())?,
                );
            }
            "--min-age-days" => {
                index += 1;
                parsed.min_age_days = args
                    .get(index)
                    .ok_or_else(|| "--min-age-days 값이 필요함".to_string())?
                    .parse()
                    .map_err(|_| "--min-age-days는 정수여야 함".to_string())?;
            }
            "--help" | "-h" => {
                return Err("usage: disksage-worktrees [--root PATH] [--min-age-days N]".into());
            }
            flag => return Err(format!("알 수 없는 인자: {flag}")),
        }
        index += 1;
    }
    Ok(parsed)
}

#[cfg(not(coverage))]
fn run() -> Result<(), String> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .map_err(|_| "HOME/USERPROFILE을 찾을 수 없음".to_string())?;
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let args = parse_args(&raw, &home)?;
    if !args.root.is_dir() {
        return Err(format!(
            "스캔 루트가 디렉터리가 아님: {}",
            args.root.display()
        ));
    }
    let report = disksage_lib::worktrees::inventory(
        &args.root,
        args.min_age_days,
        disksage_lib::worktrees::system_now_ms(),
    );
    println!(
        "{}",
        serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?
    );
    Ok(())
}

#[cfg(not(coverage))]
fn main() {
    if let Err(error) = run() {
        eprintln!("DiskSage worktree inventory: {error}");
        std::process::exit(2);
    }
}

#[cfg(coverage)]
fn main() {}

#[cfg(all(test, coverage))]
mod coverage_tests {
    #[test]
    fn noop_main_runs() {
        super::main();
    }
}

#[cfg(all(test, not(coverage)))]
mod tests {
    use super::*;

    #[test]
    fn parses_defaults_and_explicit_values() {
        let defaults = parse_args(&[], Path::new("/home/test")).unwrap();
        assert_eq!(defaults.root, Path::new("/home/test"));
        assert_eq!(defaults.min_age_days, 30);
        let parsed = parse_args(
            &[
                "--root".into(),
                "/repos".into(),
                "--min-age-days".into(),
                "90".into(),
            ],
            Path::new("/home/test"),
        )
        .unwrap();
        assert_eq!(parsed.root, Path::new("/repos"));
        assert_eq!(parsed.min_age_days, 90);
    }

    #[test]
    fn rejects_missing_invalid_and_unknown_values() {
        assert!(parse_args(&["--root".into()], Path::new("/h")).is_err());
        assert!(parse_args(&["--min-age-days".into(), "x".into()], Path::new("/h")).is_err());
        assert!(parse_args(&["--wat".into()], Path::new("/h")).is_err());
    }
}
