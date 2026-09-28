use std::process::Command;

const EXPECTED_USAGE: &str = "usage: disksage-git-worktree-metadata-prune --repository-root ABSOLUTE_PATH [--execute --confirm EXACT_PHRASE --rationale TEXT --record-path ABSOLUTE_PATH]";

#[test]
fn metadata_prune_help_exits_successfully_without_error_output() {
    for flag in ["--help", "-h"] {
        let output = Command::new(env!("CARGO_BIN_EXE_disksage-git-worktree-metadata-prune"))
            .arg(flag)
            .output()
            .expect("metadata prune CLI must launch for help");

        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        assert_eq!(
            String::from_utf8(output.stdout).expect("help must be UTF-8"),
            format!("{EXPECTED_USAGE}\n")
        );
    }
}

#[test]
fn metadata_prune_mixed_help_is_a_bounded_failure() {
    for arguments in [
        &(["--help", "--repository-root", "/tmp/repository"][..]),
        &(["--repository-root", "/tmp/repository", "--help"][..]),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_disksage-git-worktree-metadata-prune"))
            .args(arguments)
            .output()
            .expect("metadata prune CLI must launch for mixed-help validation");

        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        let stderr = String::from_utf8(output.stderr).expect("diagnostic must be UTF-8");
        assert!(stderr.contains("help must be used alone"));
        assert!(stderr.contains(EXPECTED_USAGE));
    }
}

#[cfg(unix)]
#[test]
fn metadata_prune_non_utf8_argument_fails_without_reflection() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let opaque = OsString::from_vec(vec![b'-', b'-', b'o', b'p', b'a', b'q', 0xff]);
    let output = Command::new(env!("CARGO_BIN_EXE_disksage-git-worktree-metadata-prune"))
        .arg(opaque)
        .output()
        .expect("metadata prune CLI must launch for non-UTF-8 validation");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("diagnostic must be UTF-8");
    assert_eq!(
        stderr.trim_end(),
        "disksage-git-worktree-metadata-prune: option must be valid UTF-8"
    );
    assert!(!stderr.contains("\u{fffd}"));
}
