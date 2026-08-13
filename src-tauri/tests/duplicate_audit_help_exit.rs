#![cfg(feature = "cloud-cli")]

use std::process::Command;

#[test]
fn duplicate_audit_help_exits_successfully_without_error_output() {
    let output = Command::new(env!("CARGO_BIN_EXE_disksage-duplicate-audit"))
        .arg("--help")
        .output()
        .expect("duplicate-audit CLI must launch for its help contract");

    assert!(
        output.status.success(),
        "--help must be a successful terminal action, got status {:?} and stderr {:?}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "successful help must not be projected through stderr"
    );
    let stdout = String::from_utf8(output.stdout).expect("help output must be valid UTF-8");
    assert!(stdout.contains("usage: disksage-duplicate-audit --root ABSOLUTE_PATH"));
    assert!(stdout.contains("--private-output ABSOLUTE_NEW_FILE.json"));
}
