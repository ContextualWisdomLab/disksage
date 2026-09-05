//! Commercial provider-cache reclaim facade.
//!
//! Planning remains read-only and reversible Trash is the only mutation lifecycle exposed to Rust
//! callers outside the application crate. Historical irreversible approval and execution stay
//! crate-private repair evidence until deletion authority is object-bound and recovery-complete on
//! every supported platform.

use serde::Serialize;
use std::path::Path;

use crate::provider_cache_reclaim::{
    ProviderCacheCleanupMode as InternalProviderCacheCleanupMode,
    ProviderCacheCleanupResult as InternalProviderCacheCleanupResult,
    ProviderCacheReclaimPlan as InternalProviderCacheReclaimPlan,
};
pub use crate::provider_cache_reclaim::{
    ProviderCacheCandidate, ProviderCacheCleanupItemResult, ProviderCacheCleanupRequest,
    ProviderCacheKind,
};

/// Commercially exposed provider-cache reclaim plan.
///
/// The historical lower-level plan contains `exact_approval_phrase` for an irreversible lifecycle
/// that DiskSage does not ship. This DTO deliberately omits that field so callers cannot infer or
/// persist unsupported permanent-deletion authority from the public Rust schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProviderCacheReclaimPlan {
    pub schema_version: u32,
    pub platform: String,
    pub observed_at_ms: u64,
    pub installed_edge_version: Option<String>,
    pub podman_machine_present: bool,
    pub podman_recreation_source: Option<String>,
    pub evidence_complete: bool,
    pub candidates: Vec<ProviderCacheCandidate>,
    pub issues: Vec<String>,
    pub plan_fingerprint: String,
    pub trash_approval_phrase: Option<String>,
}

/// Commercially exposed provider-cache cleanup lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderCacheCleanupMode {
    Trash,
}

/// Trash-only provider-cache cleanup result exposed outside the application crate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProviderCacheCleanupResult {
    pub plan_fingerprint: String,
    pub requested_count: usize,
    pub completed_count: usize,
    pub executed_at_ms: u64,
    pub rationale: String,
    pub mode: ProviderCacheCleanupMode,
    pub immutable_receipt_path: String,
    pub items: Vec<ProviderCacheCleanupItemResult>,
}

pub(crate) fn project_plan(plan: InternalProviderCacheReclaimPlan) -> ProviderCacheReclaimPlan {
    ProviderCacheReclaimPlan {
        schema_version: plan.schema_version,
        platform: plan.platform,
        observed_at_ms: plan.observed_at_ms,
        installed_edge_version: plan.installed_edge_version,
        podman_machine_present: plan.podman_machine_present,
        podman_recreation_source: plan.podman_recreation_source,
        evidence_complete: plan.evidence_complete,
        candidates: plan.candidates,
        issues: plan.issues,
        plan_fingerprint: plan.plan_fingerprint,
        trash_approval_phrase: plan.trash_approval_phrase,
    }
}

pub(crate) fn project_trash_result(
    result: InternalProviderCacheCleanupResult,
) -> Result<ProviderCacheCleanupResult, String> {
    if result.mode != InternalProviderCacheCleanupMode::Trash {
        return Err("provider-cache-trash-result-mode-mismatch".into());
    }
    Ok(ProviderCacheCleanupResult {
        plan_fingerprint: result.plan_fingerprint,
        requested_count: result.requested_count,
        completed_count: result.completed_count,
        executed_at_ms: result.executed_at_ms,
        rationale: result.rationale,
        mode: ProviderCacheCleanupMode::Trash,
        immutable_receipt_path: result.immutable_receipt_path,
        items: result.items,
    })
}

/// Inspect exact provider-cache candidates without publishing irreversible approval authority.
pub fn plan_with_runtime(
    home: &Path,
    applications: &Path,
    podman_bin: &Path,
    observed_at_ms: u64,
) -> ProviderCacheReclaimPlan {
    project_plan(crate::provider_cache_reclaim::plan_with_runtime(
        home,
        applications,
        podman_bin,
        observed_at_ms,
    ))
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
    let result = crate::provider_cache_reclaim::execute(
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
        InternalProviderCacheCleanupMode::Trash,
        executed_at_ms,
    )?;
    project_trash_result(result)
}
