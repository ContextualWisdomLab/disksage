//! Fingerprint-bound Maven cache pruning. Dry-run is the default.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use disksage_lib::maven_cache::{prune_maven_repository, MavenCachePruneReport};

const DEFAULT_MAX_ENTRIES: u64 = 2_000_000;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    repository_root: PathBuf,
    expected_candidate_set_fingerprint: String,
    apply: bool,
    max_entries: u64,
    output: Option<PathBuf>,
}

fn usage() -> &'static str {
    "usage: disksage-maven-cache-prune --repository-root ABSOLUTE_PATH --expected-candidate-set-fingerprint HEX [--apply] [--max-entries N] [--output NEW_ABSOLUTE_JSON_PATH]"
}

fn value(args: &[String], index: &mut usize, flag: &str) -> Result<String, String> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| format!("{flag} 값이 필요함"))
}

fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut repository_root = None;
    let mut expected_candidate_set_fingerprint = None;
    let mut apply = false;
    let mut max_entries = DEFAULT_MAX_ENTRIES;
    let mut output = None;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--repository-root" => {
                repository_root = Some(PathBuf::from(value(args, &mut index, "--repository-root")?))
            }
            "--expected-candidate-set-fingerprint" => {
                expected_candidate_set_fingerprint = Some(value(
                    args,
                    &mut index,
                    "--expected-candidate-set-fingerprint",
                )?)
            }
            "--apply" => apply = true,
            "--max-entries" => {
                max_entries = value(args, &mut index, "--max-entries")?
                    .parse()
                    .map_err(|_| "--max-entries는 정수여야 함".to_string())?
            }
            "--output" => output = Some(PathBuf::from(value(args, &mut index, "--output")?)),
            "--help" | "-h" => return Err(usage().into()),
            unknown => return Err(format!("알 수 없는 인자: {unknown}")),
        }
        index += 1;
    }

    let repository_root =
        repository_root.ok_or_else(|| "--repository-root 값이 필요함".to_string())?;
    if !repository_root.is_absolute() {
        return Err("--repository-root는 절대 경로여야 함".into());
    }
    let expected_candidate_set_fingerprint = expected_candidate_set_fingerprint
        .ok_or_else(|| "--expected-candidate-set-fingerprint 값이 필요함".to_string())?;
    if max_entries == 0 {
        return Err("--max-entries는 1 이상이어야 함".into());
    }
    if output.as_ref().is_some_and(|path| !path.is_absolute()) {
        return Err("--output은 절대 경로여야 함".into());
    }

    Ok(Args {
        repository_root,
        expected_candidate_set_fingerprint,
        apply,
        max_entries,
        output,
    })
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|value| u64::try_from(value.as_millis()).ok())
        .unwrap_or(0)
}

fn write_new_private_json(path: &PathBuf, encoded: &[u8]) -> Result<(), String> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|_| "maven-cache-prune-output-create-failed".to_string())?;
    file.write_all(encoded)
        .map_err(|_| "maven-cache-prune-output-write-failed".to_string())?;
    file.sync_all()
        .map_err(|_| "maven-cache-prune-output-sync-failed".to_string())
}

fn output_summary(path: &PathBuf, report: &MavenCachePruneReport) -> Result<String, String> {
    serde_json::to_string(&serde_json::json!({
        "schema_kind": report.schema_kind,
        "output": path.to_string_lossy(),
        "observed_candidate_set_fingerprint": report.observed_candidate_set_fingerprint,
        "candidate_directories": report.candidate_directories,
        "candidate_bytes": report.candidate_bytes,
        "removed_directories": report.removed_directories,
        "removed_bytes": report.removed_bytes,
        "skipped_directories": report.skipped_directories,
        "apply_requested": report.apply_requested,
        "filesystem_mutation_executed": report.filesystem_mutation_executed,
        "complete": report.complete,
    }))
    .map_err(|error| error.to_string())
}

fn run() -> Result<(), String> {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let args = parse_args(&raw)?;
    let report = prune_maven_repository(
        &args.repository_root,
        &args.expected_candidate_set_fingerprint,
        args.apply,
        args.max_entries,
        now_ms(),
    )?;
    let encoded = serde_json::to_vec_pretty(&report).map_err(|error| error.to_string())?;
    if let Some(output) = &args.output {
        write_new_private_json(output, &encoded)?;
        println!("{}", output_summary(output, &report)?);
    } else {
        println!("{}", String::from_utf8_lossy(&encoded));
    }
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
    fn parser_defaults_to_dry_run_and_requires_exact_inputs() {
        let args = parse_args(&[
            "--repository-root".into(),
            "/Users/example/.m2/repository".into(),
            "--expected-candidate-set-fingerprint".into(),
            "a".repeat(64),
        ])
        .unwrap();
        assert!(!args.apply);
        assert_eq!(args.max_entries, DEFAULT_MAX_ENTRIES);

        let applied = parse_args(&[
            "--repository-root".into(),
            "/Users/example/.m2/repository".into(),
            "--expected-candidate-set-fingerprint".into(),
            "a".repeat(64),
            "--apply".into(),
            "--max-entries".into(),
            "1000".into(),
            "--output".into(),
            "/tmp/result.json".into(),
        ])
        .unwrap();
        assert!(applied.apply);
        assert_eq!(applied.max_entries, 1000);
        assert_eq!(applied.output, Some(PathBuf::from("/tmp/result.json")));

        assert!(parse_args(&[]).is_err());
        assert!(parse_args(&[
            "--repository-root".into(),
            "relative".into(),
            "--expected-candidate-set-fingerprint".into(),
            "a".repeat(64),
        ])
        .is_err());
        assert!(parse_args(&[
            "--repository-root".into(),
            "/tmp/repository".into(),
            "--expected-candidate-set-fingerprint".into(),
            "a".repeat(64),
            "--max-entries".into(),
            "0".into(),
        ])
        .is_err());
    }

    #[test]
    fn private_output_is_create_new_and_not_overwritten() {
        let tmp = tempfile::tempdir().unwrap();
        let output = tmp.path().join("prune.json");

        write_new_private_json(&output, b"{\"first\":true}").unwrap();
        assert_eq!(std::fs::read(&output).unwrap(), b"{\"first\":true}");
        assert!(write_new_private_json(&output, b"{\"second\":true}").is_err());
        assert_eq!(std::fs::read(&output).unwrap(), b"{\"first\":true}");
    }
}
