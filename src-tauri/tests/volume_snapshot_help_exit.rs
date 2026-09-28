use std::process::Command;

const EXPECTED_USAGE: &str =
    "Usage: disksage-volume-snapshot [--path PATH] [--baseline SNAPSHOT_JSON --logical-removed-bytes BYTES]";

#[test]
fn volume_snapshot_help_exits_successfully_without_error_output() {
    for flag in ["--help", "-h"] {
        let output = Command::new(env!("CARGO_BIN_EXE_disksage-volume-snapshot"))
            .arg(flag)
            .output()
            .expect("volume snapshot CLI must launch for its help contract");

        assert!(
            output.status.success(),
            "{flag} must succeed, got status {:?} and stderr {:?}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            output.stderr.is_empty(),
            "successful help must not use stderr"
        );
        assert_eq!(
            String::from_utf8(output.stdout).expect("help output must be UTF-8"),
            format!("{EXPECTED_USAGE}\n")
        );
    }
}

#[test]
fn volume_snapshot_mixed_help_is_a_bounded_failure() {
    for arguments in [["--help", "--path", "."], ["--path", ".", "--help"]] {
        let output = Command::new(env!("CARGO_BIN_EXE_disksage-volume-snapshot"))
            .args(arguments)
            .output()
            .expect("volume snapshot CLI must launch for mixed-help validation");

        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert_eq!(
            String::from_utf8(output.stderr)
                .expect("diagnostic must be UTF-8")
                .trim_end(),
            "error:local-volume-help-requires-alone"
        );
    }
}

#[test]
fn volume_snapshot_unknown_argument_is_bounded_without_reflection() {
    let output = Command::new(env!("CARGO_BIN_EXE_disksage-volume-snapshot"))
        .arg("--opaque-option=not-shown")
        .output()
        .expect("volume snapshot CLI must launch for unknown-argument validation");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("diagnostic must be UTF-8");
    assert_eq!(stderr.trim_end(), "error:local-volume-argument-unknown");
    assert!(!stderr.contains("not-shown"));
}

#[cfg(unix)]
#[test]
fn volume_snapshot_non_utf8_argument_fails_without_panic_or_reflection() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let opaque = OsString::from_vec(vec![b'-', b'-', b'o', b'p', b'a', b'q', 0xff]);
    let output = Command::new(env!("CARGO_BIN_EXE_disksage-volume-snapshot"))
        .arg(opaque)
        .output()
        .expect("volume snapshot CLI must launch for non-UTF-8 validation");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("diagnostic must be UTF-8");
    assert_eq!(stderr.trim_end(), "error:local-volume-argument-non-utf8");
    assert!(!stderr.contains("\u{fffd}"));
}
