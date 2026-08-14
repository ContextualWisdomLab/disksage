#![cfg(feature = "cloud-cli")]

use std::process::{Command, Output};

const EXPECTED_USAGE: &str = "usage: disksage-duplicate-audit --root ABSOLUTE_PATH [--min-bytes POSITIVE_INTEGER] [--max-entries 1..=1000000] [--private-output ABSOLUTE_NEW_FILE.json]";

fn assert_invalid_argument_is_bounded(output: Output) {
    assert!(
        !output.status.success(),
        "an invalid invocation must remain a non-zero failure"
    );
    assert!(
        output.stdout.is_empty(),
        "invalid invocation must not emit successful output on stdout"
    );
    let stderr = String::from_utf8(output.stderr).expect("CLI diagnostics must be valid UTF-8");
    assert!(!stderr.is_empty(), "invalid invocation must remain visible");
    assert!(
        !stderr.contains("not-shown"),
        "invalid diagnostics must not echo arbitrary argument payloads"
    );
}

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
        assert_eq!(
            stdout,
            format!("{EXPECTED_USAGE}\n"),
            "help output must equal the stable usage synopsis"
        );
    }
}

#[test]
fn duplicate_audit_unknown_and_mixed_arguments_are_bounded() {
    for arguments in [
        vec!["--opaque-option=not-shown"],
        vec!["--help", "--opaque-option=not-shown"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_disksage-duplicate-audit"))
            .args(arguments)
            .output()
            .expect("duplicate-audit CLI must launch for invalid-argument validation");
        assert_invalid_argument_is_bounded(output);
    }
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
