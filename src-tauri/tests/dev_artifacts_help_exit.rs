use std::process::Command;

const EXPECTED_USAGE: &str = "usage: disksage-dev-artifacts --root ABSOLUTE_PATH";

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_disksage-dev-artifacts"))
}

#[test]
fn help_exits_successfully_with_usage_on_stdout() {
    for flag in ["--help", "-h"] {
        let output = command()
            .arg(flag)
            .output()
            .expect("development-artifact CLI must launch for help");

        assert!(output.status.success(), "bare {flag} must succeed");
        assert!(
            output.stderr.is_empty(),
            "bare {flag} must not write stderr"
        );
        assert!(String::from_utf8(output.stdout)
            .expect("help must be UTF-8")
            .starts_with(EXPECTED_USAGE));
    }
}

#[test]
fn mixed_help_is_a_bounded_failure_without_domain_work() {
    let fixture = tempfile::tempdir().expect("temporary fixture");

    let output = command()
        .arg("--help")
        .arg("--execute")
        .arg("--root")
        .arg(fixture.path())
        .output()
        .expect("development-artifact CLI must launch for mixed help");

    assert_eq!(output.status.code(), Some(2));
    assert!(
        output.stdout.is_empty(),
        "mixed help must not emit a cleanup report"
    );
    let stderr = String::from_utf8(output.stderr).expect("diagnostic must be UTF-8");
    assert!(stderr.contains("usage: disksage-dev-artifacts"));
}

#[cfg(unix)]
#[test]
fn non_utf8_argument_fails_without_panicking() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let output = command()
        .arg(OsString::from_vec(vec![b'-', b'-', 0xff]))
        .output()
        .expect("development-artifact CLI must launch for non-UTF-8 validation");

    assert_eq!(
        output.status.code(),
        Some(2),
        "malformed bytes must fail bounded instead of panicking"
    );
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("diagnostic must be UTF-8");
    assert!(!stderr.contains("panicked"));
    assert!(!stderr.contains('\u{fffd}'));
}
