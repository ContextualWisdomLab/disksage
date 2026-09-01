//! Public Colima reclaim contract that binds mutation evidence to Colima's configured cache root.

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

pub use crate::colima_reclaim_impl::{
    execute_colima_dangling_images, execute_colima_guest_trim, plan_colima_dangling_images,
    plan_colima_guest_trim, ColimaCachePruneExecution, ColimaDanglingImageExecution,
    ColimaDanglingImagePlan, ColimaEmptyVolumeExecution, ColimaEmptyVolumePlan,
    ColimaGuestTrimExecution, ColimaGuestTrimPlan, ColimaProfileEvidence, ColimaReclaimPlan,
};

const COLIMA_EMPTY_VOLUME_ATOMIC_REMOVAL_UNAVAILABLE: &str =
    "colima-empty-volume-atomic-removal-unavailable";

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

fn resolved_colima_executable(executable: &Path) -> PathBuf {
    if executable.is_absolute() || executable.components().count() != 1 {
        return executable.to_path_buf();
    }

    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .filter(|directory| directory.is_absolute())
        .map(|directory| directory.join(executable))
        .find(|candidate| {
            std::fs::symlink_metadata(candidate).is_ok_and(|metadata| {
                metadata.is_file() && !metadata.file_type().is_symlink()
            })
        })
        .unwrap_or_else(|| executable.to_path_buf())
}

/// Builds Colima reclaim evidence only when the measured cache root agrees with Colima's
/// explicit cache-home configuration. A relative or mismatched configured root fails closed so
/// a reviewed fingerprint cannot authorize pruning a different cache tree. Bare executable names
/// are resolved through absolute PATH entries before identity validation, while symlinked or
/// non-regular candidates remain rejected by the implementation boundary.
pub fn plan_colima_reclaim(
    executable: &Path,
    cache_root: &Path,
    timeout: Duration,
) -> ColimaReclaimPlan {
    let executable = resolved_colima_executable(executable);
    let mut plan =
        crate::colima_reclaim_impl::plan_colima_reclaim(&executable, cache_root, timeout);
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
/// Colima cache root and resolved a PATH-installed executable to the same identity used for the
/// fresh plan. The implementation then performs its own fingerprint check before mutation.
pub fn execute_colima_cache_prune(
    executable: &Path,
    cache_root: &Path,
    confirmation_phrase: &str,
    rationale: &str,
    executed_at_ms: u64,
) -> Result<ColimaCachePruneExecution, String> {
    let executable = resolved_colima_executable(executable);
    let public_plan = plan_colima_reclaim(&executable, cache_root, Duration::from_secs(10));
    if !public_plan.evidence_complete {
        return Err("colima-cache-prune-evidence-incomplete".into());
    }
    crate::colima_reclaim_impl::execute_colima_cache_prune(
        &executable,
        cache_root,
        confirmation_phrase,
        rationale,
        executed_at_ms,
    )
}

/// Inspects dangling Colima volumes while withholding check-then-remove deletion authority.
///
/// The implementation can prove that a volume was empty, but a separate later `docker volume rm`
/// cannot atomically bind that observation to the mutation. The public contract therefore keeps
/// the read-only candidate evidence and suppresses approval until an atomic provider boundary is
/// available.
pub fn plan_colima_empty_volumes(
    executable: &Path,
    profile: &str,
    timeout: Duration,
) -> ColimaEmptyVolumePlan {
    let executable = resolved_colima_executable(executable);
    let mut plan =
        crate::colima_reclaim_impl::plan_colima_empty_volumes(&executable, profile, timeout);
    plan.exact_approval_phrase = None;
    if plan.empty_candidate_count > 0
        && !plan
            .issues
            .iter()
            .any(|issue| issue == COLIMA_EMPTY_VOLUME_ATOMIC_REMOVAL_UNAVAILABLE)
    {
        plan.issues
            .push(COLIMA_EMPTY_VOLUME_ATOMIC_REMOVAL_UNAVAILABLE.into());
    }
    plan
}

/// Refuses Colima empty-volume deletion before invoking the provider until emptiness and removal
/// can be bound to one atomic operation. This prevents data written after a successful re-scan
/// from being removed under an older approval.
pub fn execute_colima_empty_volumes(
    _executable: &Path,
    _profile: &str,
    _confirmation_phrase: &str,
    _rationale: &str,
    _executed_at_ms: u64,
) -> Result<ColimaEmptyVolumeExecution, String> {
    Err(COLIMA_EMPTY_VOLUME_ATOMIC_REMOVAL_UNAVAILABLE.into())
}
