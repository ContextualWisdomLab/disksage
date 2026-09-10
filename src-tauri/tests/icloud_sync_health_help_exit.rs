use std::process::Command;

const EXPECTED_USAGE: &str =
    "usage: disksage-icloud-sync-health [--db-dir ABSOLUTE_CLOUDDOCS_DB_DIR] [--output ABSOLUTE_NEW_FILE.json]";

#[test]
fn icloud_sync_health_help_exits_successfully_without_error_output() {
    for flag in ["--help", "-h"] {
        let output = Command::new(env!("CARGO_BIN_EXE_disksage-icloud-sync-health"))
            .arg(flag)
            .output()
            .expect("iCloud sync health CLI must launch for help");

        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        assert_eq!(
            String::from_utf8(output.stdout).expect("help must be UTF-8"),
            format!("{EXPECTED_USAGE}\n")
        );
    }
}

#[test]
fn icloud_sync_health_mixed_help_is_a_bounded_failure() {
    for arguments in [
        &(["--help", "--db-dir", "/tmp/db"][..]),
        &(["--db-dir", "/tmp/db", "--help"][..]),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_disksage-icloud-sync-health"))
            .args(arguments)
            .output()
            .expect("iCloud sync health CLI must launch for mixed-help validation");

        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        let stderr = String::from_utf8(output.stderr).expect("diagnostic must be UTF-8");
        assert!(stderr.contains("DiskSage iCloud sync health:"));
        assert!(stderr.contains("usage:"));
    }
}

#[cfg(unix)]
#[test]
fn icloud_sync_health_non_utf8_argument_fails_without_reflection() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let opaque = OsString::from_vec(vec![b'-', b'-', b'o', b'p', b'a', b'q', 0xff]);
    let output = Command::new(env!("CARGO_BIN_EXE_disksage-icloud-sync-health"))
        .arg(opaque)
        .output()
        .expect("iCloud sync health CLI must launch for non-UTF-8 validation");

    assert_eq!(
        output.status.code(),
        Some(1),
        "unexpected child output: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("diagnostic must be UTF-8");
    assert!(stderr.contains("invalid argument encoding"));
    assert!(!stderr.contains("\u{fffd}"));
}
