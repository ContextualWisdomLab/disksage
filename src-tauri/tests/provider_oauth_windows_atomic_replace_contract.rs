#![allow(dead_code, unused_imports)]

//! Windows durable OAuth metadata replacement must never create a delete-before-publish window.
//!
//! `std::fs::rename` is the cross-platform publication primitive used by this historical contract
//! and replaces an existing regular destination on supported Windows filesystems. A separate
//! `remove_file(path)` before that call destroys the last known-good connection document if
//! replacement then fails. Keep the regression source-only so it does not duplicate private OAuth
//! production modules in the integration-test crate.

#[test]
fn production_writer_does_not_predelete_the_durable_destination() {
    let source = include_str!("../src/provider_oauth.rs");
    let predelete = "#[cfg(windows)]\n    if path.exists() {\n        std::fs::remove_file(path)";

    assert!(
        !source.contains(predelete),
        "Windows replacement must not delete the durable OAuth document before the replacement primitive runs"
    );
}

#[cfg(windows)]
#[test]
fn windows_std_rename_replaces_an_existing_regular_file() {
    let temp = tempfile::tempdir().unwrap();
    let durable = temp.path().join("connections.json");
    let replacement = temp.path().join("connections.tmp");
    std::fs::write(&durable, b"old-durable-document").unwrap();
    std::fs::write(&replacement, b"new-complete-document").unwrap();

    std::fs::rename(&replacement, &durable).unwrap();

    assert_eq!(std::fs::read(&durable).unwrap(), b"new-complete-document");
    assert!(!replacement.exists());
}
