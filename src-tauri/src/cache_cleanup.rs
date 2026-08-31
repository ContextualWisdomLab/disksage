use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::{commands::CleanResult, rules, safety};

fn sort_targets(targets: &mut Vec<rules::CacheTarget>) {
    targets.sort_by(|left, right| left.path.cmp(&right.path));
}

/// Local caches observed during the current low-disk incident and safe to regenerate.
/// npm's content-addressed cache is rebuilt by npm on demand; it is included only after the same
/// per-child identity and active-use checks as the other caches.
pub const AUTO_REGENERABLE_CACHE_IDS: [&str; 11] = [
    "npm-cache",
    "pnpm-cache",
    "adobe-cache",
    "edge-cache",
    "edge-code-sign-clones",
    "uv-cache",
    "trivy-cache",
    "appmap-download-cache",
    "superset-http-cache",
    "superset-code-cache",
    "playwright-cache",
];

const PROVEN_CACHE_TRASH_NAMES: [&str; 13] = [
    "_cacache",
    "v11",
    "Default",
    "simple-v21",
    "simple-v22",
    "simple-v24",
    "typequest",
    "wheels-v6",
    "sdists-v9",
    "builds-v0",
    "git-v0",
    "archive-v0",
    "db",
];
const MAX_CACHE_TRASH_ENTRIES: usize = 1_000_000;
// Large package caches need longer than the interactive worktree probe while retaining the same
// recursive open-handle evidence and fail-closed timeout behavior.
const CACHE_ACTIVE_USE_PROBE_TIMEOUT_MS: u64 = 30_000;

fn remaining_probe_timeout_ms(elapsed: Duration) -> Option<u64> {
    Duration::from_millis(CACHE_ACTIVE_USE_PROBE_TIMEOUT_MS)
        .checked_sub(elapsed)
        .and_then(|remaining| u64::try_from(remaining.as_millis()).ok())
        .filter(|remaining| *remaining > 0)
}

/// A cache directory already in OS Trash whose structure is still recognizable without reading
/// user file contents. Permanent removal is intentionally limited to these signatures.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CacheTrashCandidate {
    pub name: String,
    pub path: String,
    pub bytes: u64,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UvCachePruneResult {
    pub cache_path: String,
    pub bytes_before: u64,
    pub bytes_after: u64,
    pub observed_reduction_bytes: u64,
    pub status_code: i32,
    pub executed: bool,
}

#[cfg(target_os = "macos")]
fn fixed_uv_path() -> Result<PathBuf, String> {
    use std::os::unix::fs::PermissionsExt;

    for link in ["/opt/homebrew/bin/uv", "/usr/local/bin/uv"] {
        let Ok(path) = std::fs::canonicalize(link) else {
            continue;
        };
        let allowed = path.starts_with("/opt/homebrew/Cellar/uv/")
            || path.starts_with("/usr/local/Cellar/uv/");
        let metadata = std::fs::symlink_metadata(&path).ok();
        if allowed
            && metadata.is_some_and(|metadata| {
                metadata.is_file()
                    && !metadata.file_type().is_symlink()
                    && metadata.permissions().mode() & 0o111 != 0
            })
        {
            return Ok(path);
        }
    }
    Err("uv-cache-prune-executable-unavailable".into())
}

#[cfg(target_os = "macos")]
fn private_uv_copy(source_path: &Path) -> Result<tempfile::TempDir, String> {
    use std::io::{Seek, SeekFrom};
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let mut source = std::fs::File::open(source_path)
        .map_err(|_| "uv-cache-prune-executable-unavailable".to_string())?;
    let opened = source
        .metadata()
        .map_err(|_| "uv-cache-prune-executable-unavailable".to_string())?;
    let current = std::fs::symlink_metadata(source_path)
        .map_err(|_| "uv-cache-prune-executable-unavailable".to_string())?;
    if !opened.is_file()
        || !current.is_file()
        || current.file_type().is_symlink()
        || opened.dev() != current.dev()
        || opened.ino() != current.ino()
    {
        return Err("uv-cache-prune-executable-identity-changed".into());
    }
    source
        .seek(SeekFrom::Start(0))
        .map_err(|_| "uv-cache-prune-executable-copy-failed".to_string())?;
    let directory = tempfile::Builder::new()
        .prefix("disksage-uv-")
        .tempdir()
        .map_err(|_| "uv-cache-prune-private-copy-unavailable".to_string())?;
    std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700))
        .map_err(|_| "uv-cache-prune-private-copy-unavailable".to_string())?;
    let destination = directory.path().join("uv");
    let mut copy = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&destination)
        .map_err(|_| "uv-cache-prune-private-copy-unavailable".to_string())?;
    std::io::copy(&mut source, &mut copy)
        .map_err(|_| "uv-cache-prune-executable-copy-failed".to_string())?;
    copy.sync_all()
        .map_err(|_| "uv-cache-prune-executable-copy-failed".to_string())?;
    std::fs::set_permissions(&destination, std::fs::Permissions::from_mode(0o700))
        .map_err(|_| "uv-cache-prune-private-copy-unavailable".to_string())?;
    Ok(directory)
}

#[cfg(target_os = "macos")]
fn run_private_uv_prune(executable: &Path, cache: &Path) -> Result<i32, String> {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    use std::thread;

    let mut command = Command::new(executable);
    command
        .args(["cache", "prune", "--cache-dir"])
        .arg(cache)
        .args(["--no-config", "--no-progress"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    unsafe {
        command.pre_exec(|| {
            if libc::setpgid(0, 0) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = command
        .spawn()
        .map_err(|_| "uv-cache-prune-spawn-failed".to_string())?;
    let deadline = Instant::now() + Duration::from_secs(300);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status.code().unwrap_or(-1)),
            Ok(None) if Instant::now() >= deadline => {
                unsafe {
                    let _ = libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL);
                }
                let _ = child.kill();
                let _ = child.wait();
                return Err("uv-cache-prune-timeout".into());
            }
            Ok(None) => thread::sleep(Duration::from_millis(50)),
            Err(_) => {
                unsafe {
                    let _ = libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL);
                }
                let _ = child.kill();
                let _ = child.wait();
                return Err("uv-cache-prune-wait-failed".into());
            }
        }
    }
}

/// Run uv's native dangling-entry prune without `--force`, from a private copy of the verified
/// Homebrew executable. uv retains environments that are still in use.
pub fn prune_uv_cache_headless(
    journal_path: &Path,
    now_ms: u64,
) -> Result<UvCachePruneResult, String> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (journal_path, now_ms);
        return Err("uv-cache-prune-unsupported-platform".into());
    }
    #[cfg(target_os = "macos")]
    {
        let bases = rules::BaseDirs::from_env().ok_or("cache-base-directories-unavailable")?;
        let cache = rules::cache_candidates(&bases)
            .into_iter()
            .find(|candidate| candidate.id == "uv-cache" && candidate.exists)
            .map(|candidate| PathBuf::from(candidate.path))
            .ok_or("uv-cache-prune-cache-unavailable")?;
        let mut entries = 0;
        let bytes_before = bounded_tree_size(&cache, &mut entries, true)?;
        let source = fixed_uv_path()?;
        let private = private_uv_copy(&source)?;
        let mut journal = safety::JournalEntry {
            ts_ms: now_ms,
            op: "uv_cache_prune".into(),
            path: cache.to_string_lossy().into_owned(),
            bytes: bytes_before,
            outcome: "pending".into(),
        };
        safety::journal_append(journal_path, &journal).map_err(|error| error.to_string())?;
        let status_code = match run_private_uv_prune(&private.path().join("uv"), &cache) {
            Ok(status_code) => status_code,
            Err(error) => {
                journal.outcome = format!("error:{error}");
                safety::journal_append(journal_path, &journal)
                    .map_err(|journal_error| journal_error.to_string())?;
                return Err(error);
            }
        };
        let mut entries = 0;
        let bytes_after = bounded_tree_size(&cache, &mut entries, true)?;
        journal.outcome = if status_code == 0 {
            "ok"
        } else {
            "error:uv-exit-nonzero"
        }
        .into();
        safety::journal_append(journal_path, &journal).map_err(|error| error.to_string())?;
        Ok(UvCachePruneResult {
            cache_path: cache.to_string_lossy().into_owned(),
            bytes_before,
            bytes_after,
            observed_reduction_bytes: bytes_before.saturating_sub(bytes_after),
            status_code,
            executed: true,
        })
    }
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

fn native_trash_collision_suffix(suffix: &str) -> bool {
    !suffix.is_empty()
        && (suffix.bytes().all(|byte| byte.is_ascii_digit())
            || suffix.split('-').map(str::len).eq([2, 2, 2, 3])
                && suffix
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || byte == b'-'))
}

fn proven_cache_base_name(name: &str) -> Option<&str> {
    PROVEN_CACHE_TRASH_NAMES.iter().copied().find(|base| {
        name == *base
            || name
                .strip_prefix(base)
                .and_then(|suffix| suffix.strip_prefix(' '))
                .is_some_and(native_trash_collision_suffix)
    })
}

fn edge_code_sign_clone_name(name: &str) -> bool {
    let (base, collision) = name
        .split_once(' ')
        .map_or((name, None), |(base, suffix)| (base, Some(suffix)));
    let Some(suffix) = base.strip_prefix("code_sign_clone.") else {
        return false;
    };
    suffix.len() == 6
        && suffix.bytes().all(|byte| byte.is_ascii_alphanumeric())
        && collision.is_none_or(native_trash_collision_suffix)
}

fn looks_like_uv_archive_cache(path: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(path) else {
        return false;
    };
    let mut seen = false;
    for entry in entries {
        let Ok(entry) = entry else {
            return false;
        };
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let Ok(metadata) = entry.path().symlink_metadata() else {
            return false;
        };
        if !metadata.is_dir()
            || metadata.file_type().is_symlink()
            || name.len() != 16
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
        {
            return false;
        }
        seen = true;
    }
    seen
}

fn looks_like_proven_cache_trash(path: &Path, name: &str) -> Option<&'static str> {
    if edge_code_sign_clone_name(name) {
        let bundle = path.join("Microsoft Edge.app.bundle");
        let contents = bundle.join("Contents");
        return (direct_child_is_dir(path, "Microsoft Edge.app.bundle")
            && direct_child_is_dir(&bundle, "Contents")
            && direct_child_is_dir(&contents, "MacOS")
            && direct_child_is_dir(&contents, "_CodeSignature")
            && direct_child_is_file(&contents, "Info.plist"))
        .then_some("edge-code-sign-clone");
    }
    let base_name = proven_cache_base_name(name)?;
    let signature = match base_name {
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
        "simple-v21" | "simple-v22" | "simple-v24" if direct_child_is_dir(path, "pypi") => {
            "uv-simple-index-cache"
        }
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
        "git-v0"
            if direct_child_is_dir(path, "locks")
                && direct_child_is_dir(path, "checkouts")
                && direct_child_is_dir(path, "db") =>
        {
            "uv-git-cache"
        }
        "archive-v0" if looks_like_uv_archive_cache(path) => "uv-archive-cache",
        "db" if direct_child_is_file(path, "trivy.db")
            && direct_child_is_file(path, "metadata.json") =>
        {
            "trivy-database-cache"
        }
        _ => return None,
    };
    Some(signature)
}

fn bounded_tree_size(
    path: &Path,
    entries: &mut usize,
    allow_unfollowed_symlinks: bool,
) -> Result<u64, String> {
    *entries = entries.saturating_add(1);
    if *entries > MAX_CACHE_TRASH_ENTRIES {
        return Err("cache-trash-entry-limit-exceeded".into());
    }
    let metadata =
        std::fs::symlink_metadata(path).map_err(|_| "cache-trash-stat-failed".to_string())?;
    if metadata.file_type().is_symlink() {
        return allow_unfollowed_symlinks
            .then_some(0)
            .ok_or_else(|| "cache-trash-symlink-rejected".into());
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
        total = total.saturating_add(bounded_tree_size(
            &entry.path(),
            entries,
            allow_unfollowed_symlinks,
        )?);
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
        if proven_cache_base_name(&name).is_none() && !edge_code_sign_clone_name(&name) {
            continue;
        }
        let path = entry.path();
        let Some(signature) = looks_like_proven_cache_trash(&path, &name) else {
            continue;
        };
        let mut count = 0;
        let Ok(bytes) = bounded_tree_size(
            &path,
            &mut count,
            matches!(signature, "edge-code-sign-clone" | "uv-archive-cache"),
        ) else {
            continue;
        };
        candidates.push(CacheTrashCandidate {
            name,
            path: path.to_string_lossy().into_owned(),
            bytes,
            signature: signature.into(),
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
        let mut entry = crate::safety::JournalEntry {
            ts_ms: now_ms,
            op: "permanent_cache_trash_delete".into(),
            path: candidate.path.clone(),
            bytes: candidate.bytes,
            outcome: "pending".into(),
        };
        crate::safety::journal_append(journal_path, &entry).map_err(|error| error.to_string())?;
        let outcome = if looks_like_proven_cache_trash(&path, &candidate.name)
            .is_some_and(|signature| signature == candidate.signature)
        {
            match std::fs::remove_dir_all(&path) {
                Ok(()) => Ok(()),
                Err(error) => Err(error.to_string()),
            }
        } else {
            Err("cache-trash-signature-changed".into())
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

    let probe_started = Instant::now();
    Ok(expected
        .into_iter()
        .map(|target| {
            let Some(probe_timeout_ms) = remaining_probe_timeout_ms(probe_started.elapsed()) else {
                return CleanResult {
                    path: target.path,
                    ok: false,
                    error: "cache-target-active-use-evidence-incomplete".into(),
                };
            };
            // Probe each reviewed child independently: a live MCP/uv process must not prevent
            // reclaiming unrelated archives, within one bounded operation-wide evidence budget.
            let recursive = std::fs::symlink_metadata(&target.path)
                .map(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink())
                .unwrap_or(false);
            let active_use = crate::git_worktree::active_use_evidence(
                Path::new(&target.path),
                probe_timeout_ms,
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
            match safety::trash_delete_if_identity_in_catalog_root(
                Path::new(&target.path),
                dir,
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

    #[test]
    fn active_use_probe_budget_is_operation_wide() {
        assert_eq!(remaining_probe_timeout_ms(Duration::ZERO), Some(30_000));
        assert_eq!(
            remaining_probe_timeout_ms(Duration::from_secs(29)),
            Some(1_000)
        );
        assert_eq!(remaining_probe_timeout_ms(Duration::from_secs(30)), None);
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
        assert!(journal_text.contains("permanent_cache_trash_delete"));
        assert!(journal_text.contains("\"outcome\":\"ok\""));
    }

    #[test]
    fn proven_uv_git_cache_requires_all_native_directories() {
        let tmp = tempfile::tempdir().unwrap();
        let trash = tmp.path().join(".Trash");
        let git = trash.join("git-v0");
        fs::create_dir_all(git.join("locks")).unwrap();
        fs::create_dir(git.join("checkouts")).unwrap();
        assert!(proven_cache_trash_candidates(tmp.path()).is_empty());

        fs::create_dir(git.join("db")).unwrap();
        let candidates = proven_cache_trash_candidates(tmp.path());
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].signature, "uv-git-cache");
    }

    #[cfg(unix)]
    #[test]
    fn proven_uv_archive_cache_requires_native_keys_and_never_follows_symlinks() {
        let tmp = tempfile::tempdir().unwrap();
        let archive = tmp.path().join(".Trash/archive-v0");
        let entry = archive.join("Ab12_-cdEF34ghIJ");
        fs::create_dir_all(&entry).unwrap();
        let outside = tmp.path().join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("keep"), b"keep").unwrap();
        std::os::unix::fs::symlink(&outside, entry.join("linked-package")).unwrap();

        let candidates = proven_cache_trash_candidates(tmp.path());
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].signature, "uv-archive-cache");
        assert_eq!(candidates[0].bytes, 0);

        let results =
            purge_proven_cache_trash(tmp.path(), &tmp.path().join("journal.jsonl"), 9).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].purged);
        assert_eq!(fs::read(outside.join("keep")).unwrap(), b"keep");

        let invalid = tmp.path().join(".Trash/archive-v0");
        fs::create_dir_all(invalid.join("not-a-native-key!")).unwrap();
        assert!(proven_cache_trash_candidates(tmp.path()).is_empty());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn private_uv_prune_uses_only_fixed_non_force_arguments() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().unwrap();
        let executable = tmp.path().join("uv");
        fs::write(
            &executable,
            b"#!/bin/sh\n[ \"$1\" = cache ] && [ \"$2\" = prune ] && [ \"$3\" = --cache-dir ] && [ \"$5\" = --no-config ] && [ \"$6\" = --no-progress ]\n",
        )
        .unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        let cache = tmp.path().join("cache");
        fs::create_dir(&cache).unwrap();

        assert_eq!(run_private_uv_prune(&executable, &cache).unwrap(), 0);
    }

    #[test]
    fn proven_cache_accepts_only_native_trash_collision_names() {
        let tmp = tempfile::tempdir().unwrap();
        let trash = tmp.path().join(".Trash");
        for name in [
            "git-v0 2",
            "git-v0 14-56-42-563",
            "git-v0-old",
            "git-v0 2 old",
            "git-v01",
        ] {
            let git = trash.join(name);
            fs::create_dir_all(git.join("locks")).unwrap();
            fs::create_dir(git.join("checkouts")).unwrap();
            fs::create_dir(git.join("db")).unwrap();
        }

        let candidates = proven_cache_trash_candidates(tmp.path());
        assert_eq!(candidates.len(), 2);
        assert!(candidates
            .iter()
            .all(|candidate| candidate.signature == "uv-git-cache"));

        let wheels = trash.join("wheels-v6 14-56-42-563");
        fs::create_dir_all(wheels.join("pypi")).unwrap();
        let candidates = proven_cache_trash_candidates(tmp.path());
        assert_eq!(candidates.len(), 3);
        assert!(candidates.iter().any(|candidate| {
            candidate.name == "wheels-v6 14-56-42-563" && candidate.signature == "uv-wheel-cache"
        }));
    }

    #[cfg(unix)]
    #[test]
    fn edge_code_sign_clone_signature_purges_without_following_bundle_symlinks() {
        let tmp = tempfile::tempdir().unwrap();
        let trash = tmp.path().join(".Trash");
        let clone = trash.join("code_sign_clone.Ab12zZ");
        let contents = clone.join("Microsoft Edge.app.bundle/Contents");
        fs::create_dir_all(contents.join("MacOS")).unwrap();
        fs::create_dir(contents.join("_CodeSignature")).unwrap();
        fs::write(contents.join("Info.plist"), b"plist").unwrap();
        let outside = tmp.path().join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("keep"), b"keep").unwrap();
        std::os::unix::fs::symlink(&outside, contents.join("Frameworks")).unwrap();

        for invalid in ["code_sign_clone.short", "code_sign_clone.Ab12zZ.old"] {
            fs::create_dir_all(trash.join(invalid)).unwrap();
        }

        let candidates = proven_cache_trash_candidates(tmp.path());
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].signature, "edge-code-sign-clone");

        let results =
            purge_proven_cache_trash(tmp.path(), &tmp.path().join("journal.jsonl"), 8).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].purged);
        assert!(!clone.exists());
        assert_eq!(fs::read(outside.join("keep")).unwrap(), b"keep");
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
