use std::process::Command;

const EXPECTED_USAGE: &str = "DiskSage archive proof: usage: disksage-archive-tree --zip PATH [--expected-tree HEX40 | --prove-subset-of PATH] [--keep-top-level]";

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

#[test]
fn archive_tree_help_does_not_hide_an_unknown_argument() {
    let output = Command::new(env!("CARGO_BIN_EXE_disksage-archive-tree"))
        .args(["--help", "--unknown"])
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
    assert!(
        !output.stderr.is_empty(),
        "invalid invocation must remain visible through stderr"
    );
}

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
