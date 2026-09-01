//! Identity-bound reclaim for stale OneDrive download staging files.

use crate::{git_worktree, safety};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
};

const ONTOLOGY_CLASS: &str = "https://disksage.app/ontology#CloudTransferTemporaryArtifact";
const MIN_AGE_MS: u64 = 86_400_000;
const MAX_FILES: usize = 1_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OneDriveTempCandidate {
    pub ontology_class: String,
    pub path: String,
    pub object_id: String,
    pub content_sha1: String,
    pub local_bytes: u64,
    pub remote_bytes: u64,
    pub modified_ms: u64,
    pub candidate_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OneDriveTempPlan {
    pub schema_kind: String,
    pub ontology_class: String,
    pub observed_at_ms: u64,
    pub temp_root: String,
    pub database_path: String,
    pub candidates: Vec<OneDriveTempCandidate>,
    pub candidate_allocated_bytes: u64,
    pub candidate_set_fingerprint: String,
    pub evidence_complete: bool,
    pub exact_approval_phrase: Option<String>,
    pub filesystem_mutation_executed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OneDriveTempExecution {
    pub ontology_class: &'static str,
    pub candidate_set_fingerprint: String,
    pub planned_count: usize,
    pub removed_count: usize,
    pub removed_allocated_bytes_upper_bound: u64,
    pub executed_at_ms: u64,
    pub filesystem_mutation_executed: bool,
    pub verification_complete: bool,
    pub failure: Option<String>,
    pub recoverability: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RemovalProgress {
    removed_count: usize,
    failure: Option<String>,
}

fn paths(home: &Path) -> Result<(PathBuf, PathBuf), String> {
    if !home.is_absolute() {
        return Err("onedrive-temp-home-invalid".into());
    }
    Ok((
        home.join("Library/Application Support/OneDrive/tmp"),
        home.join("Library/Group Containers/UBF8T346G9.OneDriveStandaloneSuite/.Dbfs.Dbfs_Personal.noindex/dbfs.db"),
    ))
}

fn filename_sha1(path: &Path) -> Option<String> {
    let hash = path
        .file_name()?
        .to_str()?
        .strip_suffix(".temp")?
        .rsplit('-')
        .next()?;
    (hash.len() == 40 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| hash.to_ascii_uppercase())
}

fn remote_records(database: &Path, hashes: &[String]) -> Result<BTreeMap<String, u64>, String> {
    if hashes.is_empty() {
        return Ok(BTreeMap::new());
    }
    let sqlite = Path::new("/usr/bin/sqlite3");
    let metadata = fs::symlink_metadata(sqlite).map_err(|_| "onedrive-temp-sqlite-unavailable")?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("onedrive-temp-sqlite-invalid".into());
    }
    let values = hashes
        .iter()
        .map(|hash| format!("'{hash}'"))
        .collect::<Vec<_>>()
        .join(",");
    let query = format!(
        "SELECT substr(hex(content_hash),1,40),max(size) FROM dbfs_mftrecord \
         WHERE substr(hex(content_hash),1,40) IN ({values}) AND ph_insync=1 AND pending=0 \
         GROUP BY 1 ORDER BY 1;"
    );
    let output = Command::new(sqlite)
        .arg("-readonly")
        .arg("-separator")
        .arg("\t")
        .arg(database)
        .arg(query)
        .output()
        .map_err(|_| "onedrive-temp-sqlite-spawn-failed")?;
    if !output.status.success() {
        return Err("onedrive-temp-database-query-failed".into());
    }
    let text =
        std::str::from_utf8(&output.stdout).map_err(|_| "onedrive-temp-database-output-invalid")?;
    text.lines()
        .map(|line| {
            let (hash, size) = line
                .split_once('\t')
                .ok_or_else(|| "onedrive-temp-database-output-invalid".to_string())?;
            Ok((
                hash.to_string(),
                size.parse()
                    .map_err(|_| "onedrive-temp-database-output-invalid".to_string())?,
            ))
        })
        .collect()
}

fn fingerprint(values: &[&str]) -> String {
    let mut hasher = blake3::Hasher::new();
    for value in values {
        hasher.update(&(value.len() as u64).to_le_bytes());
        hasher.update(value.as_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

fn candidate_fingerprint(
    path: &str,
    object_id: &str,
    hash: &str,
    local: u64,
    remote: u64,
    modified: u64,
) -> String {
    fingerprint(&[
        path,
        object_id,
        hash,
        &local.to_string(),
        &remote.to_string(),
        &modified.to_string(),
    ])
}

fn candidate_set_fingerprint(candidates: &[OneDriveTempCandidate]) -> String {
    fingerprint(
        &candidates
            .iter()
            .map(|candidate| candidate.candidate_fingerprint.as_str())
            .collect::<Vec<_>>(),
    )
}

pub fn plan(home: &Path, observed_at_ms: u64) -> Result<OneDriveTempPlan, String> {
    let (temp_root, database_path) = paths(home)?;
    let mut entries = fs::read_dir(&temp_root)
        .map_err(|_| "onedrive-temp-root-unavailable".to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "onedrive-temp-root-unreadable".to_string())?;
    entries.sort_by_key(|entry| entry.file_name());
    if entries.len() > MAX_FILES {
        return Err("onedrive-temp-file-limit-exceeded".into());
    }
    let hashes = entries
        .iter()
        .filter_map(|entry| filename_sha1(&entry.path()))
        .collect::<Vec<_>>();
    let records = remote_records(&database_path, &hashes)?;
    let mut candidates = Vec::new();
    for entry in entries {
        let path = entry.path();
        let Some(content_sha1) = filename_sha1(&path) else {
            continue;
        };
        let metadata = fs::symlink_metadata(&path)
            .map_err(|_| "onedrive-temp-file-metadata-unavailable".to_string())?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            continue;
        }
        let modified_ms = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .and_then(|duration| u64::try_from(duration.as_millis()).ok())
            .ok_or_else(|| "onedrive-temp-file-time-unavailable".to_string())?;
        let Some(&remote_bytes) = records.get(&content_sha1) else {
            continue;
        };
        if observed_at_ms.saturating_sub(modified_ms) < MIN_AGE_MS || metadata.len() > remote_bytes
        {
            continue;
        }
        let active = git_worktree::active_use_evidence(&path, 10_000, 64, false);
        if !active.assessed || !active.evidence_complete || active.active {
            continue;
        }
        let object_id = safety::filesystem_object_id(&path)
            .map_err(|_| "onedrive-temp-file-identity-unavailable".to_string())?;
        let path_string = path.to_string_lossy().into_owned();
        candidates.push(OneDriveTempCandidate {
            ontology_class: ONTOLOGY_CLASS.into(),
            candidate_fingerprint: candidate_fingerprint(
                &path_string,
                &object_id,
                &content_sha1,
                metadata.len(),
                remote_bytes,
                modified_ms,
            ),
            path: path_string,
            object_id,
            content_sha1,
            local_bytes: metadata.len(),
            remote_bytes,
            modified_ms,
        });
    }
    candidates.sort_by(|left, right| left.path.cmp(&right.path));
    let candidate_set_fingerprint = candidate_set_fingerprint(&candidates);
    let candidate_allocated_bytes = candidates
        .iter()
        .try_fold(0u64, |total, candidate| {
            let evidence = git_worktree::size_evidence(Path::new(&candidate.path), 1, 10_000);
            evidence
                .evidence_complete
                .then_some(total.saturating_add(evidence.allocated_bytes))
        })
        .ok_or_else(|| "onedrive-temp-allocation-evidence-incomplete".to_string())?;
    let exact_approval_phrase = (!candidates.is_empty()).then(|| {
        format!("DiskSage OneDrive stale download reclaim 승인 {candidate_set_fingerprint}")
    });
    Ok(OneDriveTempPlan {
        schema_kind: "disksage.onedrive-temp-reclaim-plan/v1".into(),
        ontology_class: ONTOLOGY_CLASS.into(),
        observed_at_ms,
        temp_root: temp_root.to_string_lossy().into_owned(),
        database_path: database_path.to_string_lossy().into_owned(),
        candidates,
        candidate_allocated_bytes,
        candidate_set_fingerprint,
        evidence_complete: true,
        exact_approval_phrase,
        filesystem_mutation_executed: false,
    })
}

fn provider_quiesced_with(
    mut observe: impl FnMut(&str) -> std::io::Result<ExitStatus>,
) -> Result<bool, String> {
    for name in ["OneDrive", "OneDrive Sync Service"] {
        let status = observe(name)
            .map_err(|_| "onedrive-temp-provider-observation-failed".to_string())?;
        match status.code() {
            Some(0) => return Ok(false),
            Some(1) => {}
            _ => return Err("onedrive-temp-provider-observation-failed".into()),
        }
    }
    Ok(true)
}

fn provider_quiesced() -> Result<bool, String> {
    provider_quiesced_with(|name| {
        Command::new("/usr/bin/pgrep")
            .args(["-x", name])
            .status()
    })
}

fn plan_while_provider_quiesced<T>(
    mut observe: impl FnMut() -> Result<bool, String>,
    planner: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    if !observe()? {
        return Err("onedrive-temp-provider-not-quiesced".into());
    }
    let planned = planner()?;
    if !observe()? {
        return Err("onedrive-temp-provider-restarted-during-plan".into());
    }
    Ok(planned)
}

fn remove_candidates_with(
    candidates: &[OneDriveTempCandidate],
    mut quiesced: impl FnMut() -> Result<bool, String>,
    mut object_id: impl FnMut(&Path) -> Result<String, String>,
    mut remove: impl FnMut(&Path) -> Result<(), String>,
) -> Result<RemovalProgress, String> {
    let mut removed_count = 0usize;
    for candidate in candidates {
        let attempt = (|| -> Result<(), String> {
            if !quiesced()? {
                return Err("onedrive-temp-provider-restarted-before-delete".into());
            }
            let path = Path::new(&candidate.path);
            if object_id(path)? != candidate.object_id {
                return Err("onedrive-temp-file-changed".into());
            }
            remove(path)?;
            Ok(())
        })();
        match attempt {
            Ok(()) => removed_count = removed_count.saturating_add(1),
            Err(error) if removed_count == 0 => return Err(error),
            Err(error) => {
                return Ok(RemovalProgress {
                    removed_count,
                    failure: Some(error),
                });
            }
        }
    }
    Ok(RemovalProgress {
        removed_count,
        failure: None,
    })
}

pub fn execute(
    home: &Path,
    expected_fingerprint: &str,
    approval: &str,
    executed_at_ms: u64,
) -> Result<OneDriveTempExecution, String> {
    let plan = plan_while_provider_quiesced(provider_quiesced, || plan(home, executed_at_ms))?;
    if plan.candidate_set_fingerprint != expected_fingerprint
        || plan.exact_approval_phrase.as_deref() != Some(approval)
    {
        return Err("onedrive-temp-approval-mismatch".into());
    }
    let progress = remove_candidates_with(
        &plan.candidates,
        provider_quiesced,
        |path| {
            safety::filesystem_object_id(path)
                .map_err(|_| "onedrive-temp-file-identity-unavailable".to_string())
        },
        |path| fs::remove_file(path).map_err(|_| "onedrive-temp-remove-failed".to_string()),
    )?;
    let verification_complete = progress.failure.is_none() && progress.removed_count == plan.candidates.len();
    Ok(OneDriveTempExecution {
        ontology_class: ONTOLOGY_CLASS,
        candidate_set_fingerprint: plan.candidate_set_fingerprint,
        planned_count: plan.candidates.len(),
        removed_count: progress.removed_count,
        removed_allocated_bytes_upper_bound: plan.candidate_allocated_bytes,
        executed_at_ms,
        filesystem_mutation_executed: progress.removed_count > 0,
        verification_complete,
        failure: progress.failure,
        recoverability: "not-recoverable; remote OneDrive content retained",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(path: &str, object_id: &str) -> OneDriveTempCandidate {
        OneDriveTempCandidate {
            ontology_class: ONTOLOGY_CLASS.into(),
            path: path.into(),
            object_id: object_id.into(),
            content_sha1: "A".repeat(40),
            local_bytes: 1,
            remote_bytes: 1,
            modified_ms: 1,
            candidate_fingerprint: "f".repeat(64),
        }
    }

    #[test]
    fn filename_parser_accepts_only_terminal_sha1_temp_names() {
        assert_eq!(
            filename_sha1(Path::new(
                "account-item-0123456789abcdef0123456789abcdef01234567.temp"
            )),
            Some("0123456789ABCDEF0123456789ABCDEF01234567".into())
        );
        assert_eq!(filename_sha1(Path::new("item.temp")), None);
        assert_eq!(
            filename_sha1(Path::new(
                "item-0123456789abcdef0123456789abcdef0123456z.temp"
            )),
            None
        );
    }

    #[test]
    fn provider_observation_failure_is_not_quiescence() {
        let result = provider_quiesced_with(|_| Err(std::io::Error::other("pgrep unavailable")));
        assert_eq!(
            result,
            Err("onedrive-temp-provider-observation-failed".into())
        );
    }

    #[test]
    fn provider_restart_during_planning_fails_closed() {
        let mut observations = [true, false].into_iter();
        let result = plan_while_provider_quiesced(
            || Ok(observations.next().expect("two provider observations")),
            || Ok(42_u8),
        );
        assert_eq!(
            result,
            Err("onedrive-temp-provider-restarted-during-plan".into())
        );
    }

    #[test]
    fn later_delete_failure_preserves_partial_mutation_receipt() {
        let candidates = vec![candidate("/tmp/one.temp", "one"), candidate("/tmp/two.temp", "two")];
        let mut remove_calls = 0usize;
        let progress = remove_candidates_with(
            &candidates,
            || Ok(true),
            |path| {
                Ok(if path == Path::new("/tmp/one.temp") {
                    "one".into()
                } else {
                    "two".into()
                })
            },
            |_| {
                remove_calls = remove_calls.saturating_add(1);
                if remove_calls == 2 {
                    Err("onedrive-temp-remove-failed".into())
                } else {
                    Ok(())
                }
            },
        )
        .unwrap();
        assert_eq!(progress.removed_count, 1);
        assert_eq!(progress.failure.as_deref(), Some("onedrive-temp-remove-failed"));
    }
}
