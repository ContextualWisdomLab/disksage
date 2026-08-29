//! Read-only operator evidence for one catalog cache before any cleanup authority is requested.
//!
//! The preview resolves only a catalog ID, snapshots the exact direct-child manifests used by the
//! cleanup path, and attaches an active-use probe for each target. It never journals or mutates.

use std::path::Path;

const MAX_PREVIEW_TARGETS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogCacheTargetPreview {
    pub path: String,
    pub bytes: u64,
    pub modified_ms: u64,
    pub object_id: String,
    pub manifest_fingerprint: String,
    pub active_use: crate::git_worktree::GitWorktreeActiveUseEvidence,
}

/// Snapshot the exact targets and current active-use evidence for one catalog cache ID.
///
/// This is deliberately read-only. Callers receive the same target identity/manifest fields that
/// cleanup later revalidates plus an independent active-use assessment for operator inspection.
pub fn preview_catalog_cache_headless(
    cache_id: &str,
) -> Result<Vec<CatalogCacheTargetPreview>, String> {
    let bases = crate::rules::BaseDirs::from_env()
        .ok_or_else(|| "cache-base-directories-unavailable".to_string())?;
    let root = crate::rules::cache_catalog_path(&bases, cache_id)
        .ok_or_else(|| "cache-id-not-in-catalog".to_string())?;
    match std::fs::symlink_metadata(&root) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err("cache-target-metadata-unavailable".into()),
    }
    let targets = crate::cache_cleanup::catalog_cache_targets(cache_id, &root)?;
    if targets.len() > MAX_PREVIEW_TARGETS {
        return Err("cache-preview-target-limit-exceeded".into());
    }

    Ok(targets
        .into_iter()
        .map(|target| {
            let recursive = std::fs::symlink_metadata(&target.path)
                .map(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink())
                .unwrap_or(false);
            let active_use = crate::git_worktree::active_use_evidence(
                Path::new(&target.path),
                crate::reclaim::ACTIVE_USE_PROBE_TIMEOUT_MS,
                crate::reclaim::ACTIVE_USE_PROBE_MAX_PIDS,
                recursive,
            );
            CatalogCacheTargetPreview {
                path: target.path,
                bytes: target.bytes,
                modified_ms: target.modified_ms,
                object_id: target.object_id,
                manifest_fingerprint: target.manifest_fingerprint,
                active_use,
            }
        })
        .collect())
}
