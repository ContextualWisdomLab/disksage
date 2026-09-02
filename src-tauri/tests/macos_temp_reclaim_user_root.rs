#![cfg(target_os = "macos")]

use std::process::Command;

#[test]
fn native_temp_plan_respects_the_current_user_tmpdir() {
    let root = tempfile::tempdir().expect("a private temporary root must be creatable");
    let expected = root
        .path()
        .canonicalize()
        .expect("the private temporary root must canonicalize");

    let output = Command::new(env!("CARGO_BIN_EXE_disksage-temp-reclaim"))
        .env("TMPDIR", root.path())
        .output()
        .expect("the shipped temporary reclaim CLI must start");

    assert!(
        output.status.success(),
        "planning a private user temp root must succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let plan: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("the planner must emit JSON");
    assert_eq!(
        plan.get("canonical_root").and_then(serde_json::Value::as_str),
        Some(expected.to_string_lossy().as_ref()),
        "macOS native reclaim must stay inside the current user's TMPDIR instead of the shared /tmp alias"
    );
}
