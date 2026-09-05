//! Public iCloud batch-eviction boundary.
//!
//! Batch approval and execution are iCloud-only because the implementation ultimately uses
//! iCloud-specific eviction and post-eviction verification. Provider-neutral cloud roots must not
//! acquire that authority merely by matching a serialized plan.

use crate::cloud::{CloudProvider, CloudRoot};
use std::path::{Path, PathBuf};

pub use crate::cloud_local_eviction_batch_impl::{
    IcloudLocalEvictionBatchApproval, IcloudLocalEvictionBatchItem,
    IcloudLocalEvictionBatchItemOutcome, IcloudLocalEvictionBatchPlan,
    IcloudLocalEvictionBatchResult, IcloudLocalEvictionBatchUnavailable,
    ICLOUD_LOCAL_EVICTION_BATCH_VERSION, MAX_BATCH_ITEMS,
};

fn require_icloud_provider(root: &CloudRoot) -> Result<(), String> {
    if root.provider != CloudProvider::Icloud {
        return Err("icloud-local-eviction-batch-provider-mismatch".into());
    }
    Ok(())
}

/// Build a read-only batch plan only for an iCloud-owned root.
pub fn plan_icloud_local_eviction_batch(
    root: &CloudRoot,
    paths: &[PathBuf],
    observed_at_ms: u64,
) -> Result<IcloudLocalEvictionBatchPlan, String> {
    require_icloud_provider(root)?;
    crate::cloud_local_eviction_batch_impl::plan_icloud_local_eviction_batch(
        root,
        paths,
        observed_at_ms,
    )
}

/// Bind attributed human approval only to an iCloud-owned batch plan.
pub fn approve_icloud_local_eviction_batch(
    plan: &IcloudLocalEvictionBatchPlan,
    root: &CloudRoot,
    approved_batch_fingerprint: &str,
    approved_at_ms: u64,
    approved_by: &str,
    rationale: &str,
) -> Result<IcloudLocalEvictionBatchApproval, String> {
    require_icloud_provider(root)?;
    if plan.provider != CloudProvider::Icloud {
        return Err("icloud-local-eviction-batch-provider-mismatch".into());
    }
    crate::cloud_local_eviction_batch_impl::approve_icloud_local_eviction_batch(
        plan,
        root,
        approved_batch_fingerprint,
        approved_at_ms,
        approved_by,
        rationale,
    )
}

/// Execute only when the live root and approved batch are both explicitly iCloud-owned.
pub fn execute_icloud_local_eviction_batch(
    root: &CloudRoot,
    plan: &IcloudLocalEvictionBatchPlan,
    approval: &IcloudLocalEvictionBatchApproval,
    confirmation_batch_fingerprint: &str,
    record_dir: &Path,
    requested_at_ms: u64,
) -> Result<IcloudLocalEvictionBatchResult, String> {
    require_icloud_provider(root)?;
    if plan.provider != CloudProvider::Icloud {
        return Err("icloud-local-eviction-batch-provider-mismatch".into());
    }
    crate::cloud_local_eviction_batch_impl::execute_icloud_local_eviction_batch(
        root,
        plan,
        approval,
        confirmation_batch_fingerprint,
        record_dir,
        requested_at_ms,
    )
}
