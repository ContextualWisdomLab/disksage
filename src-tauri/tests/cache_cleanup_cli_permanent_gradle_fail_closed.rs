use std::process::Command;

#[test]
fn permanent_gradle_execution_is_rejected_before_filesystem_mutation() {
    let temp = tempfile::tempdir().expect("temporary cache cleanup fixture");
    let home = temp.path().join("home");
    let journal = temp.path().join("new-evidence-dir").join("journal.jsonl");
    std::fs::create_dir(&home).expect("create isolated home");

    let output = Command::new(env!("CARGO_BIN_EXE_disksage-cache-cleanup"))
        .env("HOME", &home)
        .env("XDG_DATA_HOME", temp.path().join("xdg-data"))
        .args([
            "--execute",
            "--cache-id",
            "gradle-cache",
            "--permanent-cache",
            "--journal-path",
        ])
        .arg(&journal)
        .output()
        .expect("run cache cleanup CLI");

    assert!(
        !output.status.success(),
        "irreversible Gradle deletion must remain unavailable"
    );
    assert!(output.stdout.is_empty(), "failure must not emit success JSON");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr).trim(),
        "disksage-cache-cleanup: cache-permanent-delete-unavailable"
    );
    assert!(
        !journal.exists() && !journal.parent().unwrap().exists(),
        "authority rejection must happen before journal-directory mutation"
    );
}

#[test]
fn reversible_named_gradle_preview_remains_available() {
    let temp = tempfile::tempdir().expect("temporary cache preview fixture");
    let home = temp.path().join("home");
    std::fs::create_dir(&home).expect("create isolated home");

    let output = Command::new(env!("CARGO_BIN_EXE_disksage-cache-cleanup"))
        .env("HOME", &home)
        .env("XDG_DATA_HOME", temp.path().join("xdg-data"))
        .args(["--cache-id", "gradle-cache"])
        .output()
        .expect("run cache cleanup preview");

    assert!(output.status.success(), "read-only named-cache preview stays usable");
    let receipt: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("preview emits machine-readable JSON");
    assert_eq!(receipt["executed"], false);
    assert_eq!(receipt["cache_id"], "gradle-cache");
    assert_eq!(receipt["permanent_cache"], false);
}
