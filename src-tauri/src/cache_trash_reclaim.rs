use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

use crate::cache_cleanup::{
    approval_phrase_for_candidates as shared_approval_phrase, CacheTrashCandidate,
    CacheTrashPurgeExecution, CacheTrashPurgeResult, CacheTrashSnapshot,
};

const REVIEW_SCHEMA_KIND: &str = "disksage.cache-trash-review";
const REVIEW_SCHEMA_VERSION: u32 = 1;
const MAX_APPROVED_CANDIDATES: usize = 9;

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

pub fn approval_phrase_for_candidates(candidates: &[CacheTrashCandidate]) -> String {
    shared_approval_phrase(candidates)
}

pub fn review_for_home(home: &Path) -> CacheTrashReview {
    let supported = !cfg!(target_os = "windows");
    let candidates = if supported {
        crate::cache_cleanup::proven_cache_trash_candidates(home)
    } else {
        Vec::new()
    };
    CacheTrashReview {
        schema_kind: REVIEW_SCHEMA_KIND.into(),
        schema_version: REVIEW_SCHEMA_VERSION,
        supported,
        approval_phrase: (!candidates.is_empty())
            .then(|| approval_phrase_for_candidates(&candidates)),
        candidates,
        notice: (!supported).then(|| "cache-trash-native-discovery-unsupported".into()),
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
    if confirmation_phrase != approval_phrase_for_candidates(approved) {
        return Err("cache-trash-confirmation-mismatch".into());
    }
    let snapshot = CacheTrashSnapshot {
        candidates: sorted_candidates(approved),
        approval_phrase: confirmation_phrase.to_owned(),
    };
    crate::cache_cleanup::purge_proven_cache_trash(home, journal_path, now_ms, &snapshot)
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
