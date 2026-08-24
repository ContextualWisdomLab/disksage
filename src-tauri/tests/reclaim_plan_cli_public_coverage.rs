//! Black-box coverage for the shipped read-only reclaim-plan CLI boundary.
//!
//! The tests execute the real binary against temporary filesystem objects so argument
//! decoding, planner invocation, JSON projection, and terminal error behavior are covered
//! together without granting cleanup authority.

use std::fs;
use std::process::Command;

/// Returns the compiled reclaim-plan binary used by Cargo integration tests.
fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_disksage-reclaim-plan"))
}

#[test]
fn sole_help_flags_are_successful_terminal_actions() {
    for help_flag in ["--help", "-h"] {
        let output = command()
            .arg(help_flag)
            .output()
            .expect("the reclaim-plan binary should start");

        assert_eq!(output.status.code(), Some(0), "help flag: {help_flag}");
        let stdout = String::from_utf8(output.stdout).expect("stdout should remain UTF-8");
        let stderr = String::from_utf8(output.stderr).expect("stderr should remain UTF-8");
        assert!(stdout.contains("Usage: disksage-reclaim-plan"));
        assert!(stdout.contains("never moves or deletes files"));
        assert!(stderr.is_empty(), "help flag: {help_flag}, stderr: {stderr}");
    }
}

#[test]
fn missing_paths_and_operation_values_fail_closed() {
    let cases: &[(&[&str], &str)] = &[
        (&[], "at least one path is required"),
        (&["--operation"], "--operation requires trash or delete"),
        (
            &["--operation", "erase"],
            "unsupported operation: erase; expected trash or delete",
        ),
    ];

    for (arguments, expected_error) in cases {
        let output = command()
            .args(*arguments)
            .output()
            .expect("the reclaim-plan binary should start");

        assert_eq!(output.status.code(), Some(2), "arguments: {arguments:?}");
        assert!(output.stdout.is_empty(), "arguments: {arguments:?}");
        let stderr = String::from_utf8(output.stderr).expect("stderr should remain UTF-8");
        assert!(
            stderr.contains(expected_error),
            "arguments: {arguments:?}, stderr: {stderr}"
        );
        assert!(!stderr.contains("panicked"), "arguments: {arguments:?}");
    }
}

#[test]
fn default_trash_plan_emits_compact_read_only_json_for_a_real_file() {
    let temp = tempfile::tempdir().expect("temporary directory should be created");
    let file = temp.path().join("candidate.bin");
    let payload = b"DiskSage reclaim evidence";
    fs::write(&file, payload).expect("fixture should be written");

    let output = command()
        .arg(&file)
        .output()
        .expect("the reclaim-plan binary should start");

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).expect("stdout should remain UTF-8");
    assert!(
        !stdout.contains("\n  \""),
        "compact JSON unexpectedly used pretty indentation: {stdout}"
    );
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("stdout should be JSON");
    assert_eq!(value["schema_kind"], "disksage.reclaim-plan");
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["operation"], "trash");
    assert_eq!(value["paths"].as_array().map(Vec::len), Some(1));
    assert_eq!(value["paths"][0]["kind"], "file");
    assert_eq!(value["paths"][0]["files"], 1);
    assert_eq!(
        value["paths"][0]["estimate"]["logical_bytes"],
        payload.len() as u64
    );
    assert!(value["paths"][0].get("active_use").is_none());
    assert_eq!(
        value["totals"]["physically_reclaimable_bytes"],
        serde_json::Value::Null
    );
    assert!(value["totals"]["reason_codes"]
        .as_array()
        .expect("reason codes should be an array")
        .iter()
        .any(|reason| reason == "trash-retains-bytes-until-emptied"));
    assert!(file.exists(), "the read-only planner must preserve its input");
}

#[test]
fn delete_pretty_plan_changes_only_evidence_semantics() {
    let temp = tempfile::tempdir().expect("temporary directory should be created");
    let file = temp.path().join("candidate.txt");
    fs::write(&file, b"delete planning remains read only").expect("fixture should be written");

    let output = command()
        .args(["--operation", "delete", "--pretty"])
        .arg(&file)
        .output()
        .expect("the reclaim-plan binary should start");

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).expect("stdout should remain UTF-8");
    assert!(stdout.contains("\n  \""), "pretty JSON was not emitted: {stdout}");
    let value: serde_json::Value = serde_json::from_str(&stdout).expect("stdout should be JSON");
    assert_eq!(value["operation"], "delete");
    assert!(!value["totals"]["reason_codes"]
        .as_array()
        .expect("reason codes should be an array")
        .iter()
        .any(|reason| reason == "trash-retains-bytes-until-emptied"));
    assert!(file.exists(), "delete planning must not perform deletion");
}

#[test]
fn double_dash_preserves_an_option_like_native_path() {
    let temp = tempfile::tempdir().expect("temporary directory should be created");
    let file = temp.path().join("--candidate");
    fs::write(&file, b"x").expect("fixture should be written");

    let output = command()
        .current_dir(temp.path())
        .args(["--", "--candidate"])
        .output()
        .expect("the reclaim-plan binary should start");

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(value["paths"].as_array().map(Vec::len), Some(1));
    assert_eq!(value["paths"][0]["files"], 1);
    assert!(file.exists(), "the read-only planner must preserve its input");
}

#[cfg(unix)]
#[test]
fn non_utf8_operation_value_is_rejected_without_panicking() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let output = command()
        .arg("--operation")
        .arg(OsString::from_vec(vec![0xff, b'x']))
        .output()
        .expect("the reclaim-plan binary should start");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("stderr should remain UTF-8");
    assert!(stderr.contains("valid UTF-8 value"));
    assert!(!stderr.contains("panicked"));
}
