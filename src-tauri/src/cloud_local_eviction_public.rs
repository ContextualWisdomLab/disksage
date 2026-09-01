//! Public iCloud-local-eviction boundary.
//!
//! The implementation module contains provider-neutral observation helpers needed by the iCloud
//! workflow. This facade keeps the exported mutation contract provider-specific and fail-closed.

pub use crate::cloud_local_eviction_impl::*;

use crate::cloud::{CloudProvider, CloudRoot};
use std::path::Path;

fn require_icloud_provider(root: &CloudRoot) -> Result<(), String> {
    if root.provider != CloudProvider::Icloud {
        return Err("icloud-local-eviction-provider-mismatch".into());
    }
    Ok(())
}

/// Build a read-only local-copy eviction plan only for an iCloud-owned root.
#[cfg(not(coverage))]
pub fn plan_icloud_local_eviction(
    root: &CloudRoot,
    path: &Path,
    observed_at_ms: u64,
) -> Result<IcloudLocalEvictionPlan, String> {
    require_icloud_provider(root)?;
    crate::cloud_local_eviction_impl::plan_icloud_local_eviction(root, path, observed_at_ms)
}

/// Bind approval only to a provider-correct iCloud plan.
pub fn approve_icloud_local_eviction(
    plan: &IcloudLocalEvictionPlan,
    approved_plan_fingerprint: &str,
    approved_at_ms: u64,
    approved_by: &str,
    rationale: &str,
) -> Result<IcloudLocalEvictionApproval, String> {
    if plan.provider != CloudProvider::Icloud {
        return Err("icloud-local-eviction-provider-mismatch".into());
    }
    crate::cloud_local_eviction_impl::approve_icloud_local_eviction(
        plan,
        approved_plan_fingerprint,
        approved_at_ms,
        approved_by,
        rationale,
    )
}

/// Execute only when both the live root and the reviewed plan are explicitly iCloud-owned.
#[cfg(not(coverage))]
pub fn execute_icloud_local_eviction(
    root: &CloudRoot,
    approved_plan: &IcloudLocalEvictionPlan,
    approval: &IcloudLocalEvictionApproval,
    confirmation_plan_fingerprint: &str,
    requested_at_ms: u64,
) -> Result<IcloudLocalEvictionResult, String> {
    require_icloud_provider(root)?;
    if approved_plan.provider != CloudProvider::Icloud {
        return Err("icloud-local-eviction-provider-mismatch".into());
    }
    crate::cloud_local_eviction_impl::execute_icloud_local_eviction(
        root,
        approved_plan,
        approval,
        confirmation_plan_fingerprint,
        requested_at_ms,
    )
}
