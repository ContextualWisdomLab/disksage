//! Commercial provider-cache reclaim facade.
//!
//! Planning remains read-only and reversible Trash is the only mutation lifecycle exposed to Rust
//! callers outside the application crate. The historical irreversible executor stays crate-private
//! repair evidence until deletion authority is object-bound and recovery-complete on every supported
//! platform.

use std::path::Path;

use crate::provider_cache_reclaim::ProviderCacheCleanupMode;
pub use crate::provider_cache_reclaim::{
    ProviderCacheCandidate, ProviderCacheCleanupItemResult, ProviderCacheCleanupRequest,
    ProviderCacheCleanupResult, ProviderCacheKind, ProviderCacheReclaimPlan,
};

/// Inspect exact provider-cache candidates without publishing irreversible approval authority.
pub fn plan_with_runtime(
    home: &Path,
    applications: &Path,
    podman_bin: &Path,
    observed_at_ms: u64,
) -> ProviderCacheReclaimPlan {
    let mut plan = crate::provider_cache_reclaim::plan_with_runtime(
        home,
        applications,
        podman_bin,
        observed_at_ms,
    );
    plan.exact_approval_phrase = None;
    plan
}

/// Re-plan and execute only reversible Trash cleanup for the explicitly approved candidate set.
#[allow(clippy::too_many_arguments)]
pub fn execute_trash(
    home: &Path,
    applications: &Path,
    podman_bin: &Path,
    requested: &[ProviderCacheCleanupRequest],
    approved_plan_fingerprint: &str,
    confirm_plan_fingerprint: &str,
    confirmation_phrase: &str,
    rationale: &str,
    journal_path: &Path,
    receipt_dir: &Path,
    executed_at_ms: u64,
) -> Result<ProviderCacheCleanupResult, String> {
    crate::provider_cache_reclaim::execute(
        home,
        applications,
        podman_bin,
        requested,
        approved_plan_fingerprint,
        confirm_plan_fingerprint,
        confirmation_phrase,
        rationale,
        journal_path,
        receipt_dir,
        ProviderCacheCleanupMode::Trash,
        executed_at_ms,
    )
}
