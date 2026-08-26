use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::cache_cleanup::{CacheTrashCandidate, CacheTrashPurgeExecution, CacheTrashPurgeResult};

const REVIEW_SCHEMA_KIND: &str = "disksage.cache-trash-review";
const REVIEW_SCHEMA_VERSION: u32 = 1;
const MAX_APPROVED_CANDIDATES: usize = 9;
const PERMANENT_DELETE_UNAVAILABLE: &str =
    "cache-trash-identity-bound-permanent-delete-unavailable";
#[cfg(not(target_os = "macos"))]
const NATIVE_REVIEW_MACOS_ONLY: &str = "cache-trash-native-review-macos-only";

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
    let trash = home.join(".Trash");
    std::fs::symlink_metadata(trash)
        .is_ok_and(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink())
}

#[cfg(target_os = "macos")]
fn macos_review(home: &Path) -> CacheTrashReview {
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
    let notice = (!candidates.is_empty()).then(|| PERMANENT_DELETE_UNAVAILABLE.into());
    CacheTrashReview {
        schema_kind: REVIEW_SCHEMA_KIND.into(),
        schema_version: REVIEW_SCHEMA_VERSION,
        supported: true,
        candidates,
        approval_phrase: None,
        notice,
    }
}

pub fn review_for_home(home: &Path) -> CacheTrashReview {
    #[cfg(target_os = "macos")]
    {
        macos_review(home)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = home;
        CacheTrashReview {
            schema_kind: REVIEW_SCHEMA_KIND.into(),
            schema_version: REVIEW_SCHEMA_VERSION,
            supported: false,
            candidates: Vec::new(),
            approval_phrase: None,
            notice: Some(NATIVE_REVIEW_MACOS_ONLY.into()),
        }
    }
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

/// Refuse irreversible removal until DiskSage has a primitive whose final deletion syscall remains
/// bound to the exact reviewed filesystem object. A pathname check immediately before
/// `remove_dir_all` is still vulnerable to same-user replacement between that check and the final
/// recursive deletion, even if the reviewed root was previously moved through a staging pathname.
fn permanently_remove_identity_bound(
    path: &Path,
    expected_object_id: &str,
    _now_ms: u64,
) -> Result<(), String> {
    let actual = crate::safety::filesystem_object_id(path)
        .map_err(|_| "cache-trash-approved-candidate-changed".to_string())?;
    if actual != expected_object_id {
        return Err("cache-trash-approved-candidate-changed".into());
    }
    Err(PERMANENT_DELETE_UNAVAILABLE.into())
}

/// Evaluate only candidates in the operator-reviewed snapshot.
///
/// The current Trash is rescanned only to revalidate each approved object. Newly appearing proven
/// caches can never expand deletion authority because iteration is over `approved`, not the fresh
/// discovery result. The reviewed phrase binds each root filesystem identity. Permanent deletion
/// currently fails closed because the standard pathname-recursive primitive cannot preserve that
/// identity through the final irreversible syscall boundary.
#[cfg(target_os = "macos")]
pub fn purge_approved_cache_trash(
    home: &Path,
    approved: &[CacheTrashCandidate],
    confirmation_phrase: &str,
    journal_path: &Path,
    now_ms: u64,
) -> Result<Vec<CacheTrashPurgeResult>, String> {
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

#[cfg(not(target_os = "macos"))]
pub fn purge_approved_cache_trash(
    _home: &Path,
    _approved: &[CacheTrashCandidate],
    _confirmation_phrase: &str,
    _journal_path: &Path,
    _now_ms: u64,
) -> Result<Vec<CacheTrashPurgeResult>, String> {
    Err("cache-trash-native-discovery-unsupported".into())
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
            std::os::unix::fs::symlink(external.path(), home.path().join(".Trash")).unwrap();
            assert!(!native_trash_root_is_safe(home.path()));
        }
        #[cfg(not(unix))]
        {
            let _ = external;
        }
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn unsupported_platform_never_pretends_dot_trash_is_native() {
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir(home.path().join(".Trash")).unwrap();
        let review = review_for_home(home.path());
        assert!(!review.supported);
        assert!(review.candidates.is_empty());
        assert!(review.approval_phrase.is_none());
        assert_eq!(review.notice.as_deref(), Some(NATIVE_REVIEW_MACOS_ONLY));
    }
}
