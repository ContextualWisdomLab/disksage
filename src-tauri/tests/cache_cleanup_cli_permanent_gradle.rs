#![cfg(target_os = "macos")]

use std::process::Command;

#[test]
fn permanent_gradle_execution_removes_only_the_fresh_catalogued_tree_and_journals() {
    let temp = tempfile::tempdir().expect("temporary cache cleanup fixture");
    let home = temp.path().join("home");
    let journal = temp.path().join("new-evidence-dir").join("journal.jsonl");
    std::fs::create_dir(&home).expect("create isolated home");
    let cache_child = home.join(".gradle/caches/modules/generated.bin");
    std::fs::create_dir_all(cache_child.parent().unwrap()).expect("create Gradle cache fixture");
    std::fs::write(&cache_child, b"regenerable").expect("write Gradle cache fixture");

    let output = Command::new(env!("CARGO_BIN_EXE_disksage-cache-cleanup"))
        .env("HOME", &home)
        .env_remove("GRADLE_USER_HOME")
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
        output.status.success(),
        "safe fixture must be reclaimed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!cache_child.parent().unwrap().exists());
    let receipt: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("execution emits machine-readable JSON");
    assert_eq!(receipt["executed"], true);
    assert_eq!(receipt["cache_id"], "gradle-cache");
    assert_eq!(receipt["permanent_cache"], true);
    assert!(
        std::fs::read_to_string(&journal)
            .expect("journal must exist")
            .contains("permanent_generated_directory_delete"),
        "permanent cleanup must retain its mutation evidence"
    );
}

#[test]
fn reversible_named_gradle_preview_remains_available() {
    let temp = tempfile::tempdir().expect("temporary cache preview fixture");
    let home = temp.path().join("home");
    std::fs::create_dir(&home).expect("create isolated home");

    let output = Command::new(env!("CARGO_BIN_EXE_disksage-cache-cleanup"))
        .env("HOME", &home)
        .env_remove("GRADLE_USER_HOME")
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
