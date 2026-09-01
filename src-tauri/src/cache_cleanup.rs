use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::{commands::CleanResult, rules, safety};

fn sort_targets(targets: &mut Vec<rules::CacheTarget>) {
    targets.sort_by(|left, right| left.path.cmp(&right.path));
}

/// Local caches observed during the current low-disk incident and safe to regenerate.
/// npm's content-addressed cache is rebuilt by npm on demand; it is included only after the same
/// per-child identity and active-use checks as the other caches.
pub const AUTO_REGENERABLE_CACHE_IDS: [&str; 6] = [
    "npm-cache",
    "pnpm-cache",
    "adobe-cache",
    "edge-cache",
    "uv-cache",
    "trivy-cache",
];

const PROVEN_CACHE_TRASH_NAMES: [&str; 9] = [
    "_cacache",
    "v11",
    "Default",
    "simple-v21",
    "typequest",
    "wheels-v6",
    "sdists-v9",
    "builds-v0",
    "db",
];
const MAX_CACHE_TRASH_ENTRIES: usize = 1_000_000;

/// A cache directory already in OS Trash whose structure is still recognizable without reading
/// user file contents. Permanent removal is intentionally limited to these signatures.
///
/// The object identity, modification time, size, exact direct-child path, and structural signature
/// form the approval snapshot used by the irreversible purge boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CacheTrashCandidate {
    pub name: String,
    pub path: String,
    pub bytes: u64,
    pub modified_ms: u64,
    pub object_id: String,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CacheTrashPurgeResult {
    pub name: String,
    pub path: String,
    pub bytes: u64,
    pub signature: String,
    pub purged: bool,
    pub error: String,
}

fn modified_ms(metadata: &std::fs::Metadata) -> u64 {
    metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|value| u64::try_from(value.as_millis()).ok())
        .unwrap_or(0)
}

fn direct_child_is_dir(path: &Path, name: &str) -> bool {
    let child = path.join(name);
    std::fs::symlink_metadata(child)
        .is_ok_and(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink())
}

fn direct_child_is_file(path: &Path, name: &str) -> bool {
    let child = path.join(name);
    std::fs::symlink_metadata(child)
        .is_ok_and(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
}

fn looks_like_proven_cache_trash(path: &Path, name: &str) -> Option<&'static str> {
    let signature = match name {
        "_cacache"
            if direct_child_is_dir(path, "content-v2") && direct_child_is_dir(path, "tmp") =>
        {
            "npm-cacache"
        }
        "v11"
            if direct_child_is_dir(path, "metadata")
                && direct_child_is_dir(path, "metadata-full") =>
        {
            "pnpm-store-v11"
        }
        "Default"
            if direct_child_is_dir(path, "Cache") && direct_child_is_dir(path, "Code Cache") =>
        {
            "edge-profile-cache"
        }
        "simple-v21" if direct_child_is_dir(path, "pypi") => "uv-simple-index-cache",
        "typequest" if direct_child_is_dir(path, "common") && direct_child_is_dir(path, ".2") => {
            "uv-typequest-cache"
        }
        "wheels-v6" if direct_child_is_dir(path, "pypi") => "uv-wheel-cache",
        "sdists-v9"
            if direct_child_is_dir(path, "pypi") && direct_child_is_dir(path, "editable") =>
        {
            "uv-sdist-cache"
        }
        "builds-v0" => {
            let has_build = std::fs::read_dir(path)
                .ok()
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
                .any(|entry| {
                    let child = entry.path();
                    entry.file_name().to_string_lossy().starts_with(".tmp")
                        && direct_child_is_dir(path, &entry.file_name().to_string_lossy())
                        && direct_child_is_file(&child, "pyvenv.cfg")
                });
            has_build.then_some("uv-build-cache")?
        }
        "db" if direct_child_is_file(path, "trivy.db")
            && direct_child_is_file(path, "metadata.json") =>
        {
            "trivy-database-cache"
        }
        _ => return None,
    };
    Some(signature)
}

fn bounded_tree_size(path: &Path, entries: &mut usize) -> Result<u64, String> {
    *entries = entries.saturating_add(1);
    if *entries > MAX_CACHE_TRASH_ENTRIES {
        return Err("cache-trash-entry-limit-exceeded".into());
    }
    let metadata =
        std::fs::symlink_metadata(path).map_err(|_| "cache-trash-stat-failed".to_string())?;
    if metadata.file_type().is_symlink() {
        return Err("cache-trash-symlink-rejected".into());
    }
    if metadata.is_file() {
        return Ok(metadata.len());
    }
    if !metadata.is_dir() {
        return Err("cache-trash-object-type-unsupported".into());
    }
    let mut total = 0u64;
    for entry in std::fs::read_dir(path).map_err(|_| "cache-trash-read-dir-failed".to_string())? {
        let entry = entry.map_err(|_| "cache-trash-read-entry-failed".to_string())?;
        total = total.saturating_add(bounded_tree_size(&entry.path(), entries)?);
    }
    Ok(total)
}

fn inspect_proven_cache_trash_candidate(path: &Path, name: &str) -> Option<CacheTrashCandidate> {
    let metadata = std::fs::symlink_metadata(path).ok()?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return None;
    }
    let signature = looks_like_proven_cache_trash(path, name)?;
    let mut count = 0;
    let bytes = bounded_tree_size(path, &mut count).ok()?;
    let object_id = safety::filesystem_object_id(path).ok()?;
    Some(CacheTrashCandidate {
        name: name.into(),
        path: path.to_string_lossy().into_owned(),
        bytes,
        modified_ms: modified_ms(&metadata),
        object_id,
        signature: signature.into(),
    })
}

/// Return only direct OS-Trash children whose cache signature is proven by structure and whose
/// size can be bounded without following symlinks or reading file contents. The returned snapshot
/// is the complete candidate identity that must be reviewed before irreversible purge execution.
pub fn proven_cache_trash_candidates(home: &Path) -> Vec<CacheTrashCandidate> {
    let trash = home.join(".Trash");
    let Ok(entries) = std::fs::read_dir(&trash) else {
        return Vec::new();
    };
    let mut candidates = Vec::new();
    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !PROVEN_CACHE_TRASH_NAMES.contains(&name.as_str()) {
            continue;
        }
        let path = entry.path();
        if let Some(candidate) = inspect_proven_cache_trash_candidate(&path, &name) {
            candidates.push(candidate);
        }
    }
    candidates.sort_by(|left, right| left.path.cmp(&right.path));
    candidates
}

fn validate_approved_cache_trash_candidate(
    home: &Path,
    candidate: &CacheTrashCandidate,
) -> Result<PathBuf, String> {
    if !PROVEN_CACHE_TRASH_NAMES.contains(&candidate.name.as_str()) {
        return Err("cache-trash-candidate-name-unapproved".into());
    }
    let expected_path = home.join(".Trash").join(&candidate.name);
    let candidate_path = PathBuf::from(&candidate.path);
    if candidate_path != expected_path {
        return Err("cache-trash-candidate-not-direct-child".into());
    }
    let current = inspect_proven_cache_trash_candidate(&candidate_path, &candidate.name)
        .ok_or_else(|| "cache-trash-candidate-no-longer-proven".to_string())?;
    if current != *candidate {
        return Err("cache-trash-approved-candidate-stale".into());
    }
    Ok(candidate_path)
}

/// Permanently remove only the exact proven cache directories that were present in the reviewed
/// approval snapshot. Newly appearing Trash candidates are ignored. Every approved candidate is
/// validated before any mutation and again immediately before its own deletion; stale, replaced,
/// moved, resized, modified, symlinked, or structurally changed candidates fail closed.
pub fn purge_proven_cache_trash(
    home: &Path,
    approved_candidates: &[CacheTrashCandidate],
    journal_path: &Path,
    now_ms: u64,
) -> Result<Vec<CacheTrashPurgeResult>, String> {
    if approved_candidates.len() > PROVEN_CACHE_TRASH_NAMES.len() {
        return Err("cache-trash-approved-candidate-limit-exceeded".into());
    }
    let mut approved = approved_candidates.to_vec();
    approved.sort_by(|left, right| left.path.cmp(&right.path));
    if approved
        .windows(2)
        .any(|pair| pair[0].path == pair[1].path)
    {
        return Err("cache-trash-approved-candidate-duplicate".into());
    }

    // Validate the complete reviewed set before crossing the first irreversible boundary.
    for candidate in &approved {
        validate_approved_cache_trash_candidate(home, candidate)?;
    }

    let mut results = Vec::with_capacity(approved.len());
    for candidate in approved {
        let path = validate_approved_cache_trash_candidate(home, &candidate)?;
        let mut entry = crate::safety::JournalEntry {
            ts_ms: now_ms,
            op: "permanent_cache_trash_delete".into(),
            path: candidate.path.clone(),
            bytes: candidate.bytes,
            outcome: "pending".into(),
        };
        crate::safety::journal_append(journal_path, &entry).map_err(|error| error.to_string())?;
        let outcome = match std::fs::remove_dir_all(&path) {
            Ok(()) => Ok(()),
            Err(error) => Err(error.to_string()),
        };
        entry.outcome = match &outcome {
            Ok(()) => "ok".into(),
            Err(error) => format!("error:{error}"),
        };
        crate::safety::journal_append(journal_path, &entry).map_err(|error| error.to_string())?;
        results.push(CacheTrashPurgeResult {
            name: candidate.name,
            path: candidate.path,
            bytes: candidate.bytes,
            signature: candidate.signature,
            purged: outcome.is_ok(),
            error: outcome.err().unwrap_or_default(),
        });
    }
    Ok(results)
}

fn active_use_blocker(
    evidence: &crate::git_worktree::GitWorktreeActiveUseEvidence,
) -> Option<&'static str> {
    if !evidence.assessed || !evidence.evidence_complete {
        Some("cache-target-active-use-evidence-incomplete")
    } else if evidence.active {
        Some("cache-target-active-use-detected")
    } else {
        None
    }
}

pub(crate) fn clean_cache_contents_inner(
    bases: &rules::BaseDirs,
    dir: &Path,
    requested_targets: &[rules::CacheTarget],
    journal_path: &Path,
    now_ms: u64,
) -> Result<Vec<CleanResult>, String> {
    if !rules::is_catalog_path(bases, dir) {
        return Err("cache-root-not-current-or-safe".into());
    }
    let mut expected = requested_targets.to_vec();
    sort_targets(&mut expected);
    let mut current = rules::cache_targets(dir)?;
    sort_targets(&mut current);
    if current != expected {
        return Err("cache-cleanup-targets-stale".into());
    }

    Ok(expected
        .into_iter()
        .map(|target| {
            // Probe each reviewed child independently: a live MCP/uv process must not prevent
            // reclaiming unrelated, inactive cache archives in the same catalog root.
            let recursive = std::fs::symlink_metadata(&target.path)
                .map(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink())
                .unwrap_or(false);
            let active_use = crate::git_worktree::active_use_evidence(
                Path::new(&target.path),
                crate::reclaim::ACTIVE_USE_PROBE_TIMEOUT_MS,
                crate::reclaim::ACTIVE_USE_PROBE_MAX_PIDS,
                recursive,
            );
            if let Some(error) = active_use_blocker(&active_use) {
                return CleanResult {
                    path: target.path,
                    ok: false,
                    error: error.into(),
                };
            }
            match safety::trash_delete_if_identity(
                Path::new(&target.path),
                &target.object_id,
                target.bytes,
                journal_path,
                now_ms,
            ) {
                Ok(()) => CleanResult {
                    path: target.path,
                    ok: true,
                    error: String::new(),
                },
                Err(error) => CleanResult {
                    path: target.path,
                    ok: false,
                    error: error.to_string(),
                },
            }
        })
        .collect())
}

pub(crate) fn clean_regenerable_caches_inner(
    bases: &rules::BaseDirs,
    journal_path: &Path,
    now_ms: u64,
) -> Vec<CleanResult> {
    rules::cache_candidates(bases)
        .into_iter()
        .filter(|candidate| {
            AUTO_REGENERABLE_CACHE_IDS.contains(&candidate.id.as_str()) && candidate.exists
        })
        .flat_map(|candidate| {
            let path = std::path::PathBuf::from(&candidate.path);
            match rules::cache_targets(&path) {
                Ok(targets) if targets.is_empty() => Vec::new(),
                Ok(targets) => {
                    clean_cache_contents_inner(bases, &path, &targets, journal_path, now_ms)
                        .unwrap_or_else(|error| {
                            vec![CleanResult {
                                path: candidate.path,
                                ok: false,
                                error,
                            }]
                        })
                }
                Err(error) => vec![CleanResult {
                    path: candidate.path,
                    ok: false,
                    error,
                }],
            }
        })
        .collect()
}

/// Headless entry point used by the audited CLI; it returns only local execution evidence.
pub fn clean_regenerable_caches_headless(
    journal_path: &Path,
    now_ms: u64,
) -> Result<serde_json::Value, String> {
    let bases = rules::BaseDirs::from_env().ok_or("cache-base-directories-unavailable")?;
    serde_json::to_value(clean_regenerable_caches_inner(&bases, journal_path, now_ms))
        .map_err(|error| error.to_string())
}

/// Read the exact cache children that may be included in a later identity-bound Trash request.
#[cfg(not(coverage))]
#[tauri::command]
pub fn list_cache_targets(dir: String) -> Result<Vec<rules::CacheTarget>, String> {
    let bases = rules::BaseDirs::from_env().ok_or("cache-base-directories-unavailable")?;
    if !rules::is_catalog_path(&bases, Path::new(&dir)) {
        return Err("cache-root-not-current-or-safe".into());
    }
    rules::cache_targets(Path::new(&dir))
}

/// Move only the reviewed cache children to the OS Trash, retaining the cache root itself.
#[cfg(not(coverage))]
#[tauri::command]
pub fn clean_cache_contents(
    dir: String,
    targets: Vec<rules::CacheTarget>,
    app: tauri::AppHandle,
) -> Result<Vec<CleanResult>, String> {
    let bases = rules::BaseDirs::from_env().ok_or("cache-base-directories-unavailable")?;
    let journal_path = crate::commands::journal_file_path(&app)?;
    clean_cache_contents_inner(
        &bases,
        Path::new(&dir),
        &targets,
        &journal_path,
        crate::commands::now_ms(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fake_bases(root: &Path) -> rules::BaseDirs {
        rules::BaseDirs {
            temp: root.join("cache"),
            local_data: root.join("local"),
            home: root.join("home"),
        }
    }

    fn create_npm_cache(trash: &Path, name: &str) -> PathBuf {
        let cache = trash.join(name);
        fs::create_dir_all(cache.join("content-v2")).unwrap();
        fs::create_dir(cache.join("tmp")).unwrap();
        fs::write(cache.join("content-v2").join("entry"), b"cache").unwrap();
        cache
    }

    #[test]
    fn cleanup_rejects_non_catalog_root() {
        let tmp = tempfile::tempdir().unwrap();
        let bases = fake_bases(tmp.path());
        fs::create_dir(&bases.temp).unwrap();
        let journal = tmp.path().join("journal.jsonl");

        let error = clean_cache_contents_inner(&bases, tmp.path(), &[], &journal, 1)
            .err()
            .expect("non-catalog root must be rejected");

        assert_eq!(error, "cache-root-not-current-or-safe");
    }

    #[test]
    fn cleanup_rejects_stale_target_snapshot_without_mutation() {
        let tmp = tempfile::tempdir().unwrap();
        let bases = fake_bases(tmp.path());
        fs::create_dir(&bases.temp).unwrap();
        let victim = bases.temp.join("keep.bin");
        fs::write(&victim, b"keep").unwrap();
        let journal = tmp.path().join("journal.jsonl");
        let mut targets = rules::cache_targets(&bases.temp).unwrap();
        targets[0].bytes += 1;

        let error = clean_cache_contents_inner(&bases, &bases.temp, &targets, &journal, 1)
            .err()
            .expect("stale target snapshot must be rejected");

        assert_eq!(error, "cache-cleanup-targets-stale");
        assert_eq!(fs::read(&victim).unwrap(), b"keep");
    }

    #[test]
    fn active_use_evidence_blocks_cache_mutation() {
        let incomplete = crate::git_worktree::GitWorktreeActiveUseEvidence {
            method: "lsof-file-pid".into(),
            assessed: true,
            evidence_complete: false,
            active: false,
            observed_pids: Vec::new(),
            results_truncated: false,
            error: Some("active-use-timeout".into()),
        };
        assert_eq!(
            active_use_blocker(&incomplete),
            Some("cache-target-active-use-evidence-incomplete")
        );

        let active = crate::git_worktree::GitWorktreeActiveUseEvidence {
            method: "lsof-file-pid".into(),
            assessed: true,
            evidence_complete: true,
            active: true,
            observed_pids: vec![42],
            results_truncated: false,
            error: None,
        };
        assert_eq!(
            active_use_blocker(&active),
            Some("cache-target-active-use-detected")
        );
    }

    #[test]
    fn proven_cache_trash_requires_reviewed_identity_and_journals_purge() {
        let tmp = tempfile::tempdir().unwrap();
        let trash = tmp.path().join(".Trash");
        fs::create_dir(&trash).unwrap();
        let npm = create_npm_cache(&trash, "_cacache");
        let unrelated = trash.join("Default");
        fs::create_dir(&unrelated).unwrap();
        fs::create_dir(unrelated.join("Cache")).unwrap();

        let candidates = proven_cache_trash_candidates(tmp.path());
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].signature, "npm-cacache");
        assert_eq!(candidates[0].bytes, 5);
        assert!(!candidates[0].object_id.is_empty());

        let journal = tmp.path().join("journal.jsonl");
        let results = purge_proven_cache_trash(tmp.path(), &candidates, &journal, 7).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].purged);
        assert!(!npm.exists());
        let journal_text = fs::read_to_string(journal).unwrap();
        assert!(journal_text.contains("permanent_cache_trash_delete"));
        assert!(journal_text.contains("\"outcome\":\"pending\""));
        assert!(journal_text.contains("\"outcome\":\"ok\""));
    }

    #[test]
    fn purge_ignores_candidates_that_appeared_after_review() {
        let tmp = tempfile::tempdir().unwrap();
        let trash = tmp.path().join(".Trash");
        fs::create_dir(&trash).unwrap();
        let npm = create_npm_cache(&trash, "_cacache");
        let approved = proven_cache_trash_candidates(tmp.path());

        let trivy = trash.join("db");
        fs::create_dir(&trivy).unwrap();
        fs::write(trivy.join("trivy.db"), b"db").unwrap();
        fs::write(trivy.join("metadata.json"), b"{}").unwrap();

        let journal = tmp.path().join("journal.jsonl");
        let results = purge_proven_cache_trash(tmp.path(), &approved, &journal, 8).unwrap();

        assert_eq!(results.len(), 1);
        assert!(!npm.exists());
        assert!(trivy.exists(), "unreviewed new candidate must remain untouched");
    }

    #[test]
    fn purge_rejects_stale_reviewed_candidate_before_any_mutation() {
        let tmp = tempfile::tempdir().unwrap();
        let trash = tmp.path().join(".Trash");
        fs::create_dir(&trash).unwrap();
        let npm = create_npm_cache(&trash, "_cacache");
        let approved = proven_cache_trash_candidates(tmp.path());
        fs::write(npm.join("content-v2").join("late-entry"), b"changed").unwrap();

        let journal = tmp.path().join("journal.jsonl");
        let error = purge_proven_cache_trash(tmp.path(), &approved, &journal, 9).unwrap_err();

        assert_eq!(error, "cache-trash-approved-candidate-stale");
        assert!(npm.exists());
        assert!(!journal.exists());
    }

    #[test]
    fn purge_rejects_non_direct_approved_candidate() {
        let tmp = tempfile::tempdir().unwrap();
        let trash = tmp.path().join(".Trash");
        fs::create_dir(&trash).unwrap();
        create_npm_cache(&trash, "_cacache");
        let mut approved = proven_cache_trash_candidates(tmp.path());
        approved[0].path = trash
            .join("nested")
            .join("_cacache")
            .to_string_lossy()
            .into_owned();

        let journal = tmp.path().join("journal.jsonl");
        let error = purge_proven_cache_trash(tmp.path(), &approved, &journal, 10).unwrap_err();

        assert_eq!(error, "cache-trash-candidate-not-direct-child");
        assert!(trash.join("_cacache").exists());
        assert!(!journal.exists());
    }

    #[cfg(unix)]
    #[test]
    fn cleanup_rejects_symlinked_catalog_root_without_touching_outside_data() {
        let tmp = tempfile::tempdir().unwrap();
        let bases = fake_bases(tmp.path());
        let outside = tmp.path().join("outside");
        fs::create_dir(&outside).unwrap();
        let outside_file = outside.join("outside.bin");
        fs::write(&outside_file, b"outside").unwrap();
        std::os::unix::fs::symlink(&outside, &bases.temp).unwrap();
        let journal = tmp.path().join("journal.jsonl");

        let error = clean_cache_contents_inner(&bases, &bases.temp, &[], &journal, 1)
            .err()
            .expect("symlink root must be rejected");

        assert_eq!(error, "cache-root-not-current-or-safe");
        assert_eq!(fs::read(&outside_file).unwrap(), b"outside");
    }
}
