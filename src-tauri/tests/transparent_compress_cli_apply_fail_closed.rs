use std::process::Command;

#[test]
fn apply_is_unavailable_until_compression_root_authorization_exists() {
    let output = Command::new(env!("CARGO_BIN_EXE_disksage-transparent-compress"))
        .args([
            "--root",
            "/definitely-not-an-authorized-disksage-root",
            "--apply",
            "--plan-fingerprint",
            "reviewed-plan",
            "--confirmation-phrase",
            "reviewed-phrase",
            "--rationale",
            "reviewed",
        ])
        .output()
        .expect("transparent compression CLI should launch");

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).expect("stderr is UTF-8"),
        "disksage-transparent-compress: transparent-compression-root-authorization-unavailable\n"
    );
}
