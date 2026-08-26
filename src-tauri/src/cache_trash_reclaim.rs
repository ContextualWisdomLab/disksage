use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::cache_cleanup::{CacheTrashCandidate, CacheTrashPurgeExecution, CacheTrashPurgeResult};

const REVIEW_SCHEMA_KIND: &str = "disksage.cache-trash-review";
const REVIEW_SCHEMA_VERSION: u32 = 1;
const MAX_APPROVED_CANDIDATES: usize = 9;
static PURGE_STAGING_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CacheTrashReview {
    pub schema_kind: String,
    pub schema_version: u32,
    pub supported: bool,
    pub candidates: Vec<CacheTrashCandidate>,
    pub approval_phrase: Option<String>,
    pub notice: Option<String>,
}

fn sorted_candidates(candidates: &[CacheTrashCandidate]) -> Vec<CacheTrashCandidate> {
    let mut sorted = candidates.to_vec();
    sorted.sort_by(|left, right| left.path.cmp(&right.path));
    sorted
}

fn strict_candidate_identities(
    candidates: &[CacheTrashCandidate],
) -> Result<HashMap<String, String>, String> {
    let mut identities = HashMap::with_capacity(candidates.len());
    for candidate in candidates {
        let identity = crate::safety::filesystem_object_id(Path::new(&candidate.path))
            .map_err(|_| "cache-trash-approved-candidate-changed".to_string())?;
        identities.insert(candidate.path.clone(), identity);
    }
    Ok(identities)
}

fn phrase_for_candidates_and_identities(
    candidates: &[CacheTrashCandidate],
    identities: &HashMap<String, String>,
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage.cache-trash-reviewed-snapshot.v2\0");
    for candidate in sorted_candidates(candidates) {
        for field in [&candidate.name, &candidate.path, &candidate.signature] {
            hasher.update(&(field.len() as u64).to_le_bytes());
            hasher.update(field.as_bytes());
        }
        hasher.update(&candidate.bytes.to_le_bytes());
        let identity = identities
            .get(&candidate.path)
            .map(String::as_str)
            .unwrap_or("<object-identity-unavailable>");
        hasher.update(&(identity.len() as u64).to_le_bytes());
        hasher.update(identity.as_bytes());
    }
    format!(
        "DiskSage cache-trash reviewed snapshot {}",
        hasher.finalize().to_hex()
    )
}

/// Return an opaque approval phrase bound to both the reviewed candidate fields and the current
/// filesystem identity of each candidate root. The raw device/inode or platform file identity is
/// never exposed through the IPC contract.
pub fn approval_phrase_for_candidates(candidates: &[CacheTrashCandidate]) -> String {
    let identities = candidates
        .iter()
        .map(|candidate| {
            let identity = crate::safety::filesystem_object_id(Path::new(&candidate.path))
                .unwrap_or_else(|_| "<object-identity-unavailable>".into());
            (candidate.path.clone(), identity)
        })
        .collect::<HashMap<_, _>>();
    phrase_for_candidates_and_identities(candidates, &identities)
}

fn native_trash_root_is_safe(home: &Path) -> bool {
    let Some(trash) = crate::cache_cleanup::trash_directory(home) else {
        return false;
    };
    std::fs::symlink_metadata(trash)
        .is_ok_and(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink())
}

fn native_review(home: &Path) -> CacheTrashReview {
    if !native_trash_root_is_safe(home) {
        return CacheTrashReview {
            schema_kind: REVIEW_SCHEMA_KIND.into(),
            schema_version: REVIEW_SCHEMA_VERSION,
            supported: true,
            candidates: Vec::new(),
            approval_phrase: None,
            notice: Some("cache-trash-native-root-unsafe".into()),
        };
    }
    let mut candidates = crate::cache_cleanup::proven_cache_trash_candidates(home);
    candidates.retain(|candidate| {
        crate::safety::filesystem_object_id(Path::new(&candidate.path)).is_ok()
    });
    let approval_phrase =
        (!candidates.is_empty()).then(|| approval_phrase_for_candidates(&candidates));
    CacheTrashReview {
        schema_kind: REVIEW_SCHEMA_KIND.into(),
        schema_version: REVIEW_SCHEMA_VERSION,
        supported: true,
        candidates,
        approval_phrase,
        notice: None,
    }
}

pub fn review_for_home(home: &Path) -> CacheTrashReview {
    if cfg!(target_os = "windows") {
        return CacheTrashReview {
            schema_kind: REVIEW_SCHEMA_KIND.into(),
            schema_version: REVIEW_SCHEMA_VERSION,
            supported: false,
            candidates: Vec::new(),
            approval_phrase: None,
            notice: Some("cache-trash-native-discovery-unsupported".into()),
        };
    }
    native_review(home)
}

fn merge_purge_errors(operation_error: Option<String>, journal_error: Option<String>) -> String {
    match (operation_error, journal_error) {
        (None, None) => String::new(),
        (Some(operation), None) => operation,
        (None, Some(journal)) => format!("purged-but-journal-write-failed:{journal}"),
        (Some(operation), Some(journal)) => {
            format!("{operation};journal-write-failed:{journal}")
        }
    }
}

fn validate_approved_snapshot(approved: &[CacheTrashCandidate]) -> Result<(), String> {
    if approved.is_empty() || approved.len() > MAX_APPROVED_CANDIDATES {
        return Err("cache-trash-approved-snapshot-invalid".into());
    }
    let mut paths = HashSet::with_capacity(approved.len());
    for candidate in approved {
        if candidate.name.is_empty()
            || candidate.path.is_empty()
            || candidate.signature.is_empty()
            || !paths.insert(candidate.path.clone())
        {
            return Err("cache-trash-approved-snapshot-invalid".into());
        }
    }
    Ok(())
}

fn create_purge_staging_dir(path: &Path, now_ms: u64) -> Result<PathBuf, String> {
    let parent = path
        .parent()
        .ok_or_else(|| "cache-trash-approved-candidate-changed".to_string())?;
    let pid = std::process::id();
    for _ in 0..32 {
        let serial = PURGE_STAGING_COUNTER.fetch_add(1, Ordering::Relaxed);
        let staging = parent.join(format!(
            ".disksage-cache-purge-{}-{}-{}",
            pid, now_ms, serial
        ));
        match std::fs::create_dir(&staging) {
            Ok(()) => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(&staging, std::fs::Permissions::from_mode(0o700))
                        .map_err(|error| error.to_string())?;
                }
                return Ok(staging);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("cache-trash-staging-create-failed:{error}")),
        }
    }
    Err("cache-trash-staging-create-collision".into())
}

fn restore_staged_candidate(path: &Path, staged: &Path, staging_dir: &Path) -> Result<(), String> {
    let source_absent = matches!(
        std::fs::symlink_metadata(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound
    );
    if !source_absent {
        return Err("cache-trash-staged-candidate-retained-source-reappeared".into());
    }
    std::fs::rename(staged, path)
        .map_err(|error| format!("cache-trash-staged-restore-failed:{error}"))?;
    std::fs::remove_dir(staging_dir)
        .map_err(|error| format!("cache-trash-staging-cleanup-failed:{error}"))?;
    Ok(())
}

/// Permanently delete only the exact filesystem object whose identity was part of the reviewed
/// approval phrase. The candidate is atomically moved to a private sibling staging directory and
/// its identity is checked again after the rename, so a pathname replacement cannot win the final
/// mutation race and be deleted under the reviewed authority.
fn permanently_remove_identity_bound(
    path: &Path,
    expected_object_id: &str,
    now_ms: u64,
) -> Result<(), String> {
    let actual = crate::safety::filesystem_object_id(path)
        .map_err(|_| "cache-trash-approved-candidate-changed".to_string())?;
    if actual != expected_object_id {
        return Err("cache-trash-approved-candidate-changed".into());
    }
    let file_name = path
        .file_name()
        .ok_or_else(|| "cache-trash-approved-candidate-changed".to_string())?;
    let staging_dir = create_purge_staging_dir(path, now_ms)?;
    let staged = staging_dir.join(file_name);
    if let Err(error) = std::fs::rename(path, &staged) {
        let _ = std::fs::remove_dir(&staging_dir);
        return Err(format!("cache-trash-staging-move-failed:{error}"));
    }
    let moved_id = match crate::safety::filesystem_object_id(&staged) {
        Ok(identity) => identity,
        Err(_) => {
            return match restore_staged_candidate(path, &staged, &staging_dir) {
                Ok(()) => Err("cache-trash-approved-candidate-changed".into()),
                Err(restore_error) => Err(format!(
                    "cache-trash-approved-candidate-changed;{restore_error}"
                )),
            };
        }
    };
    if moved_id != expected_object_id {
        return match restore_staged_candidate(path, &staged, &staging_dir) {
            Ok(()) => Err("cache-trash-approved-candidate-changed".into()),
            Err(restore_error) => Err(format!(
                "cache-trash-approved-candidate-changed;{restore_error}"
            )),
        };
    }
    std::fs::remove_dir_all(&staged)
        .map_err(|error| format!("cache-trash-permanent-delete-failed:{error}"))?;
    std::fs::remove_dir(&staging_dir)
        .map_err(|error| format!("cache-trash-staging-cleanup-failed:{error}"))?;
    Ok(())
}

/// Permanently remove only candidates in the operator-reviewed snapshot.
///
/// The current Trash is rescanned only to revalidate each approved object. Newly appearing proven
/// caches can never expand deletion authority because iteration is over `approved`, not the fresh
/// discovery result. The reviewed phrase also binds each root filesystem identity, and the final
/// deletion is performed only after an atomic staging move preserves that exact identity.
pub fn purge_approved_cache_trash(
    home: &Path,
    approved: &[CacheTrashCandidate],
    confirmation_phrase: &str,
    journal_path: &Path,
    now_ms: u64,
) -> Result<Vec<CacheTrashPurgeResult>, String> {
    if cfg!(target_os = "windows") {
        return Err("cache-trash-native-discovery-unsupported".into());
    }
    validate_approved_snapshot(approved)?;
    if !native_trash_root_is_safe(home) {
        return Err("cache-trash-native-root-unsafe".into());
    }
    let approved_identities = strict_candidate_identities(approved)?;
    if confirmation_phrase != phrase_for_candidates_and_identities(approved, &approved_identities) {
        return Err("cache-trash-confirmation-mismatch".into());
    }

    let mut results = Vec::with_capacity(approved.len());
    for candidate in sorted_candidates(approved) {
        let current = crate::cache_cleanup::proven_cache_trash_candidates(home);
        let still_exact = current.iter().any(|observed| observed == &candidate);
        if !still_exact {
            results.push(CacheTrashPurgeResult {
                name: candidate.name,
                path: candidate.path,
                bytes: candidate.bytes,
                signature: candidate.signature,
                purged: false,
                error: "cache-trash-approved-candidate-changed".into(),
            });
            continue;
        }

        let path = PathBuf::from(&candidate.path);
        let expected_object_id = approved_identities
            .get(&candidate.path)
            .expect("validated cache-trash snapshot has one identity per path");
        let mut entry = crate::safety::JournalEntry {
            ts_ms: now_ms,
            op: "permanent_cache_trash_delete".into(),
            path: candidate.path.clone(),
            bytes: candidate.bytes,
            outcome: "pending".into(),
        };
        if let Err(error) = crate::safety::journal_append(journal_path, &entry) {
            results.push(CacheTrashPurgeResult {
                name: candidate.name,
                path: candidate.path,
                bytes: candidate.bytes,
                signature: candidate.signature,
                purged: false,
                error: format!("journal-write-failed:{error}"),
            });
            continue;
        }

        // Re-scan immediately before mutation. Exact equality rechecks path, signature and bounded
        // bytes; the identity-bound staging primitive then closes the replacement race between this
        // final observation and permanent deletion.
        let immediately_current = crate::cache_cleanup::proven_cache_trash_candidates(home);
        let outcome = if immediately_current
            .iter()
            .any(|observed| observed == &candidate)
        {
            permanently_remove_identity_bound(&path, expected_object_id, now_ms)
        } else {
            Err("cache-trash-approved-candidate-changed".into())
        };
        entry.outcome = match &outcome {
            Ok(()) => "ok".into(),
            Err(error) => format!("error:{error}"),
        };
        let journal_error = crate::safety::journal_append(journal_path, &entry)
            .err()
            .map(|error| error.to_string());
        let operation_error = outcome.as_ref().err().cloned();
        results.push(CacheTrashPurgeResult {
            name: candidate.name,
            path: candidate.path,
            bytes: candidate.bytes,
            signature: candidate.signature,
            purged: outcome.is_ok(),
            error: merge_purge_errors(operation_error, journal_error),
        });
    }
    Ok(results)
}

#[cfg(not(coverage))]
#[tauri::command]
pub fn review_proven_cache_trash() -> Result<CacheTrashReview, String> {
    let bases = crate::rules::BaseDirs::from_env().ok_or("cache-base-directories-unavailable")?;
    Ok(review_for_home(&bases.home))
}

#[cfg(not(coverage))]
#[tauri::command]
pub fn purge_proven_cache_trash(
    app: tauri::AppHandle,
    approved_candidates: Vec<CacheTrashCandidate>,
    confirmation_phrase: String,
) -> Result<CacheTrashPurgeExecution, String> {
    let bases = crate::rules::BaseDirs::from_env().ok_or("cache-base-directories-unavailable")?;
    let journal_path = crate::commands::journal_file_path(&app)?;
    let before =
        crate::volume_pressure::snapshot_volume(&bases.home, crate::commands::now_ms()).ok();
    let items = purge_approved_cache_trash(
        &bases.home,
        &approved_candidates,
        &confirmation_phrase,
        &journal_path,
        crate::commands::now_ms(),
    )?;
    let after =
        crate::volume_pressure::snapshot_volume(&bases.home, crate::commands::now_ms()).ok();
    let before_available_bytes = before.as_ref().map(|snapshot| snapshot.available_bytes);
    let after_available_bytes = after.as_ref().map(|snapshot| snapshot.available_bytes);
    let observed_available_gain_bytes = before_available_bytes
        .zip(after_available_bytes)
        .and_then(|(before, after)| after.checked_sub(before));
    Ok(CacheTrashPurgeExecution {
        schema_kind: "disksage.cache-trash-purge".into(),
        schema_version: 1,
        items,
        before_available_bytes,
        after_available_bytes,
        observed_available_gain_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phrase_is_order_independent_and_binds_candidate_fields() {
        let a = CacheTrashCandidate {
            name: "_cacache".into(),
            path: "/tmp/.Trash/_cacache".into(),
            bytes: 10,
            signature: "npm-cacache".into(),
        };
        let mut b = CacheTrashCandidate {
            name: "db".into(),
            path: "/tmp/.Trash/db".into(),
            bytes: 20,
            signature: "trivy-database-cache".into(),
        };
        assert_eq!(
            approval_phrase_for_candidates(&[a.clone(), b.clone()]),
            approval_phrase_for_candidates(&[b.clone(), a.clone()])
        );
        let original = approval_phrase_for_candidates(&[a.clone(), b.clone()]);
        b.bytes += 1;
        assert_ne!(original, approval_phrase_for_candidates(&[a, b]));
    }

    #[test]
    fn native_trash_root_rejects_symlink_identity() {
        let home = tempfile::tempdir().unwrap();
        let external = tempfile::tempdir().unwrap();
        #[cfg(unix)]
        {
            let trash = crate::cache_cleanup::trash_directory(home.path()).unwrap();
            std::os::unix::fs::symlink(external.path(), trash).unwrap();
            assert!(!native_trash_root_is_safe(home.path()));
        }
        #[cfg(not(unix))]
        {
            let _ = external;
        }
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn unsupported_platform_never_pretends_trash_is_native() {
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir(home.path().join(".Trash")).unwrap();
        let review = review_for_home(home.path());
        assert!(!review.supported);
        assert!(review.candidates.is_empty());
        assert!(review.approval_phrase.is_none());
        assert_eq!(
            review.notice.as_deref(),
            Some("cache-trash-native-discovery-unsupported")
        );
    }
}
