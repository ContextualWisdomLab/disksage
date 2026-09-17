//! Read-only Maven local-repository provenance audit. This command never removes artifacts.

use std::ffi::OsString;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use disksage_lib::maven_cache::{
    audit_maven_repository, MavenCacheAuditOptions, MavenCacheAuditReport,
};
use disksage_lib::private_evidence::{write_private_json_create_new, PrivateEvidenceReceipt};

const MAX_MAVEN_CACHE_ENTRIES: u64 = 2_000_000;
const MAX_MAVEN_CACHE_OUTPUT_ITEMS: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    repository_root: PathBuf,
    output: Option<PathBuf>,
    max_entries: u64,
    max_candidates: usize,
    max_issues: usize,
}

fn usage() -> &'static str {
    "usage: disksage-maven-cache-audit --repository-root ABSOLUTE_PATH [--output NEW_ABSOLUTE_JSON_PATH] [--max-entries N] [--max-candidates N] [--max-issues N]\n\
다음 단계: 후보와 후보 집합 지문을 검토하세요. 이 명령은 캐시를 제거하지 않습니다."
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

fn number<T: std::str::FromStr>(
    args: &[OsString],
    index: &mut usize,
    flag: &str,
) -> Result<T, String> {
    text_value(args, index, flag)?
        .parse()
        .map_err(|_| format!("{flag}는 정수여야 함"))
}

fn parse_args_os(args: &[OsString]) -> Result<Args, String> {
    let defaults = MavenCacheAuditOptions::default();
    let mut repository_root = None;
    let mut output = None;
    let mut max_entries = defaults.max_entries;
    let mut max_entries_seen = false;
    let mut max_candidates = defaults.max_candidates;
    let mut max_candidates_seen = false;
    let mut max_issues = defaults.max_issues;
    let mut max_issues_seen = false;
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
            Some("--output") => {
                if output.is_some() {
                    return Err("--output은 한 번만 지정할 수 있음".into());
                }
                output = Some(PathBuf::from(native_value(args, &mut index, "--output")?));
            }
            Some("--max-entries") => {
                if max_entries_seen {
                    return Err("--max-entries는 한 번만 지정할 수 있음".into());
                }
                max_entries_seen = true;
                max_entries = number(args, &mut index, "--max-entries")?;
            }
            Some("--max-candidates") => {
                if max_candidates_seen {
                    return Err("--max-candidates는 한 번만 지정할 수 있음".into());
                }
                max_candidates_seen = true;
                max_candidates = number(args, &mut index, "--max-candidates")?;
            }
            Some("--max-issues") => {
                if max_issues_seen {
                    return Err("--max-issues는 한 번만 지정할 수 있음".into());
                }
                max_issues_seen = true;
                max_issues = number(args, &mut index, "--max-issues")?;
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
    if output.as_ref().is_some_and(|path| !path.is_absolute()) {
        return Err("--output은 절대 경로여야 함".into());
    }
    if !(1..=MAX_MAVEN_CACHE_ENTRIES).contains(&max_entries) {
        return Err(format!(
            "--max-entries는 1..={MAX_MAVEN_CACHE_ENTRIES} 범위여야 함"
        ));
    }
    if max_candidates > MAX_MAVEN_CACHE_OUTPUT_ITEMS {
        return Err(format!(
            "--max-candidates는 0..={MAX_MAVEN_CACHE_OUTPUT_ITEMS} 범위여야 함"
        ));
    }
    if max_issues > MAX_MAVEN_CACHE_OUTPUT_ITEMS {
        return Err(format!(
            "--max-issues는 0..={MAX_MAVEN_CACHE_OUTPUT_ITEMS} 범위여야 함"
        ));
    }
    Ok(Args {
        repository_root,
        output,
        max_entries,
        max_candidates,
        max_issues,
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

fn report(args: &Args) -> Result<MavenCacheAuditReport, String> {
    audit_maven_repository(
        &args.repository_root,
        MavenCacheAuditOptions {
            max_entries: args.max_entries,
            max_candidates: args.max_candidates,
            max_issues: args.max_issues,
        },
        now_ms(),
    )
}

fn output_summary(
    receipt: &PrivateEvidenceReceipt,
    report: &MavenCacheAuditReport,
) -> Result<String, String> {
    serde_json::to_string(&serde_json::json!({
        "schema_kind": report.schema_kind,
        "private_output": receipt,
        "candidate_set_fingerprint": report.candidate_set_fingerprint,
        "remote_recoverable_directories": report.remote_recoverable_directories,
        "remote_recoverable_bytes": report.remote_recoverable_bytes,
        "held_directories": report.held_directories,
        "scan_truncated": report.scan_truncated,
        "candidate_output_truncated": report.candidate_output_truncated,
        "truncated": report.truncated,
        "provider_write_executed": report.provider_write_executed,
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
    let report = report(&args)?;
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
    fn parser_requires_absolute_root_and_accepts_bounds() {
        assert_eq!(
            parse_args(&[
                "--repository-root".into(),
                "/Users/example/.m2/repository".into(),
                "--max-entries".into(),
                "1000".into(),
                "--max-candidates".into(),
                "20".into(),
                "--max-issues".into(),
                "10".into(),
            ])
            .unwrap(),
            Args {
                repository_root: PathBuf::from("/Users/example/.m2/repository"),
                output: None,
                max_entries: 1000,
                max_candidates: 20,
                max_issues: 10,
            }
        );
        assert!(parse_args(&[]).is_err());
        assert!(parse_args(&["--repository-root".into(), "relative".into()]).is_err());
        assert!(parse_args(&[
            "--repository-root".into(),
            "/tmp/repository".into(),
            "--output".into(),
            "relative.json".into(),
        ])
        .is_err());
        assert!(parse_args(&[
            "--repository-root".into(),
            "/tmp/repository".into(),
            "--max-entries".into(),
            "0".into(),
        ])
        .is_err());
        assert!(parse_args(&["--unknown".into()]).is_err());
    }

    #[test]
    fn output_summary_exposes_only_private_evidence_commitment() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("repository");
        std::fs::create_dir(&root).unwrap();
        let report = audit_maven_repository(&root, MavenCacheAuditOptions::default(), 123).unwrap();
        let receipt = PrivateEvidenceReceipt {
            written: true,
            sha256: "a".repeat(64),
            bytes: 123,
            unix_mode: "0600".into(),
            create_new: true,
            contains_sensitive_local_paths: true,
            is_approval: false,
        };

        let encoded = output_summary(&receipt, &report).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&encoded).unwrap();

        assert_eq!(parsed["private_output"]["written"], true);
        assert_eq!(parsed["private_output"]["sha256"], "a".repeat(64));
        assert_eq!(parsed["private_output"]["unix_mode"], "0600");
        assert!(parsed.get("output").is_none());
        assert_eq!(parsed["provider_write_executed"], false);
    }
}
