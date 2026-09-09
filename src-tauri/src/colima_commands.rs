//! Tauri adapter for Colima inspection and reclaim execution.

use crate::{colima_platform, colima_reclaim};
use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Manager};

fn colima_binary(home: &Path) -> PathBuf {
    [
        PathBuf::from("/opt/homebrew/bin/colima"),
        PathBuf::from("/usr/local/bin/colima"),
        home.join(".local/bin/colima"),
    ]
    .into_iter()
    .find(|path| {
        std::fs::symlink_metadata(path)
            .is_ok_and(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
    })
    .unwrap_or_else(|| PathBuf::from("colima"))
}

fn resolve_home(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .home_dir()
        .map_err(|_| "home-directory-unavailable".to_string())
}

fn cache_root(app: &AppHandle) -> Result<PathBuf, String> {
    let platform_cache = app
        .path()
        .cache_dir()
        .map_err(|_| "cache-directory-unavailable".to_string())?;
    colima_platform::configured_cache_root(&platform_cache)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

/// Runs filesystem scans and subprocess polling on Tauri's dedicated blocking executor.
///
/// Colima inspection and reclaim execution perform synchronous filesystem and child-process work.
/// Keeping that work off the async command executor prevents one bounded provider operation from
/// occupying an async runtime worker that also serves unrelated desktop commands.
async fn run_colima_blocking<T, F>(task: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(task)
        .await
        .map_err(|_| "colima-blocking-task-join-failed".to_string())?
}

/// Returns read-only Colima profile, VM-state, configured-disk, and cache-allocation evidence.
#[tauri::command(rename = "inspect_colima_reclaim")]
pub async fn inspect_colima_reclaim_configured(
    app: AppHandle,
) -> Result<colima_reclaim::ColimaReclaimPlan, String> {
    let home = resolve_home(&app)?;
    let cache_root = cache_root(&app)?;
    let binary = colima_binary(&home);
    run_colima_blocking(move || {
        Ok(colima_reclaim::plan_colima_reclaim(
            &binary,
            &cache_root,
            Duration::from_secs(10),
        ))
    })
    .await
}

/// Replans against the currently configured Colima cache root before invoking native cache prune.
#[tauri::command(rename = "execute_colima_cache_prune")]
pub async fn execute_colima_cache_prune_configured(
    confirmation_phrase: String,
    rationale: String,
    app: AppHandle,
) -> Result<colima_reclaim::ColimaCachePruneExecution, String> {
    let home = resolve_home(&app)?;
    let cache_root = cache_root(&app)?;
    let binary = colima_binary(&home);
    run_colima_blocking(move || {
        colima_reclaim::execute_colima_cache_prune(
            &binary,
            &cache_root,
            &confirmation_phrase,
            &rationale,
            now_ms(),
        )
    })
    .await
}

/// Returns exact dangling-image evidence without blocking Tauri's async command executor.
#[tauri::command(rename = "inspect_colima_dangling_images")]
pub async fn inspect_colima_dangling_images_configured(
    profile: String,
    app: AppHandle,
) -> Result<colima_reclaim::ColimaDanglingImagePlan, String> {
    let home = resolve_home(&app)?;
    let binary = colima_binary(&home);
    run_colima_blocking(move || {
        Ok(colima_reclaim::plan_colima_dangling_images(
            &binary,
            &profile,
            Duration::from_secs(10),
        ))
    })
    .await
}

/// Revalidates and removes only the approved dangling-image identities on a blocking worker.
#[tauri::command(rename = "execute_colima_dangling_images")]
pub async fn execute_colima_dangling_images_configured(
    profile: String,
    confirmation_phrase: String,
    rationale: String,
    app: AppHandle,
) -> Result<colima_reclaim::ColimaDanglingImageExecution, String> {
    let home = resolve_home(&app)?;
    let binary = colima_binary(&home);
    run_colima_blocking(move || {
        colima_reclaim::execute_colima_dangling_images(
            &binary,
            &profile,
            &confirmation_phrase,
            &rationale,
            now_ms(),
        )
    })
    .await
}

/// Returns dangling and privileged-empty-content volume evidence on a blocking worker.
#[tauri::command(rename = "inspect_colima_empty_volumes")]
pub async fn inspect_colima_empty_volumes_configured(
    profile: String,
    app: AppHandle,
) -> Result<colima_reclaim::ColimaEmptyVolumePlan, String> {
    let home = resolve_home(&app)?;
    let binary = colima_binary(&home);
    run_colima_blocking(move || {
        Ok(colima_reclaim::plan_colima_empty_volumes(
            &binary,
            &profile,
            Duration::from_secs(10),
        ))
    })
    .await
}

/// Revalidates and removes only approved empty-volume identities on a blocking worker.
#[tauri::command(rename = "execute_colima_empty_volumes")]
pub async fn execute_colima_empty_volumes_configured(
    profile: String,
    confirmation_phrase: String,
    rationale: String,
    app: AppHandle,
) -> Result<colima_reclaim::ColimaEmptyVolumeExecution, String> {
    let home = resolve_home(&app)?;
    let binary = colima_binary(&home);
    run_colima_blocking(move || {
        colima_reclaim::execute_colima_empty_volumes(
            &binary,
            &profile,
            &confirmation_phrase,
            &rationale,
            now_ms(),
        )
    })
    .await
}

/// Returns guest-TRIM eligibility and native host-compaction evidence on a blocking worker.
#[tauri::command(rename = "inspect_colima_guest_trim")]
pub async fn inspect_colima_guest_trim_configured(
    profile: String,
    app: AppHandle,
) -> Result<colima_reclaim::ColimaGuestTrimPlan, String> {
    let home = resolve_home(&app)?;
    let binary = colima_binary(&home);
    run_colima_blocking(move || {
        Ok(colima_reclaim::plan_colima_guest_trim(
            &binary,
            &profile,
            Duration::from_secs(10),
        ))
    })
    .await
}

/// Runs approved guest fstrim and records host evidence on a blocking worker.
#[tauri::command(rename = "execute_colima_guest_trim")]
pub async fn execute_colima_guest_trim_configured(
    profile: String,
    confirmation_phrase: String,
    rationale: String,
    app: AppHandle,
) -> Result<colima_reclaim::ColimaGuestTrimExecution, String> {
    let home = resolve_home(&app)?;
    let binary = colima_binary(&home);
    run_colima_blocking(move || {
        colima_reclaim::execute_colima_guest_trim(
            &binary,
            &profile,
            &confirmation_phrase,
            &rationale,
            now_ms(),
        )
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocking_colima_work_runs_off_calling_thread() {
        let caller = std::thread::current().id();
        let observed = tauri::async_runtime::block_on(run_colima_blocking(move || {
            Ok::<_, String>(std::thread::current().id())
        }))
        .expect("blocking worker should return its thread identity");

        assert_ne!(observed, caller);
    }
}
