//! Shipped provider-cache cleanup admission boundary.
//!
//! Reversible Trash cleanup may delegate to the existing evidence-bound executor. Permanent purge
//! remains unavailable until the canonical deletion-safety owner provides object-bound staging and
//! deletion authority with recovery evidence across supported platforms.

use crate::provider_cache_reclaim::{
    ProviderCacheCleanupMode, ProviderCacheCleanupRequest, ProviderCacheCleanupResult,
};

/// Execute provider-cache cleanup only through the currently commercial-safe reversible mode.
///
/// Permanent purge fails before receipt creation or any lower-level mutation. Keeping this gate at
/// the shipped Tauri boundary prevents a historical pathname-authorized irreversible path from
/// becoming product authority while its useful read-only planning and Trash evidence remain usable.
#[cfg(not(coverage))]
#[tauri::command(async)]
pub fn execute_provider_cache_reclaim(
    app: tauri::AppHandle,
    requests: Vec<ProviderCacheCleanupRequest>,
    approved_plan_fingerprint: String,
    confirm_plan_fingerprint: String,
    confirmation_phrase: String,
    rationale: String,
    mode: ProviderCacheCleanupMode,
) -> Result<ProviderCacheCleanupResult, String> {
    if mode == ProviderCacheCleanupMode::PermanentPurge {
        return Err("provider-cache-identity-bound-permanent-delete-unavailable".into());
    }

    crate::commands::execute_provider_cache_reclaim(
        app,
        requests,
        approved_plan_fingerprint,
        confirm_plan_fingerprint,
        confirmation_phrase,
        rationale,
        mode,
    )
}
