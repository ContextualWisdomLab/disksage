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

#[test]
fn named_cache_preview_fails_closed_before_unbounded_active_use_probes() {
    let temp = tempfile::tempdir().expect("temporary bounded preview fixture");
    let home = temp.path().join("home");
    let gradle_home = temp.path().join("gradle-home");
    let cache_root = gradle_home.join("caches");
    std::fs::create_dir_all(&home).expect("create isolated home");
    std::fs::create_dir_all(&cache_root).expect("create isolated Gradle cache root");

    for index in 0..65 {
        let child = cache_root.join(format!("generated-{index:02}"));
        std::fs::create_dir(&child).expect("create bounded preview child");
        std::fs::write(child.join("payload.bin"), b"regenerable")
            .expect("write bounded preview payload");
    }

    let output = Command::new(env!("CARGO_BIN_EXE_disksage-cache-cleanup"))
        .env("HOME", &home)
        .env("GRADLE_USER_HOME", &gradle_home)
        .env("XDG_DATA_HOME", temp.path().join("xdg-data"))
        .args(["--cache-id", "gradle-cache"])
        .output()
        .expect("run bounded cache cleanup preview");

    assert!(!output.status.success(), "oversized preview must fail closed");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr).trim(),
        "disksage-cache-cleanup: cache-preview-target-limit-exceeded"
    );
    assert!(
        output.stdout.is_empty(),
        "failed preview must not publish partial inactivity evidence"
    );
}
