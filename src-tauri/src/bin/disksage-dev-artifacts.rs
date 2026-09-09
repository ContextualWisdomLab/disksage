//! Bounded, identity-checked development-artifact inventory and cleanup.
//!
//! The default operation is read-only. Execution requires the exact selection-bound phrase and
//! review timestamp emitted by a prior read-only plan; every requested artifact is then re-scanned
//! before being moved to OS Trash.

use disksage_lib::dev_artifact_approval::{
    clean_artifacts_with_approval, review_selection, selection_fingerprint, DevArtifactApproval,
    MAX_REVIEW_AGE_MS,
};
use disksage_lib::dev_artifacts::{find_artifacts, DevArtifactCleanResult};
use std::path::{Component, Path, PathBuf};

const MAX_AGE_DAYS: u64 = 3_650;
const USAGE: &str = "usage: disksage-dev-artifacts --root ABSOLUTE_PATH [--min-age-days N] [--journal-path ABSOLUTE_PATH] [--execute --approval-phrase EXACT_PHRASE --approved-at-ms EPOCH_MS]";

#[derive(Debug, PartialEq, Eq)]
struct Args {
    root: PathBuf,
    min_age_days: u64,
    journal_path: PathBuf,
    execute: bool,
    approval_phrase: Option<String>,
    approved_at_ms: Option<u64>,
}

fn absolute_without_parent(path: &Path) -> bool {
    path.is_absolute()
        && !path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
}

fn default_journal_path() -> Result<PathBuf, String> {
    #[cfg(not(target_os = "windows"))]
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|path| absolute_without_parent(path))
        .ok_or_else(|| "home-directory-unavailable".to_string())?;
    #[cfg(target_os = "macos")]
    let path = home
        .join("Library")
        .join("Application Support")
        .join("com.contextualwisdomlab.disksage")
        .join("journal.jsonl");
    #[cfg(target_os = "windows")]
    let path = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .filter(|value| absolute_without_parent(value))
        .ok_or_else(|| "app-data-directory-unavailable".to_string())?
        .join("com.contextualwisdomlab.disksage")
        .join("journal.jsonl");
    #[cfg(all(unix, not(target_os = "macos")))]
    let path = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|value| absolute_without_parent(value))
        .unwrap_or_else(|| home.join(".local").join("share"))
        .join("com.contextualwisdomlab.disksage")
        .join("journal.jsonl");
    Ok(path)
}

fn parse_args(raw: &[String]) -> Result<Args, String> {
    let mut root = None;
    let mut min_age_days = 30;
    let mut journal_path = default_journal_path()?;
    let mut execute = false;
    let mut approval_phrase = None;
    let mut approved_at_ms = None;
    let mut index = 0usize;
    while index < raw.len() {
        match raw[index].as_str() {
            "--root" => {
                index += 1;
                root = Some(PathBuf::from(
                    raw.get(index)
                        .ok_or_else(|| "--root 값이 필요함".to_string())?,
                ));
            }
            "--min-age-days" => {
                index += 1;
                let value = raw
                    .get(index)
                    .ok_or_else(|| "--min-age-days 값이 필요함".to_string())?;
                min_age_days = value
                    .parse::<u64>()
                    .map_err(|_| "--min-age-days는 정수여야 함".to_string())?;
                if min_age_days > MAX_AGE_DAYS {
                    return Err(format!("--min-age-days는 {MAX_AGE_DAYS} 이하이어야 함"));
                }
            }
            "--journal-path" => {
                index += 1;
                journal_path = PathBuf::from(
                    raw.get(index)
                        .ok_or_else(|| "--journal-path 값이 필요함".to_string())?,
                );
            }
            "--execute" => execute = true,
            "--approval-phrase" => {
                index += 1;
                approval_phrase = Some(
                    raw.get(index)
                        .ok_or_else(|| "--approval-phrase 값이 필요함".to_string())?
                        .clone(),
                );
            }
            "--approved-at-ms" => {
                index += 1;
                approved_at_ms = Some(
                    raw.get(index)
                        .ok_or_else(|| "--approved-at-ms 값이 필요함".to_string())?
                        .parse::<u64>()
                        .map_err(|_| "--approved-at-ms는 정수여야 함".to_string())?,
                );
            }
            "--help" | "-h" => return Err(USAGE.into()),
            flag => return Err(format!("알 수 없는 인자: {flag}")),
        }
        index += 1;
    }
    let root = root.ok_or_else(|| "--root가 필요함".to_string())?;
    if !absolute_without_parent(&root) || !root.is_dir() {
        return Err("--root는 존재하는 절대 디렉터리여야 함".into());
    }
    if !absolute_without_parent(&journal_path) {
        return Err("--journal-path는 상위 탐색이 없는 절대 경로여야 함".into());
    }
    Ok(Args {
        root,
        min_age_days,
        journal_path,
        execute,
        approval_phrase,
        approved_at_ms,
    })
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

fn run(args: Args) -> Result<serde_json::Value, String> {
    let observed_at_ms = now_ms();
    let candidates = find_artifacts(&args.root, args.min_age_days, observed_at_ms);
    let review = if candidates.is_empty() {
        None
    } else {
        Some(review_selection(&args.root, &candidates, observed_at_ms)?)
    };
    let results: Vec<DevArtifactCleanResult> = if args.execute {
        let approval_phrase = args
            .approval_phrase
            .as_deref()
            .ok_or_else(|| "development-artifact-confirmation-required".to_string())?;
        let approved_at_ms = args
            .approved_at_ms
            .ok_or_else(|| "development-artifact-confirmation-required".to_string())?;
        if candidates.is_empty() {
            return Err("development-artifact-selection-empty".into());
        }
        if let Some(parent) = args.journal_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|_| "development-artifact-journal-parent-create-failed".to_string())?;
        }
        let selection_fingerprint = selection_fingerprint(&args.root, &candidates)?;
        let approval = DevArtifactApproval {
            selection_fingerprint,
            reviewed_at_ms: approved_at_ms,
            expires_at_ms: approved_at_ms.saturating_add(MAX_REVIEW_AGE_MS),
            exact_phrase: approval_phrase.to_string(),
        };
        clean_artifacts_with_approval(
            &candidates,
            &args.root,
            args.min_age_days,
            &args.journal_path,
            observed_at_ms,
            &approval,
        )
    } else {
        Vec::new()
    };
    serde_json::to_value(serde_json::json!({
        "schema_version": 2,
        "schema_kind": "disksage.dev-artifact-cleanup",
        "root": args.root,
        "min_age_days": args.min_age_days,
        "observed_at_ms": observed_at_ms,
        "executed": args.execute,
        "candidate_count": candidates.len(),
        "candidates": candidates,
        "selection_fingerprint": review.as_ref().map(|value| value.selection_fingerprint.as_str()),
        "exact_approval_phrase": review.as_ref().map(|value| value.exact_phrase.as_str()),
        "approval_expires_at_ms": review.as_ref().map(|value| value.expires_at_ms),
        "results": results,
        "journal_path": if args.execute { Some(args.journal_path) } else { None::<PathBuf> },
        "cloud_write_executed": false,
        "source_eviction_executed": false,
    }))
    .map_err(|_| "development-artifact-report-encode-failed".to_string())
}

fn main() {
    let raw = std::env::args().skip(1).collect::<Vec<_>>();
    if raw.len() == 1 && matches!(raw[0].as_str(), "--help" | "-h") {
        println!("{USAGE}");
        return;
    }
    match parse_args(&raw).and_then(run) {
        Ok(report) => println!(
            "{}",
            serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".into())
        ),
        Err(error) => {
            eprintln!("disksage-dev-artifacts: {error}");
            std::process::exit(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_defaults_to_read_only_and_requires_bounded_absolute_root() {
        let root = std::env::temp_dir();
        let parsed = parse_args(&["--root".into(), root.to_string_lossy().into_owned()]).unwrap();
        assert_eq!(parsed.min_age_days, 30);
        assert!(!parsed.execute);
        assert!(parsed.approval_phrase.is_none());
        assert!(parsed.approved_at_ms.is_none());
        assert!(parse_args(&["--root".into(), "relative".into()]).is_err());
        assert!(parse_args(&[
            "--root".into(),
            root.to_string_lossy().into_owned(),
            "--min-age-days".into(),
            (MAX_AGE_DAYS + 1).to_string(),
        ])
        .is_err());
    }

    #[test]
    fn parser_accepts_explicit_review_authority_and_journal() {
        let root = std::env::temp_dir();
        let parsed = parse_args(&[
            "--root".into(),
            root.to_string_lossy().into_owned(),
            "--min-age-days".into(),
            "7".into(),
            "--journal-path".into(),
            "/tmp/disksage-dev-artifacts-journal.jsonl".into(),
            "--execute".into(),
            "--approval-phrase".into(),
            "MOVE DEVELOPMENT ARTIFACTS abc TO TRASH".into(),
            "--approved-at-ms".into(),
            "123".into(),
        ])
        .unwrap();
        assert_eq!(parsed.min_age_days, 7);
        assert!(parsed.execute);
        assert_eq!(parsed.approved_at_ms, Some(123));
        assert_eq!(
            parsed.approval_phrase.as_deref(),
            Some("MOVE DEVELOPMENT ARTIFACTS abc TO TRASH")
        );
        assert_eq!(
            parsed.journal_path,
            PathBuf::from("/tmp/disksage-dev-artifacts-journal.jsonl")
        );
    }
}
