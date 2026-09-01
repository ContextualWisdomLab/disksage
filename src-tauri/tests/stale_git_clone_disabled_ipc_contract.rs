use disksage_lib::stale_git_clone_commands::remove_stale_git_clone;

const REMOVAL_UNAVAILABLE: &str =
    "stale-git-clone-removal-identity-bound-trash-unavailable";

#[test]
fn disabled_ipc_fails_before_payload_validation() {
    let error = remove_stale_git_clone(
        "relative-path".into(),
        0,
        String::new(),
        String::new(),
        " invalid\n".into(),
    )
    .expect_err("disabled stale-clone removal must fail closed");

    assert_eq!(error, REMOVAL_UNAVAILABLE);
}

#[test]
fn tauri_registration_uses_fail_closed_adapter() {
    let lib_source = include_str!("../src/lib.rs");

    assert!(lib_source.contains("stale_git_clone_commands::remove_stale_git_clone"));
    assert!(!lib_source.contains("commands::remove_stale_git_clone,"));
}
