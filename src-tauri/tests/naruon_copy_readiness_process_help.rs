use std::process::Command;

const EXPECTED_USAGE: &str = "usage: disksage-naruon-copy-readiness-verify ABSOLUTE_READINESS.json";

#[test]
fn naruon_readiness_help_exits_successfully_without_error_output() {
    for flag in ["--help", "-h"] {
        let output = Command::new(env!("CARGO_BIN_EXE_disksage-naruon-copy-readiness-verify"))
            .arg(flag)
            .output()
            .expect("Naruon readiness verifier must launch for help");

        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        assert_eq!(
            String::from_utf8(output.stdout).expect("help must be UTF-8"),
            format!("{EXPECTED_USAGE}\n")
        );
    }
}

#[test]
fn naruon_readiness_mixed_help_is_a_bounded_failure() {
    let output = Command::new(env!("CARGO_BIN_EXE_disksage-naruon-copy-readiness-verify"))
        .args(["--help", "relative.json"])
        .output()
        .expect("Naruon readiness verifier must launch for mixed-help validation");

    assert_eq!(output.status.code(), Some(64));
    let stdout = String::from_utf8(output.stdout).expect("error JSON must be UTF-8");
    assert!(stdout.contains("naruon-copy-readiness-verifier-usage-invalid"));
    assert!(String::from_utf8(output.stderr)
        .expect("usage diagnostic must be UTF-8")
        .contains(EXPECTED_USAGE));
}

#[cfg(unix)]
#[test]
fn naruon_readiness_non_utf8_argument_fails_without_panic() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let opaque = OsString::from_vec(vec![b'/', b'r', b'e', b'a', b'd', 0xff]);
    let output = Command::new(env!("CARGO_BIN_EXE_disksage-naruon-copy-readiness-verify"))
        .arg(opaque)
        .output()
        .expect("Naruon readiness verifier must launch for non-UTF-8 validation");

    assert_eq!(output.status.code(), Some(65));
    let stdout = String::from_utf8(output.stdout).expect("error JSON must be UTF-8");
    assert!(stdout.contains("\"ok\":false"));
    assert!(!stdout.contains("\u{fffd}"));
}
