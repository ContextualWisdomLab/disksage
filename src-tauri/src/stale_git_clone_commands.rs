use crate::stale_git_clone::StaleGitCloneRemoval;

const REMOVAL_UNAVAILABLE: &str =
    "stale-git-clone-removal-identity-bound-trash-unavailable";

/// Keep destructive stale-clone removal unavailable before any caller-controlled path,
/// approval, or rationale is interpreted. Read-only planning remains available separately.
#[tauri::command]
pub fn remove_stale_git_clone(
    repository_root: String,
    open_age_days: u64,
    approved_plan_fingerprint: String,
    confirmation_exact_approval_phrase: String,
    rationale: String,
) -> Result<StaleGitCloneRemoval, String> {
    let _ = (
        repository_root,
        open_age_days,
        approved_plan_fingerprint,
        confirmation_exact_approval_phrase,
        rationale,
    );
    Err(REMOVAL_UNAVAILABLE.into())
}
