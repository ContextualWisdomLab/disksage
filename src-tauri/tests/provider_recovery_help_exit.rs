use std::process::Command;

const EXPECTED_USAGE: &str =
    "usage: disksage-provider-recovery --provider onedrive|google-drive [--allow-graceful-term] [--output ABSOLUTE_NEW_FILE.json]";

#[test]
fn provider_recovery_help_exits_successfully_without_error_output() {
    for flag in ["--help", "-h"] {
        let output = Command::new(env!("CARGO_BIN_EXE_disksage-provider-recovery"))
            .arg(flag)
            .output()
            .expect("provider recovery CLI must launch for help");

        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        assert_eq!(
            String::from_utf8(output.stdout).expect("help must be UTF-8"),
            format!("{EXPECTED_USAGE}\n")
        );
    }
}

#[test]
fn provider_recovery_mixed_help_is_a_bounded_failure() {
    for arguments in [
        &(["--help", "--provider", "onedrive"][..]),
        &(["--provider", "onedrive", "--help"][..]),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_disksage-provider-recovery"))
            .args(arguments)
            .output()
            .expect("provider recovery CLI must launch for mixed-help validation");

        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        let stderr = String::from_utf8(output.stderr).expect("diagnostic must be UTF-8");
        assert!(stderr.contains("usage:") || stderr.contains("unknown argument"));
    }
}

#[cfg(unix)]
#[test]
fn provider_recovery_non_utf8_argument_fails_without_panic() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let opaque = OsString::from_vec(vec![b'-', b'-', b'o', b'p', b'a', b'q', 0xff]);
    let output = Command::new(env!("CARGO_BIN_EXE_disksage-provider-recovery"))
        .arg(opaque)
        .output()
        .expect("provider recovery CLI must launch for non-UTF-8 validation");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr)
            .expect("diagnostic must be UTF-8")
            .trim_end(),
        "invalid argument encoding"
    );
}
