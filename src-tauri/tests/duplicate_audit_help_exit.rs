#![cfg(feature = "cloud-cli")]

use std::process::Command;

#[test]
fn duplicate_audit_help_exits_successfully_without_error_output() {
    for flag in ["--help", "-h"] {
        let output = Command::new(env!("CARGO_BIN_EXE_disksage-duplicate-audit"))
            .arg(flag)
            .output()
            .expect("duplicate-audit CLI must launch for its help contract");

        assert!(
            output.status.success(),
            "{flag} must be a successful terminal action, got status {:?} and stderr {:?}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            output.stderr.is_empty(),
            "successful {flag} help must not be projected through stderr"
        );
        let stdout = String::from_utf8(output.stdout).expect("help output must be valid UTF-8");
        assert!(stdout.contains("usage: disksage-duplicate-audit --root ABSOLUTE_PATH"));
        assert!(stdout.contains("--private-output ABSOLUTE_NEW_FILE.json"));
    }
}

#[test]
fn duplicate_audit_help_does_not_hide_an_unknown_argument() {
    let output = Command::new(env!("CARGO_BIN_EXE_disksage-duplicate-audit"))
        .args(["--help", "--unknown"])
        .output()
        .expect("duplicate-audit CLI must launch for invalid-argument validation");

    assert!(
        !output.status.success(),
        "help must not turn an otherwise invalid invocation into success"
    );
    assert!(output.stdout.is_empty(), "invalid invocation must not emit help on stdout");
    assert!(
        !output.stderr.is_empty(),
        "invalid invocation must remain visible through stderr"
    );
}

#[cfg(unix)]
#[test]
fn duplicate_audit_non_utf8_argument_fails_without_panic() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let opaque = OsString::from_vec(vec![b'-', b'-', b'o', b'p', b'a', b'q', b'u', b'e', 0xff]);
    let output = Command::new(env!("CARGO_BIN_EXE_disksage-duplicate-audit"))
        .arg(opaque)
        .output()
        .expect("duplicate-audit CLI must launch for non-UTF-8 argument validation");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("CLI diagnostics must be valid UTF-8");
    assert_eq!(
        stderr.trim_end(),
        "DiskSage exact duplicate audit: duplicate-audit-argument-invalid"
    );
}
