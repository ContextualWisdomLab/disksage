#![cfg(target_os = "macos")]

use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn native_eviction_helper_rejects_untrusted_parent_process() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_disksage"))
        .env("DISKSAGE_NATIVE_ICLOUD_EVICTION_HELPER", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("DiskSage binary should start for helper authority regression");

    child
        .stdin
        .take()
        .expect("helper stdin should be available")
        .write_all(b"/tmp/disksage-untrusted-native-eviction")
        .expect("helper path should be writable");

    let output = child
        .wait_with_output()
        .expect("helper authority process should finish");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("icloud-native-eviction-helper-parent-untrusted"),
        "unexpected helper rejection evidence: {stderr}"
    );
}
