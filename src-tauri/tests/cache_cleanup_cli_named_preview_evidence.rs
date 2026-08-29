#![cfg(unix)]

use std::fs;
use std::process::Command;

fn unique_temp_dir() -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!(
        "disksage-cache-preview-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create fixture root");
    root
}

#[test]
fn named_cache_dry_run_publishes_target_manifest_and_active_use_evidence() {
    let root = unique_temp_dir();
    let home = root.join("home");
    let cache_child = home.join(".npm").join("_cacache");
    fs::create_dir_all(&cache_child).expect("create npm cache child");
    fs::write(cache_child.join("payload.bin"), b"bounded-cache-preview").expect("write payload");

    let output = Command::new(env!("CARGO_BIN_EXE_disksage-cache-cleanup"))
        .arg("--cache-id")
        .arg("npm-cache")
        .env("HOME", &home)
        .env("XDG_CACHE_HOME", root.join("xdg-cache"))
        .output()
        .expect("run cache cleanup preview");

    assert!(
        output.status.success(),
        "preview failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());

    let receipt: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("preview emits JSON");
    assert_eq!(receipt["executed"], false);
    assert_eq!(receipt["cache_id"], "npm-cache");

    let targets = receipt["cache_targets"]
        .as_array()
        .expect("named-cache preview must publish cache_targets");
    assert_eq!(targets.len(), 1);
    let target = &targets[0];
    assert_eq!(target["path"], cache_child.to_string_lossy().as_ref());
    assert!(target["bytes"].as_u64().is_some_and(|bytes| bytes > 0));
    assert!(target["modified_ms"].as_u64().is_some());
    assert!(target["object_id"].as_str().is_some_and(|value| !value.is_empty()));
    assert!(target["manifest_fingerprint"]
        .as_str()
        .is_some_and(|value| value.len() == 64));

    let active_use = target["active_use"]
        .as_object()
        .expect("each target must publish active-use evidence");
    assert!(active_use
        .get("method")
        .and_then(|value| value.as_str())
        .is_some_and(|value| !value.is_empty()));
    assert!(active_use.get("assessed").is_some_and(|value| value.is_boolean()));
    assert!(active_use
        .get("evidence_complete")
        .is_some_and(|value| value.is_boolean()));
    assert!(active_use.get("active").is_some_and(|value| value.is_boolean()));

    fs::remove_dir_all(root).expect("remove fixture");
}
