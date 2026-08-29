#![cfg(target_os = "macos")]

use std::process::Command;

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
    assert!(output.stdout.is_empty(), "failed preview must not publish partial inactivity evidence");
}
