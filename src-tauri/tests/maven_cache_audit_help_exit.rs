use std::process::Command;

const EXPECTED_USAGE: &str = "usage: disksage-maven-cache-audit --repository-root ABSOLUTE_PATH [--output NEW_ABSOLUTE_JSON_PATH] [--max-entries N] [--max-candidates N] [--max-issues N]";

#[test]
fn maven_cache_audit_help_exits_successfully_without_error_output() {
    for flag in ["--help", "-h"] {
        let output = Command::new(env!("CARGO_BIN_EXE_disksage-maven-cache-audit"))
            .arg(flag)
            .output()
            .expect("Maven audit CLI must launch for help");

        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        assert_eq!(
            String::from_utf8(output.stdout).expect("help must be UTF-8"),
            format!("{EXPECTED_USAGE}\n")
        );
    }
}

#[test]
fn maven_cache_audit_mixed_help_is_a_bounded_failure() {
    for arguments in [
        &(["--help", "--repository-root", "/tmp/repository"][..]),
        &(["--repository-root", "/tmp/repository", "--help"][..]),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_disksage-maven-cache-audit"))
            .args(arguments)
            .output()
            .expect("Maven audit CLI must launch for mixed help");

        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert_eq!(
            String::from_utf8(output.stderr)
                .expect("diagnostic must be UTF-8")
                .trim_end(),
            "help must be used alone"
        );
    }
}

#[cfg(unix)]
#[test]
fn maven_cache_audit_non_utf8_argument_fails_without_reflection() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let opaque = OsString::from_vec(vec![b'-', b'-', b'o', b'p', b'a', b'q', 0xff]);
    let output = Command::new(env!("CARGO_BIN_EXE_disksage-maven-cache-audit"))
        .arg(opaque)
        .output()
        .expect("Maven audit CLI must launch for non-UTF-8 validation");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("diagnostic must be UTF-8");
    assert_eq!(stderr.trim_end(), "인자를 UTF-8로 해석할 수 없음");
    assert!(!stderr.contains("\u{fffd}"));
}
