use disksage_lib::stale_git_clone::remove_stale_git_clone;
use std::path::Path;

#[test]
fn library_removal_fails_closed_before_repository_observation() {
    let result = remove_stale_git_clone(
        Path::new("/definitely-not-a-real-disksage-repository"),
        30,
        "reviewed-plan",
        "reviewed-confirmation",
        "reviewed by operator",
        1,
    );

    assert_eq!(
        result,
        Err("stale-git-clone-removal-identity-bound-trash-unavailable".to_string())
    );
}
