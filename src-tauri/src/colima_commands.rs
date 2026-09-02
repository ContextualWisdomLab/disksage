//! Tauri adapter for Colima host-cache inspection and exact cache-prune execution.

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

/// Returns read-only Colima profile, VM-state, configured-disk, and cache-allocation evidence.
#[tauri::command(rename = "inspect_colima_reclaim", async)]
pub fn inspect_colima_reclaim_configured(
    app: AppHandle,
) -> Result<colima_reclaim::ColimaReclaimPlan, String> {
    let home = resolve_home(&app)?;
    let cache_root = cache_root(&app)?;
    Ok(colima_reclaim::plan_colima_reclaim(
        &colima_binary(&home),
        &cache_root,
        Duration::from_secs(10),
    ))
}

/// Replans against the currently configured Colima cache root before invoking native cache prune.
#[tauri::command(rename = "execute_colima_cache_prune", async)]
pub fn execute_colima_cache_prune_configured(
    confirmation_phrase: String,
    rationale: String,
    app: AppHandle,
) -> Result<colima_reclaim::ColimaCachePruneExecution, String> {
    let home = resolve_home(&app)?;
    let cache_root = cache_root(&app)?;
    colima_reclaim::execute_colima_cache_prune(
        &colima_binary(&home),
        &cache_root,
        &confirmation_phrase,
        &rationale,
        now_ms(),
    )
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
