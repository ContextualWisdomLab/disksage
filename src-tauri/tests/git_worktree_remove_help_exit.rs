use std::process::Command;

const EXPECTED_USAGE: &str = "usage: disksage-git-worktree-remove";

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_disksage-git-worktree-remove"))
}

#[test]
fn help_exits_successfully_with_usage_on_stdout() {
    for flag in ["--help", "-h"] {
        let output = command()
            .arg(flag)
            .output()
            .expect("worktree removal CLI must launch for help");

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
fn mixed_help_is_a_bounded_failure_without_mutation() {
    let output = command()
        .args(["--help", "--include-closed-pull-requests"])
        .output()
        .expect("worktree removal CLI must launch for mixed help");

    assert_eq!(output.status.code(), Some(64));
    assert!(
        output.stdout.is_empty(),
        "mixed help must not emit a report"
    );
    let stderr = String::from_utf8(output.stderr).expect("diagnostic must be UTF-8");
    assert!(stderr.contains("help must be used alone"));
}

#[cfg(unix)]
#[test]
fn non_utf8_argument_fails_without_panicking() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let output = command()
        .arg(OsString::from_vec(vec![b'-', b'-', 0xff]))
        .output()
        .expect("worktree removal CLI must launch for non-UTF-8 validation");

    assert_eq!(output.status.code(), Some(64));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("diagnostic must be UTF-8");
    assert!(!stderr.contains("panicked"));
    assert!(!stderr.contains('\u{fffd}'));
}
