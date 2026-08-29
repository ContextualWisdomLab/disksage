use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::{commands::CleanResult, rules, safety};

fn sort_targets(targets: &mut Vec<rules::CacheTarget>) {
    targets.sort_by(|left, right| left.path.cmp(&right.path));
}

/// Local caches observed during the current low-disk incident and safe to regenerate.
/// npm's content-addressed cache is rebuilt by npm on demand; it is included only after the same
/// per-child identity and active-use checks as the other caches.
pub const AUTO_REGENERABLE_CACHE_IDS: [&str; 13] = [
    "npm-cache",
    "pip-cache",
    "pnpm-cache",
    "adobe-cache",
    "edge-cache",
    "uv-cache",
    "node-cache",
    "trivy-cache",
    "appmap-download-cache",
    "superset-http-cache",
    "superset-code-cache",
    "playwright-cache",
    "macos-app-support-cache",
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
const OBSERVED_UPDATER_CACHE_NAMES: [&str; 5] = [
    "hyosungitxmessenger-updater",
    "shure.motiv-updater",
    "cursor-updater",
    "reason-plus-companion-app-updater",
    "@mendeley-internaldesktop-reference-manager-updater",
];
const MAX_CACHE_TRASH_ENTRIES: usize = 1_000_000;

/// A cache directory already in OS Trash whose structure is still recognizable without reading
/// user file contents. Permanent removal is intentionally limited to these signatures.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CacheTrashCandidate {
    pub name: String,
    pub path: String,
    pub bytes: u64,
    pub signature: String,
    pub object_id: String,
    pub modified_ms: u64,
    pub manifest_fingerprint: String,
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

fn looks_like_updater_download_cache(path: &Path) -> bool {
    let Ok(root_entries) = std::fs::read_dir(path) else {
        return false;
    };
    let Ok(root_entries) = root_entries.collect::<Result<Vec<_>, _>>() else {
        return false;
    };
    let root_names: Vec<_> = root_entries
        .into_iter()
        .map(|entry| entry.file_name())
        .collect();
    if root_names.len() != 1 || root_names[0] != "pending" {
        return false;
    }
    let pending = path.join("pending");
    let Ok(entries) = std::fs::read_dir(&pending) else {
        return false;
    };
    let mut update_info = false;
    let mut archives = 0usize;
    let Ok(entries) = entries.collect::<Result<Vec<_>, _>>() else {
        return false;
    };
    for entry in entries {
        let Ok(metadata) = std::fs::symlink_metadata(entry.path()) else {
            return false;
        };
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return false;
        }
        if entry.file_name() == "update-info.json" {
            update_info = true;
        } else if entry.path().extension().is_some_and(|extension| extension == "zip") {
            archives += 1;
        } else {
            return false;
        }
    }
    update_info && archives == 1
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
        name if OBSERVED_UPDATER_CACHE_NAMES.contains(&name)
            && looks_like_updater_download_cache(path) =>
        {
            "electron-updater-download-cache"
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

/// Return only direct OS-Trash children whose cache signature is proven by structure and whose
/// size can be bounded without following symlinks or reading file contents.
pub fn proven_cache_trash_candidates(home: &Path) -> Vec<CacheTrashCandidate> {
    let trash = home.join(".Trash");
    let Ok(entries) = std::fs::read_dir(&trash) else {
        return Vec::new();
    };
    let mut candidates = Vec::new();
    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !PROVEN_CACHE_TRASH_NAMES.contains(&name.as_str())
            && !OBSERVED_UPDATER_CACHE_NAMES.contains(&name.as_str())
        {
            continue;
        }
        let path = entry.path();
        let Some(signature) = looks_like_proven_cache_trash(&path, &name) else {
            continue;
        };
        let mut count = 0;
        let Ok(_) = bounded_tree_size(&path, &mut count) else {
            continue;
        };
        let Ok(target) = rules::cache_target(&path) else {
            continue;
        };
        candidates.push(CacheTrashCandidate {
            name,
            path: path.to_string_lossy().into_owned(),
            bytes: target.bytes,
            signature: signature.into(),
            object_id: target.object_id,
            modified_ms: target.modified_ms,
            manifest_fingerprint: target.manifest_fingerprint,
        });
    }
    candidates.sort_by(|left, right| left.path.cmp(&right.path));
    candidates
}

/// Permanently remove only the proven cache directories in OS Trash. The explicit CLI flag is the
/// approval boundary; each object is rechecked immediately before removal and journaled.
pub fn purge_proven_cache_trash(
    home: &Path,
    journal_path: &Path,
    now_ms: u64,
) -> Result<Vec<CacheTrashPurgeResult>, String> {
    let planned = proven_cache_trash_candidates(home);
    let mut results = Vec::with_capacity(planned.len());
    for candidate in planned {
        let path = PathBuf::from(&candidate.path);
        let outcome = if looks_like_proven_cache_trash(&path, &candidate.name)
            .is_some_and(|signature| signature == candidate.signature)
        {
            safety::permanent_delete_dir_if_identity(
                &path,
                &candidate.object_id,
                candidate.bytes,
                candidate.modified_ms,
                &candidate.manifest_fingerprint,
                journal_path,
                now_ms,
            )
            .map_err(|error| error.to_string())
        } else {
            Err("cache-trash-signature-changed".into())
        };
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

pub(crate) fn catalog_cache_targets(
    cache_id: &str,
    path: &Path,
) -> Result<Vec<rules::CacheTarget>, String> {
    let mut targets = if cache_id == "macos-app-support-cache" {
        rules::named_cache_targets(path, &OBSERVED_UPDATER_CACHE_NAMES)?
    } else {
        rules::cache_targets(path)?
    };
    if cache_id == "macos-app-support-cache" {
        targets.retain(|target| {
            let path = Path::new(&target.path);
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| OBSERVED_UPDATER_CACHE_NAMES.contains(&name))
                && looks_like_updater_download_cache(path)
        });
    }
    Ok(targets)
}

pub(crate) fn clean_cache_contents_inner(
    bases: &rules::BaseDirs,
    dir: &Path,
    requested_targets: &[rules::CacheTarget],
    journal_path: &Path,
    now_ms: u64,
    permanent_directories: bool,
) -> Result<Vec<CleanResult>, String> {
    clean_cache_contents_inner_for_id(
        bases,
        dir,
        requested_targets,
        journal_path,
        now_ms,
        permanent_directories,
        None,
    )
}

fn clean_cache_contents_inner_for_id(
    bases: &rules::BaseDirs,
    dir: &Path,
    requested_targets: &[rules::CacheTarget],
    journal_path: &Path,
    now_ms: u64,
    permanent_directories: bool,
    cache_id: Option<&str>,
) -> Result<Vec<CleanResult>, String> {
    if !rules::is_catalog_path(bases, dir) {
        return Err("cache-root-not-current-or-safe".into());
    }
    let mut expected = requested_targets.to_vec();
    sort_targets(&mut expected);
    let mut current = match cache_id {
        Some(cache_id) => catalog_cache_targets(cache_id, dir)?,
        None => rules::cache_targets(dir)?,
    };
    sort_targets(&mut current);
    if current != expected {
        return Err("cache-cleanup-targets-stale".into());
    }
    if permanent_directories
        && expected.iter().any(|target| {
            std::fs::symlink_metadata(&target.path)
                .map(|metadata| !metadata.is_dir() || metadata.file_type().is_symlink())
                .unwrap_or(true)
        })
    {
        return Err("permanent-cache-target-type-unsupported".into());
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
                if permanent_directories {
                    crate::safety::PERMANENT_DIRECTORY_ACTIVE_USE_TIMEOUT_MS
                } else {
                    crate::reclaim::ACTIVE_USE_PROBE_TIMEOUT_MS
                },
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
            let path = Path::new(&target.path);
            let result = if permanent_directories {
                if !std::fs::symlink_metadata(path)
                    .is_ok_and(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink())
                {
                    return CleanResult {
                        path: target.path,
                        ok: false,
                        error: "permanent-cache-target-type-unsupported".into(),
                    };
                }
                safety::permanent_delete_dir_if_identity(
                    path,
                    &target.object_id,
                    target.bytes,
                    target.modified_ms,
                    &target.manifest_fingerprint,
                    journal_path,
                    now_ms,
                )
            } else {
                safety::trash_delete_cache_target_if_identity(
                    path,
                    &target.object_id,
                    target.bytes,
                    target.modified_ms,
                    &target.manifest_fingerprint,
                    journal_path,
                    now_ms,
                )
            };
            match result {
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
            match catalog_cache_targets(&candidate.id, &path) {
                Ok(targets) if targets.is_empty() => Vec::new(),
                Ok(targets) => {
                    clean_cache_contents_inner_for_id(
                        bases,
                        &path,
                        &targets,
                        journal_path,
                        now_ms,
                        false,
                        Some(&candidate.id),
                    )
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

/// Reclaim one named catalog cache through the existing identity and active-use checks.
/// Permanent deletion is limited to Gradle's regenerable cache-only roots.
pub fn clean_catalog_cache_headless(
    cache_id: &str,
    journal_path: &Path,
    now_ms: u64,
    permanent: bool,
) -> Result<Vec<CleanResult>, String> {
    let bases = rules::BaseDirs::from_env().ok_or("cache-base-directories-unavailable")?;
    if permanent
        && !["gradle-cache", "gradle-wrapper-cache", "gradle-jdk-cache", "gradle-daemon-cache"]
            .contains(&cache_id)
    {
        return Err("permanent-cache-id-not-approved".into());
    }
    let path = rules::cache_catalog_path(&bases, cache_id)
        .ok_or_else(|| "cache-id-not-catalogued".to_string())?;
    let targets = catalog_cache_targets(cache_id, &path)?;
    clean_cache_contents_inner_for_id(
        &bases,
        &path,
        &targets,
        journal_path,
        now_ms,
        permanent,
        Some(cache_id),
    )
}

/// Move only inactive, unchanged npx environments to OS Trash. Package downloads are regenerable
/// and every directory remains identity-bound, active-use checked, and journaled. A missing npx
/// cache is an empty successful cleanup rather than an operational failure.
pub fn clean_inactive_npx_environments_headless(
    journal_path: &Path,
    now_ms: u64,
) -> Result<Vec<CleanResult>, String> {
    let bases = rules::BaseDirs::from_env().ok_or("cache-base-directories-unavailable")?;
    let npx = rules::cache_catalog_path(&bases, "npm-cache")
        .ok_or("npm-cache-catalog-unavailable")?
        .join("_npx");
    match std::fs::symlink_metadata(&npx) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err("cache-root-metadata-unavailable".into()),
    }
    let targets = rules::cache_targets(&npx)?;
    clean_cache_contents_inner(&bases, &npx, &targets, journal_path, now_ms, false)
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
        false,
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

    #[test]
    fn permanent_catalog_cleanup_is_gradle_only() {
        let tmp = tempfile::tempdir().unwrap();
        let error = clean_catalog_cache_headless(
            "npm-cache",
            &tmp.path().join("journal.jsonl"),
            1,
            true,
        )
        .err()
        .expect("non-Gradle permanent cache cleanup must fail");
        assert_eq!(error, "permanent-cache-id-not-approved");
    }

    #[test]
    fn cleanup_rejects_non_catalog_root() {
        let tmp = tempfile::tempdir().unwrap();
        let bases = fake_bases(tmp.path());
        fs::create_dir(&bases.temp).unwrap();
        let journal = tmp.path().join("journal.jsonl");

        let error = clean_cache_contents_inner(&bases, tmp.path(), &[], &journal, 1, false)
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

        let error = clean_cache_contents_inner(&bases, &bases.temp, &targets, &journal, 1, false)
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

    #[cfg(target_os = "macos")]
    #[test]
    fn automatic_node_cache_cleanup_moves_directories_to_trash() {
        let tmp = tempfile::tempdir().unwrap();
        let bases = fake_bases(tmp.path());
        let corepack = bases.local_data.join("node/corepack");
        fs::create_dir_all(&corepack).unwrap();
        fs::write(corepack.join("archive.bin"), b"regenerable").unwrap();
        let journal = tmp.path().join("journal.jsonl");

        let results = clean_regenerable_caches_inner(&bases, &journal, 7);

        assert_eq!(results.len(), 1);
        assert!(results[0].ok);
        assert!(!corepack.exists());
        let journal_text = fs::read_to_string(journal).unwrap();
        assert!(journal_text.contains("\"op\":\"trash_delete\""));
        assert!(!journal_text.contains("permanent_generated_directory_delete"));
    }

    #[test]
    fn proven_cache_trash_requires_signature_and_journals_purge() {
        let tmp = tempfile::tempdir().unwrap();
        let trash = tmp.path().join(".Trash");
        fs::create_dir(&trash).unwrap();
        let npm = trash.join("_cacache");
        fs::create_dir_all(npm.join("content-v2")).unwrap();
        fs::create_dir(npm.join("tmp")).unwrap();
        fs::write(npm.join("content-v2").join("entry"), b"cache").unwrap();
        let unrelated = trash.join("Default");
        fs::create_dir(&unrelated).unwrap();
        fs::create_dir(unrelated.join("Cache")).unwrap();

        let candidates = proven_cache_trash_candidates(tmp.path());
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].signature, "npm-cacache");
        assert_eq!(candidates[0].bytes, 5);

        let journal = tmp.path().join("journal.jsonl");
        let results = purge_proven_cache_trash(tmp.path(), &journal, 7).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].purged);
        assert!(!npm.exists());
        let journal_text = fs::read_to_string(journal).unwrap();
        assert!(journal_text.contains("permanent_generated_directory_delete"));
        assert!(journal_text.contains("\"outcome\":\"ok\""));
    }

    #[test]
    fn updater_trash_requires_an_observed_name_and_pending_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let trash = tmp.path().join(".Trash");
        fs::create_dir(&trash).unwrap();
        let updater = trash.join("cursor-updater");
        fs::create_dir(&updater).unwrap();
        assert!(proven_cache_trash_candidates(tmp.path()).is_empty());

        fs::create_dir(updater.join("pending")).unwrap();
        fs::write(updater.join("pending/update-info.json"), b"{}").unwrap();
        fs::write(updater.join("pending/update.zip"), b"archive").unwrap();
        let candidates = proven_cache_trash_candidates(tmp.path());
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].signature, "electron-updater-download-cache");

        let unobserved = trash.join("unknown-updater");
        fs::create_dir(&unobserved).unwrap();
        fs::create_dir(unobserved.join("pending")).unwrap();
        fs::write(unobserved.join("pending/update-info.json"), b"{}").unwrap();
        fs::write(unobserved.join("pending/update.zip"), b"archive").unwrap();
        assert_eq!(proven_cache_trash_candidates(tmp.path()).len(), 1);

        fs::write(updater.join("pending/user-file.txt"), b"keep").unwrap();
        assert!(proven_cache_trash_candidates(tmp.path()).is_empty());
    }

    #[test]
    fn automatic_app_support_cleanup_selects_only_observed_updater_archives() {
        let tmp = tempfile::tempdir().unwrap();
        let updater = tmp.path().join("cursor-updater");
        fs::create_dir_all(updater.join("pending")).unwrap();
        fs::write(updater.join("pending/update-info.json"), b"{}").unwrap();
        fs::write(updater.join("pending/update.zip"), b"archive").unwrap();
        let unrelated = tmp.path().join("unrelated-cache");
        fs::create_dir(&unrelated).unwrap();
        fs::write(unrelated.join("cache.bin"), b"keep").unwrap();

        let targets = catalog_cache_targets("macos-app-support-cache", tmp.path()).unwrap();
        assert_eq!(targets.len(), 1);
        assert!(targets[0].path.ends_with("cursor-updater"));
    }

    #[cfg(unix)]
    #[test]
    fn unrelated_unreadable_cache_does_not_block_updater_discovery() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().unwrap();
        let updater = tmp.path().join("cursor-updater");
        fs::create_dir_all(updater.join("pending")).unwrap();
        fs::write(updater.join("pending/update-info.json"), b"{}").unwrap();
        fs::write(updater.join("pending/update.zip"), b"archive").unwrap();
        let unrelated = tmp.path().join("unrelated-cache");
        fs::create_dir(&unrelated).unwrap();
        fs::set_permissions(&unrelated, fs::Permissions::from_mode(0o000)).unwrap();

        let targets = catalog_cache_targets("macos-app-support-cache", tmp.path()).unwrap();

        fs::set_permissions(&unrelated, fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(targets.len(), 1);
        assert!(targets[0].path.ends_with("cursor-updater"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn automatic_app_support_cleanup_preserves_unrelated_sibling() {
        let tmp = tempfile::tempdir().unwrap();
        let bases = fake_bases(tmp.path());
        let root = bases.home.join("Library/Application Support/Caches");
        let updater = root.join("cursor-updater");
        fs::create_dir_all(updater.join("pending")).unwrap();
        fs::write(updater.join("pending/update-info.json"), b"{}").unwrap();
        fs::write(updater.join("pending/update.zip"), b"archive").unwrap();
        let unrelated = root.join("unrelated-cache");
        fs::create_dir(&unrelated).unwrap();
        fs::write(unrelated.join("cache.bin"), b"keep").unwrap();
        let targets = catalog_cache_targets("macos-app-support-cache", &root).unwrap();

        let results = clean_cache_contents_inner_for_id(
            &bases,
            &root,
            &targets,
            &tmp.path().join("journal.jsonl"),
            1,
            false,
            Some("macos-app-support-cache"),
        )
        .unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].ok);
        assert!(!updater.exists());
        assert_eq!(fs::read(unrelated.join("cache.bin")).unwrap(), b"keep");
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

        let error = clean_cache_contents_inner(&bases, &bases.temp, &[], &journal, 1, false)
            .err()
            .expect("symlink root must be rejected");
        assert_eq!(error, "cache-root-not-current-or-safe");
        assert_eq!(fs::read(&outside_file).unwrap(), b"outside");
    }
}
