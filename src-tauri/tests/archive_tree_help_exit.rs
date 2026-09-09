use std::process::Command;

const EXPECTED_USAGE: &str = "DiskSage archive proof: usage: disksage-archive-tree --zip PATH [--expected-tree HEX40 | --prove-subset-of PATH] [--keep-top-level]";

/// Prove both help flags return the exact stable usage line and empty stderr.
#[test]
fn archive_tree_help_exits_successfully_without_error_output() {
    for flag in ["--help", "-h"] {
        let output = Command::new(env!("CARGO_BIN_EXE_disksage-archive-tree"))
            .arg(flag)
            .output()
            .expect("archive-tree CLI must launch for its help contract");

        assert!(
            output.status.success(),
            "{flag} must be a successful terminal action, got status {:?} and stderr {:?}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            output.stderr.is_empty(),
            "successful help must not be projected through stderr"
        );
        let stdout = String::from_utf8(output.stdout).expect("help output must be valid UTF-8");
        assert_eq!(
            stdout,
            format!("{EXPECTED_USAGE}\n"),
            "help output must equal the stable usage synopsis"
        );
    }
}

/// Prove mixed help and opaque invalid input stays a bounded failure in either order.
#[test]
fn archive_tree_help_does_not_hide_or_reflect_an_unknown_argument() {
    for arguments in [
        ["--help", "--opaque-option=not-shown"],
        ["--opaque-option=not-shown", "--help"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_disksage-archive-tree"))
            .args(arguments)
            .output()
            .expect("archive-tree CLI must launch for invalid-argument validation");

        assert!(
            !output.status.success(),
            "help must not turn an otherwise invalid invocation into success"
        );
        assert!(
            output.stdout.is_empty(),
            "invalid invocation must not emit help on stdout"
        );
        let stderr =
            String::from_utf8(output.stderr).expect("CLI diagnostics must be valid UTF-8");
        assert!(
            !stderr.is_empty(),
            "invalid invocation must remain visible through stderr"
        );
        assert!(
            !stderr.contains("not-shown"),
            "mixed help diagnostics must not reflect opaque argument payloads"
        );
    }
}

/// Prove an unknown option uses the stable bounded diagnostic without reflection.
#[test]
fn archive_tree_unknown_argument_uses_bounded_diagnostic() {
    let output = Command::new(env!("CARGO_BIN_EXE_disksage-archive-tree"))
        .arg("--opaque-option=not-shown")
        .output()
        .expect("archive-tree CLI must launch for invalid-argument validation");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("CLI diagnostics must be valid UTF-8");
    assert_eq!(stderr.trim_end(), "archive-tree-unknown-argument");
    assert!(!stderr.contains("not-shown"));
}

/// Prove hostile non-UTF-8 arguments fail through the stable diagnostic on Unix.
#[cfg(unix)]
#[test]
fn archive_tree_non_utf8_argument_fails_without_panic() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let opaque = OsString::from_vec(vec![b'-', b'-', b'o', b'p', b'a', b'q', b'u', b'e', 0xff]);
    let output = Command::new(env!("CARGO_BIN_EXE_disksage-archive-tree"))
        .arg(opaque)
        .output()
        .expect("archive-tree CLI must launch for non-UTF-8 argument validation");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("CLI diagnostics must be valid UTF-8");
    assert_eq!(stderr.trim_end(), "archive-tree-argument-invalid");
}
