//! Shipped provider-cache cleanup admission boundary.
//!
//! Reversible Trash cleanup may delegate to the existing evidence-bound executor. Permanent purge
//! remains unavailable until the canonical deletion-safety owner provides object-bound staging and
//! deletion authority with recovery evidence across supported platforms.

use crate::provider_cache::{
    ProviderCacheCleanupRequest, ProviderCacheCleanupResult, ProviderCacheReclaimPlan,
};
use crate::provider_cache_reclaim::ProviderCacheCleanupMode as InternalProviderCacheCleanupMode;

/// Inspect provider-cache candidates without naming unavailable irreversible approval authority.
///
/// The lower-level historical planner still carries permanent-approval repair evidence. The shipped
/// command projects that internal report into the commercial Trash-only plan DTO rather than
/// serializing an irreversible approval field with a null value.
#[cfg(not(coverage))]
#[tauri::command(async)]
pub fn plan_provider_cache_reclaim() -> Result<ProviderCacheReclaimPlan, String> {
    crate::commands::plan_provider_cache_reclaim().map(crate::provider_cache::project_plan)
}

/// Execute provider-cache cleanup only through the currently commercial-safe reversible mode.
///
/// The shipped Tauri command intentionally has no caller-selected cleanup-mode argument. This keeps
/// irreversible authority out of the invoke schema rather than merely rejecting one enum value at
/// runtime. The historical lower-level executor remains repair evidence while every product call is
/// delegated as Trash and projected back through the public Trash-only result DTO.
#[cfg(not(coverage))]
#[tauri::command(async)]
pub fn execute_provider_cache_reclaim(
    app: tauri::AppHandle,
    requests: Vec<ProviderCacheCleanupRequest>,
    approved_plan_fingerprint: String,
    confirm_plan_fingerprint: String,
    confirmation_phrase: String,
    rationale: String,
) -> Result<ProviderCacheCleanupResult, String> {
    let result = crate::commands::execute_provider_cache_reclaim(
        app,
        requests,
        approved_plan_fingerprint,
        confirm_plan_fingerprint,
        confirmation_phrase,
        rationale,
        InternalProviderCacheCleanupMode::Trash,
    )?;
    crate::provider_cache::project_trash_result(result)
}
