#[test]
fn public_stale_clone_removal_is_fail_closed_until_identity_bound_trash_exists() {
    let result = disksage_lib::stale_git_clone_commands::remove_stale_git_clone(
        "/tmp/customer-repository".into(),
        30,
        "reviewed-plan".into(),
        "reviewed-confirmation".into(),
        "reviewed by operator".into(),
    );

    assert_eq!(
        result,
        Err("stale-git-clone-removal-identity-bound-trash-unavailable".to_string())
    );
}

#[test]
fn tauri_handler_routes_stale_clone_removal_through_fail_closed_boundary() {
    let lib_source = include_str!("../src/lib.rs");
    assert!(lib_source.contains("stale_git_clone_commands::remove_stale_git_clone"));
    assert!(!lib_source.contains("commands::remove_stale_git_clone,"));
}
