use std::process::Command;

const EXPECTED_USAGE: &str =
    "Usage: disksage-protect-path --path ABSOLUTE_PATH --class RETAINED_CLASS_IRI";

#[test]
fn protect_path_help_exits_successfully_without_error_output() {
    for flag in ["--help", "-h"] {
        let output = Command::new(env!("CARGO_BIN_EXE_disksage-protect-path"))
            .arg(flag)
            .output()
            .expect("protect-path CLI must launch for help");

        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        assert_eq!(
            String::from_utf8(output.stdout).expect("help must be UTF-8"),
            format!("{EXPECTED_USAGE}\n")
        );
    }
}

#[test]
fn protect_path_mixed_help_is_a_bounded_failure() {
    for arguments in [
        &(["--help", "--path", "/tmp/target"][..]),
        &(["--path", "/tmp/target", "--help"][..]),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_disksage-protect-path"))
            .args(arguments)
            .output()
            .expect("protect-path CLI must launch for mixed-help validation");

        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        let stderr = String::from_utf8(output.stderr).expect("diagnostic must be UTF-8");
        assert!(matches!(
            stderr.trim_end(),
            "disksage-protect-path: ontology-protection-missing-value"
                | "disksage-protect-path: ontology-protection-invalid-argument"
        ));
        assert!(!stderr.contains("/tmp/target"));
    }
}

#[cfg(unix)]
#[test]
fn protect_path_non_utf8_argument_fails_without_reflection() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let opaque = OsString::from_vec(vec![b'-', b'-', b'o', b'p', b'a', b'q', 0xff]);
    let output = Command::new(env!("CARGO_BIN_EXE_disksage-protect-path"))
        .arg(opaque)
        .output()
        .expect("protect-path CLI must launch for non-UTF-8 validation");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("diagnostic must be UTF-8");
    assert_eq!(
        stderr.trim_end(),
        "disksage-protect-path: ontology-protection-missing-value"
    );
    assert!(!stderr.contains("\u{fffd}"));
}
