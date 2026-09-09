#![cfg(target_os = "macos")]

use std::io::Write;
use std::process::{Command, Stdio};

fn assert_untrusted_parent_rejected(executable: &str) {
    let mut child = Command::new(executable)
        .env("DISKSAGE_NATIVE_ICLOUD_EVICTION_HELPER", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("DiskSage helper-capable binary should start for authority regression");

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

#[test]
fn desktop_native_eviction_helper_rejects_untrusted_parent_process() {
    assert_untrusted_parent_rejected(env!("CARGO_BIN_EXE_disksage"));
}

#[test]
fn single_item_cli_native_eviction_helper_rejects_untrusted_parent_process() {
    assert_untrusted_parent_rejected(env!(
        "CARGO_BIN_EXE_disksage-icloud-local-eviction"
    ));
}

#[test]
fn batch_cli_native_eviction_helper_rejects_untrusted_parent_process() {
    assert_untrusted_parent_rejected(env!(
        "CARGO_BIN_EXE_disksage-icloud-local-eviction-batch"
    ));
}
