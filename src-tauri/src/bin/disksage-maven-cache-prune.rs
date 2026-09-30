//! Fingerprint-bound Maven cache pruning. Dry-run is the default.

use std::ffi::OsString;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use disksage_lib::maven_cache::{prune_maven_repository, MavenCachePruneReport};
use disksage_lib::private_evidence::{write_private_json_create_new, PrivateEvidenceReceipt};

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
    "usage: disksage-maven-cache-prune --repository-root ABSOLUTE_PATH --expected-candidate-set-fingerprint HEX [--apply] [--max-entries N] [--output NEW_ABSOLUTE_JSON_PATH]\n\
다음 단계: 먼저 --apply 없이 결과와 지문을 확인한 뒤, 일치하는 계획에만 --apply를 사용하세요."
}

fn native_value(args: &[OsString], index: &mut usize, flag: &str) -> Result<OsString, String> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| format!("{flag} 값이 필요함"))
}

fn text_value(args: &[OsString], index: &mut usize, flag: &str) -> Result<String, String> {
    native_value(args, index, flag)?
        .into_string()
        .map_err(|_| "알 수 없는 인자".to_string())
}

fn parse_args_os(args: &[OsString]) -> Result<Args, String> {
    let mut repository_root = None;
    let mut expected_candidate_set_fingerprint = None;
    let mut apply = false;
    let mut apply_seen = false;
    let mut max_entries = DEFAULT_MAX_ENTRIES;
    let mut max_entries_seen = false;
    let mut output = None;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].to_str() {
            Some("--repository-root") => {
                if repository_root.is_some() {
                    return Err("--repository-root는 한 번만 지정할 수 있음".into());
                }
                repository_root = Some(PathBuf::from(native_value(
                    args,
                    &mut index,
                    "--repository-root",
                )?));
            }
            Some("--expected-candidate-set-fingerprint") => {
                if expected_candidate_set_fingerprint.is_some() {
                    return Err(
                        "--expected-candidate-set-fingerprint는 한 번만 지정할 수 있음".into(),
                    );
                }
                expected_candidate_set_fingerprint = Some(text_value(
                    args,
                    &mut index,
                    "--expected-candidate-set-fingerprint",
                )?);
            }
            Some("--apply") => {
                if apply_seen {
                    return Err("--apply는 한 번만 지정할 수 있음".into());
                }
                apply_seen = true;
                apply = true;
            }
            Some("--max-entries") => {
                if max_entries_seen {
                    return Err("--max-entries는 한 번만 지정할 수 있음".into());
                }
                max_entries_seen = true;
                max_entries = text_value(args, &mut index, "--max-entries")?
                    .parse()
                    .map_err(|_| "--max-entries는 정수여야 함".to_string())?;
            }
            Some("--output") => {
                if output.is_some() {
                    return Err("--output은 한 번만 지정할 수 있음".into());
                }
                output = Some(PathBuf::from(native_value(args, &mut index, "--output")?));
            }
            Some("--help" | "-h") => return Err(usage().into()),
            Some(_) | None => return Err("알 수 없는 인자".to_string()),
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
    if !(1..=DEFAULT_MAX_ENTRIES).contains(&max_entries) {
        return Err(format!(
            "--max-entries는 1..={DEFAULT_MAX_ENTRIES} 범위여야 함"
        ));
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

#[cfg(test)]
fn parse_args(args: &[String]) -> Result<Args, String> {
    let native = args.iter().map(OsString::from).collect::<Vec<_>>();
    parse_args_os(&native)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|value| u64::try_from(value.as_millis()).ok())
        .unwrap_or(0)
}

fn output_summary(
    receipt: &PrivateEvidenceReceipt,
    report: &MavenCachePruneReport,
) -> Result<String, String> {
    serde_json::to_string(&serde_json::json!({
        "schema_kind": report.schema_kind,
        "private_output": receipt,
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
    let raw = std::env::args_os().skip(1).collect::<Vec<_>>();
    if raw.len() == 1 && matches!(raw[0].to_str(), Some("--help" | "-h")) {
        println!("{}", usage());
        return Ok(());
    }
    let args = parse_args_os(&raw)?;
    let report = prune_maven_repository(
        &args.repository_root,
        &args.expected_candidate_set_fingerprint,
        args.apply,
        args.max_entries,
        now_ms(),
    )?;
    if let Some(output) = &args.output {
        let receipt = write_private_json_create_new(&args.repository_root, output, &report)?;
        println!("{}", output_summary(&receipt, &report)?);
    } else {
        let encoded = serde_json::to_vec_pretty(&report).map_err(|error| error.to_string())?;
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
    fn output_summary_exposes_only_private_evidence_commitment() {
        let receipt = PrivateEvidenceReceipt {
            written: true,
            sha256: "b".repeat(64),
            bytes: 321,
            unix_mode: "0600".into(),
            create_new: true,
            contains_sensitive_local_paths: true,
            is_approval: false,
        };
        let report = MavenCachePruneReport {
            schema_kind: "disksage.maven-cache-prune/v1".into(),
            repository_root: "/private/repository".into(),
            generated_at_ms: 1,
            expected_candidate_set_fingerprint: "a".repeat(64),
            observed_candidate_set_fingerprint: "a".repeat(64),
            candidate_directories: 0,
            candidate_bytes: 0,
            removed_directories: 0,
            removed_bytes: 0,
            skipped_directories: 0,
            skip_reason_counts: Default::default(),
            apply_requested: false,
            filesystem_mutation_executed: false,
            complete: true,
        };

        let encoded = output_summary(&receipt, &report).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&encoded).unwrap();
        assert_eq!(parsed["private_output"]["written"], true);
        assert_eq!(parsed["private_output"]["sha256"], "b".repeat(64));
        assert!(parsed.get("output").is_none());
        assert_eq!(parsed["filesystem_mutation_executed"], false);
    }
}
