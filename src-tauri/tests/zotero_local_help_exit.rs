use std::process::Command;

const EXPECTED_USAGE: &str = "usage: disksage-zotero-local --input ABSOLUTE_JSON [--execute]";

#[test]
fn zotero_local_help_exits_successfully_without_error_output() {
    for flag in ["--help", "-h"] {
        let output = Command::new(env!("CARGO_BIN_EXE_disksage-zotero-local"))
            .arg(flag)
            .output()
            .expect("Zotero local CLI must launch for its help contract");

        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        assert_eq!(
            String::from_utf8(output.stdout).expect("help output must be UTF-8"),
            format!("{EXPECTED_USAGE}\n")
        );
    }
}

#[test]
fn zotero_local_mixed_help_is_a_bounded_failure() {
    for arguments in [
        &(["--help", "--execute"][..]),
        &(["--input", "/tmp/input.json", "--help"][..]),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_disksage-zotero-local"))
            .args(arguments)
            .output()
            .expect("Zotero local CLI must launch for mixed-help validation");

        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert_eq!(
            String::from_utf8(output.stderr)
                .expect("diagnostic must be UTF-8")
                .trim_end(),
            "usage: disksage-zotero-local --input ABSOLUTE_JSON [--execute]: help must be used alone"
        );
    }
}

#[cfg(unix)]
#[test]
fn zotero_local_non_utf8_argument_fails_without_reflection() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let opaque = OsString::from_vec(vec![b'-', b'-', b'o', b'p', b'a', b'q', 0xff]);
    let output = Command::new(env!("CARGO_BIN_EXE_disksage-zotero-local"))
        .arg(opaque)
        .output()
        .expect("Zotero local CLI must launch for non-UTF-8 validation");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("diagnostic must be UTF-8");
    assert_eq!(
        stderr.trim_end(),
        "usage: disksage-zotero-local --input ABSOLUTE_JSON [--execute]: invalid UTF-8 option"
    );
    assert!(!stderr.contains("\u{fffd}"));
}
