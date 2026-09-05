//! Shipped provider-cache cleanup admission boundary.
//!
//! Reversible Trash cleanup may delegate to the existing evidence-bound executor. Permanent purge
//! remains unavailable until the canonical deletion-safety owner provides object-bound staging and
//! deletion authority with recovery evidence across supported platforms.

use crate::provider_cache_reclaim::{
    ProviderCacheCleanupMode, ProviderCacheCleanupRequest, ProviderCacheCleanupResult,
    ProviderCacheReclaimPlan,
};

const PERMANENT_PURGE_UNAVAILABLE: &str =
    "provider-cache-identity-bound-permanent-delete-unavailable";

/// Inspect provider-cache candidates without advertising an unavailable irreversible approval.
///
/// The lower-level historical planner still carries a permanent-approval field for compatibility.
/// The shipped product surface deliberately clears only that authority. Read-only evidence issues
/// and Trash approval remain unchanged so an unavailable irreversible mode cannot make reversible
/// evidence appear incomplete.
#[cfg(not(coverage))]
#[tauri::command(async)]
pub fn plan_provider_cache_reclaim() -> Result<ProviderCacheReclaimPlan, String> {
    let mut plan = crate::commands::plan_provider_cache_reclaim()?;
    plan.exact_approval_phrase = None;
    Ok(plan)
}

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
        return Err(PERMANENT_PURGE_UNAVAILABLE.into());
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
