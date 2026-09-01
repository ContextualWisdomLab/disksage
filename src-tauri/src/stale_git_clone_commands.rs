use crate::stale_git_clone::StaleGitCloneRemoval;

/// Destructive stale-clone removal is intentionally unavailable at the desktop IPC boundary
/// until the reviewed clone is moved through an identity-bound, reversible Trash operation and
/// the requested path is constrained to an authorized scan root. Planning remains available.
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
    Err("stale-git-clone-removal-identity-bound-trash-unavailable".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_removal_fails_before_any_path_or_approval_is_consumed() {
        assert_eq!(
            remove_stale_git_clone(
                "/tmp/arbitrary".into(),
                30,
                "fingerprint".into(),
                "phrase".into(),
                "reviewed".into(),
            ),
            Err("stale-git-clone-removal-identity-bound-trash-unavailable".into())
        );
    }
}
