use std::process::Command;

#[test]
fn execute_is_unavailable_until_private_identity_bound_approval_exists() {
    let output = Command::new(env!("CARGO_BIN_EXE_disksage-temp-reclaim"))
        .args(["--execute", "reviewed-plan", "reviewed-phrase", "reviewed"])
        .output()
        .expect("temporary reclaim CLI should launch");

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).expect("stderr is UTF-8"),
        "temp-reclaim-removal-private-approval-unavailable\n"
    );
}
