//! Bounded, identity-checked development-artifact inventory and cleanup.
//!
//! The default operation is read-only. `--execute` re-scans every requested artifact and moves it
//! to OS Trash only when its path, metadata manifest, and filesystem identity still match.

use disksage_lib::dev_artifacts::{
    clean_artifacts, find_artifacts, permanently_delete_artifacts, DevArtifact,
    DevArtifactCleanResult,
};
use std::path::{Component, Path, PathBuf};

const MAX_AGE_DAYS: u64 = 3_650;
const MAX_RATIONALE_CHARS: usize = 1_000;
const USAGE: &str = "usage: disksage-dev-artifacts --root ABSOLUTE_PATH [--kind ARTIFACT_KIND] [--min-age-days N] [--journal-path ABSOLUTE_PATH] [--execute] [--permanent --confirm EXACT_PHRASE --rationale TEXT]";

#[derive(Debug, PartialEq, Eq)]
struct Args {
    root: PathBuf,
    kind: Option<String>,
    min_age_days: u64,
    journal_path: PathBuf,
    execute: bool,
    permanent: bool,
    confirm: Option<String>,
    rationale: Option<String>,
}

fn absolute_without_parent(path: &Path) -> bool {
    path.is_absolute()
        && !path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
}

fn default_journal_path() -> Result<PathBuf, String> {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
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

fn rationale_valid(value: &str) -> bool {
    !value.is_empty()
        && value.trim() == value
        && value.chars().count() <= MAX_RATIONALE_CHARS
        && !value.chars().any(char::is_control)
}

fn parse_args(raw: &[String]) -> Result<Args, String> {
    let mut root = None;
    let mut kind = None;
    let mut min_age_days = 30;
    let mut journal_path = default_journal_path()?;
    let mut execute = false;
    let mut permanent = false;
    let mut confirm = None;
    let mut rationale = None;
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
            "--kind" => {
                index += 1;
                kind = Some(
                    raw.get(index)
                        .filter(|value| !value.is_empty())
                        .ok_or_else(|| "--kind 값이 필요함".to_string())?
                        .clone(),
                );
            }
            "--journal-path" => {
                index += 1;
                journal_path = PathBuf::from(
                    raw.get(index)
                        .ok_or_else(|| "--journal-path 값이 필요함".to_string())?,
                );
            }
            "--confirm" => {
                index += 1;
                confirm = Some(
                    raw.get(index)
                        .filter(|value| !value.is_empty())
                        .ok_or_else(|| "--confirm 값이 필요함".to_string())?
                        .clone(),
                );
            }
            "--rationale" => {
                index += 1;
                rationale = Some(
                    raw.get(index)
                        .filter(|value| !value.is_empty())
                        .ok_or_else(|| "--rationale 값이 필요함".to_string())?
                        .clone(),
                );
            }
            "--execute" => execute = true,
            "--permanent" => permanent = true,
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
    if permanent && !execute {
        return Err("--permanent requires --execute".into());
    }
    if permanent && (confirm.is_none() || rationale.is_none()) {
        return Err("--permanent requires --confirm and --rationale".into());
    }
    if rationale.as_deref().is_some_and(|value| !rationale_valid(value)) {
        return Err("--rationale must be 1..1000 visible characters without leading/trailing whitespace".into());
    }
    Ok(Args {
        root,
        kind,
        min_age_days,
        journal_path,
        execute,
        permanent,
        confirm,
        rationale,
    })
}

fn hash_field(hasher: &mut blake3::Hasher, value: &[u8]) {
    hasher.update(&(value.len() as u64).to_le_bytes());
    hasher.update(value);
}

fn permanent_approval_phrase(
    root: &Path,
    kind: Option<&str>,
    min_age_days: u64,
    candidates: &[DevArtifact],
) -> Option<String> {
    if candidates.is_empty() {
        return None;
    }
    let mut ordered = candidates.iter().collect::<Vec<_>>();
    ordered.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.kind.cmp(&right.kind))
            .then_with(|| left.fingerprint.cmp(&right.fingerprint))
            .then_with(|| left.object_id.cmp(&right.object_id))
    });
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-dev-artifact-permanent-v1\0");
    hash_field(&mut hasher, root.as_os_str().as_encoded_bytes());
    hash_field(&mut hasher, kind.unwrap_or_default().as_bytes());
    hash_field(&mut hasher, &min_age_days.to_le_bytes());
    hash_field(&mut hasher, &(ordered.len() as u64).to_le_bytes());
    let mut total_bytes = 0u64;
    for candidate in ordered {
        for value in [
            candidate.path.as_bytes(),
            candidate.kind.as_bytes(),
            candidate.project.as_bytes(),
            candidate.fingerprint.as_bytes(),
            candidate.object_id.as_bytes(),
        ] {
            hash_field(&mut hasher, value);
        }
        for value in [
            candidate.bytes,
            candidate.files,
            candidate.skipped,
            candidate.age_days,
        ] {
            hash_field(&mut hasher, &value.to_le_bytes());
        }
        hash_field(&mut hasher, &[u8::from(candidate.scan_complete)]);
        total_bytes = total_bytes.saturating_add(candidate.bytes);
    }
    Some(format!(
        "DiskSage permanent dev cleanup {} {total_bytes} 승인 {}",
        candidates.len(),
        hasher.finalize().to_hex()
    ))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

fn run(args: Args) -> Result<serde_json::Value, String> {
    let observed_at_ms = now_ms();
    let candidates = find_artifacts(&args.root, args.min_age_days, observed_at_ms)
        .into_iter()
        .filter(|candidate| {
            args.kind
                .as_deref()
                .is_none_or(|kind| candidate.kind == kind)
        })
        .collect::<Vec<_>>();
    let permanent_confirmation_phrase = permanent_approval_phrase(
        &args.root,
        args.kind.as_deref(),
        args.min_age_days,
        &candidates,
    );
    if args.execute && args.permanent {
        let phrase = permanent_confirmation_phrase
            .as_deref()
            .ok_or_else(|| "development-artifact-permanent-empty-candidate-set".to_string())?;
        if args.confirm.as_deref() != Some(phrase) {
            return Err("development-artifact-permanent-confirmation-mismatch".into());
        }
    }
    let results: Vec<DevArtifactCleanResult> = if args.execute {
        if let Some(parent) = args.journal_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|_| "development-artifact-journal-parent-create-failed".to_string())?;
        }
        if args.permanent {
            permanently_delete_artifacts(
                &candidates,
                &args.root,
                args.min_age_days,
                &args.journal_path,
                observed_at_ms,
            )
        } else {
            clean_artifacts(
                &candidates,
                &args.root,
                args.min_age_days,
                &args.journal_path,
                observed_at_ms,
            )
        }
    } else {
        Vec::new()
    };
    let recorded_rationale = if args.execute && args.permanent {
        args.rationale.clone()
    } else {
        None
    };
    serde_json::to_value(serde_json::json!({
        "schema_version": 1,
        "schema_kind": "disksage.dev-artifact-cleanup",
        "root": args.root,
        "kind": args.kind,
        "min_age_days": args.min_age_days,
        "observed_at_ms": observed_at_ms,
        "executed": args.execute,
        "permanent": args.permanent,
        "candidate_count": candidates.len(),
        "candidates": candidates,
        "results": results,
        "journal_path": if args.execute { Some(args.journal_path) } else { None::<PathBuf> },
        "permanent_confirmation_phrase": permanent_confirmation_phrase,
        "rationale": recorded_rationale,
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
        assert_eq!(parsed.kind, None);
        assert!(!parsed.execute);
        assert_eq!(parsed.confirm, None);
        assert_eq!(parsed.rationale, None);
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
    fn parser_accepts_explicit_execute_and_journal() {
        let root = std::env::temp_dir();
        let parsed = parse_args(&[
            "--root".into(),
            root.to_string_lossy().into_owned(),
            "--min-age-days".into(),
            "7".into(),
            "--kind".into(),
            "vscode-obsolete-extension".into(),
            "--journal-path".into(),
            "/tmp/disksage-dev-artifacts-journal.jsonl".into(),
            "--execute".into(),
        ])
        .unwrap();
        assert_eq!(parsed.min_age_days, 7);
        assert_eq!(parsed.kind.as_deref(), Some("vscode-obsolete-extension"));
        assert!(parsed.execute);
        assert_eq!(
            parsed.journal_path,
            PathBuf::from("/tmp/disksage-dev-artifacts-journal.jsonl")
        );
    }

    #[test]
    fn permanent_deletion_requires_explicit_execute() {
        let root = std::env::temp_dir();
        assert_eq!(
            parse_args(&[
                "--root".into(),
                root.to_string_lossy().into_owned(),
                "--permanent".into(),
            ])
            .unwrap_err(),
            "--permanent requires --execute"
        );
    }

    #[test]
    fn permanent_deletion_requires_bound_confirmation_and_rationale() {
        let root = std::env::temp_dir();
        assert_eq!(
            parse_args(&[
                "--root".into(),
                root.to_string_lossy().into_owned(),
                "--execute".into(),
                "--permanent".into(),
            ])
            .unwrap_err(),
            "--permanent requires --confirm and --rationale"
        );
    }

    #[test]
    fn permanent_deletion_accepts_complete_operator_authority() {
        let root = std::env::temp_dir();
        let parsed = parse_args(&[
            "--root".into(),
            root.to_string_lossy().into_owned(),
            "--execute".into(),
            "--permanent".into(),
            "--confirm".into(),
            "reviewed phrase".into(),
            "--rationale".into(),
            "operator reviewed regenerable artifacts".into(),
        ])
        .unwrap();
        assert_eq!(parsed.confirm.as_deref(), Some("reviewed phrase"));
        assert_eq!(
            parsed.rationale.as_deref(),
            Some("operator reviewed regenerable artifacts")
        );
    }

    #[test]
    fn permanent_deletion_rejects_unbounded_or_control_rationale() {
        let root = std::env::temp_dir();
        for rationale in [" leading-space", "line\nbreak"] {
            assert_eq!(
                parse_args(&[
                    "--root".into(),
                    root.to_string_lossy().into_owned(),
                    "--execute".into(),
                    "--permanent".into(),
                    "--confirm".into(),
                    "reviewed phrase".into(),
                    "--rationale".into(),
                    rationale.into(),
                ])
                .unwrap_err(),
                "--rationale must be 1..1000 visible characters without leading/trailing whitespace"
            );
        }
    }

    #[test]
    fn permanent_approval_phrase_binds_candidate_identity() {
        let root = std::env::temp_dir();
        let candidate = DevArtifact {
            path: root.join("target").to_string_lossy().into_owned(),
            kind: "target".into(),
            project: root.to_string_lossy().into_owned(),
            bytes: 4096,
            files: 8,
            skipped: 0,
            scan_complete: true,
            fingerprint: "manifest-a".into(),
            object_id: "object-a".into(),
            age_days: 30,
        };
        let first = permanent_approval_phrase(&root, Some("target"), 30, &[candidate.clone()])
            .unwrap();
        let mut changed = candidate;
        changed.object_id = "object-b".into();
        let second = permanent_approval_phrase(&root, Some("target"), 30, &[changed]).unwrap();
        assert_ne!(first, second);
        assert!(permanent_approval_phrase(&root, Some("target"), 30, &[]).is_none());
    }
}
