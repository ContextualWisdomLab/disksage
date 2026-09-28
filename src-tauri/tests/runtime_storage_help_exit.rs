use std::process::Command;

const EXPECTED_USAGE: &str =
    "Usage: disksage-runtime-storage --runtime <colima|podman-machine> [--execute --confirm EXACT_PHRASE --rationale TEXT]";

#[test]
fn runtime_storage_help_exits_successfully_without_error_output() {
    for flag in ["--help", "-h"] {
        let output = Command::new(env!("CARGO_BIN_EXE_disksage-runtime-storage"))
            .arg(flag)
            .output()
            .expect("runtime-storage CLI must launch for help");

        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        assert_eq!(
            String::from_utf8(output.stdout).expect("help must be UTF-8"),
            format!("{EXPECTED_USAGE}\n")
        );
    }
}

#[test]
fn runtime_storage_mixed_help_is_a_bounded_failure() {
    for arguments in [
        &(["--help", "--runtime", "colima"][..]),
        &(["--runtime", "colima", "--help"][..]),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_disksage-runtime-storage"))
            .args(arguments)
            .output()
            .expect("runtime-storage CLI must launch for mixed-help validation");

        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        let stderr = String::from_utf8(output.stderr).expect("diagnostic must be UTF-8");
        assert!(stderr.contains("invalid argument"));
        assert!(stderr.contains(EXPECTED_USAGE));
    }
}

#[cfg(unix)]
#[test]
fn runtime_storage_non_utf8_argument_fails_without_reflection() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let opaque = OsString::from_vec(vec![b'-', b'-', b'o', b'p', b'a', b'q', 0xff]);
    let output = Command::new(env!("CARGO_BIN_EXE_disksage-runtime-storage"))
        .arg(opaque)
        .output()
        .expect("runtime-storage CLI must launch for non-UTF-8 validation");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("diagnostic must be UTF-8");
    assert_eq!(stderr.trim_end(), "invalid argument encoding");
    assert!(!stderr.contains("\u{fffd}"));
}
