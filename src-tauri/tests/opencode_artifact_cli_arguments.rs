//! Process-level argument handling regressions for the read-only OpenCode artifact planner.

#[cfg(unix)]
#[test]
fn non_utf8_argument_exits_cleanly_without_reflecting_payload() {
    use std::os::unix::ffi::OsStringExt;
    use std::process::Command;

    let home = tempfile::tempdir().expect("temporary HOME");
    let hostile = std::ffi::OsString::from_vec(vec![b'-', b'-', 0xff, b'x']);
    let output = Command::new(env!("CARGO_BIN_EXE_disksage-opencode-artifact-reclaim"))
        .env("HOME", home.path())
        .arg(hostile)
        .output()
        .expect("planner process must start");

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("diagnostic must be UTF-8");
    assert!(stderr.contains("argument must be valid UTF-8"));
    assert!(stderr.contains("usage: disksage-opencode-artifact-reclaim"));
    assert!(!stderr.contains("panicked"));
    assert!(stderr.len() < 512);
}
