//! Public Colima reclaim contract that binds mutation evidence to Colima's configured cache root.

use std::{path::{Path, PathBuf}, time::Duration};

pub use crate::colima_reclaim_impl::{
    execute_colima_dangling_images, execute_colima_empty_volumes, execute_colima_guest_trim,
    plan_colima_dangling_images, plan_colima_empty_volumes, plan_colima_guest_trim,
    ColimaCachePruneExecution, ColimaDanglingImageExecution, ColimaDanglingImagePlan,
    ColimaEmptyVolumeExecution, ColimaEmptyVolumePlan, ColimaGuestTrimExecution,
    ColimaGuestTrimPlan, ColimaProfileEvidence, ColimaReclaimPlan,
};

fn explicitly_configured_cache_root() -> Result<Option<PathBuf>, String> {
    if let Some(value) = std::env::var_os("COLIMA_CACHE_HOME").filter(|value| !value.is_empty()) {
        let path = PathBuf::from(value);
        if !path.is_absolute() {
            return Err("colima-cache-home-relative-unsupported".into());
        }
        return Ok(Some(path));
    }

    if let Some(value) = std::env::var_os("XDG_CACHE_HOME").filter(|value| !value.is_empty()) {
        let path = PathBuf::from(value);
        if !path.is_absolute() {
            return Err("colima-cache-home-relative-unsupported".into());
        }
        return Ok(Some(path.join("colima")));
    }

    Ok(None)
}

fn cache_root_contract_issue(cache_root: &Path) -> Option<String> {
    match explicitly_configured_cache_root() {
        Err(issue) => Some(issue),
        Ok(Some(configured_root)) if configured_root != cache_root => {
            Some("colima-cache-root-mismatch".into())
        }
        Ok(_) => None,
    }
}

/// Builds Colima reclaim evidence only when the measured cache root agrees with Colima's
/// explicit cache-home configuration. A relative or mismatched configured root fails closed so
/// a reviewed fingerprint cannot authorize pruning a different cache tree.
pub fn plan_colima_reclaim(
    executable: &Path,
    cache_root: &Path,
    timeout: Duration,
) -> ColimaReclaimPlan {
    let mut plan = crate::colima_reclaim_impl::plan_colima_reclaim(executable, cache_root, timeout);
    if let Some(issue) = cache_root_contract_issue(cache_root) {
        if !plan.issues.iter().any(|existing| existing == &issue) {
            plan.issues.push(issue);
        }
        plan.evidence_complete = false;
        plan.plan_fingerprint = None;
        plan.cache_prune_approval_phrase = None;
    }
    plan
}

/// Executes a cache prune only after the public contract has revalidated the currently configured
/// Colima cache root. The implementation then performs its own fresh plan/fingerprint check before
/// invoking Colima, preserving the existing exact-approval and post-execution evidence boundary.
pub fn execute_colima_cache_prune(
    executable: &Path,
    cache_root: &Path,
    confirmation_phrase: &str,
    rationale: &str,
    executed_at_ms: u64,
) -> Result<ColimaCachePruneExecution, String> {
    let public_plan = plan_colima_reclaim(executable, cache_root, Duration::from_secs(10));
    if !public_plan.evidence_complete {
        return Err("colima-cache-prune-evidence-incomplete".into());
    }
    crate::colima_reclaim_impl::execute_colima_cache_prune(
        executable,
        cache_root,
        confirmation_phrase,
        rationale,
        executed_at_ms,
    )
}
