use std::process::Command;

#[test]
fn apply_fails_closed_before_repository_access_until_identity_bound_trash_exists() {
    let output = Command::new(env!("CARGO_BIN_EXE_disksage-stale-git-clone"))
        .args([
            "--repository-root",
            "/definitely-not-a-real-disksage-repository",
            "--apply",
            "--plan-fingerprint",
            "reviewed-plan",
            "--confirmation-phrase",
            "reviewed-confirmation",
            "--rationale",
            "reviewed by operator",
        ])
        .output()
        .expect("stale clone CLI should launch");

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).expect("stderr is UTF-8"),
        "disksage-stale-git-clone: stale-git-clone-removal-identity-bound-trash-unavailable\n"
    );
}
