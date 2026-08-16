//! Public-boundary regressions for safety failure paths that protect user data.
//!
//! These tests intentionally exercise real filesystem objects through the shipped public APIs.
//! They verify that collisions, malformed lexical paths, missing sources, and audit-journal
//! failures stop before DiskSage can overwrite or silently lose user data.

use crate::safety::{journal_recent, move_file, SafetyError};
use std::path::Path;

#[test]
fn move_refuses_existing_destination_without_journal_or_overwrite() {
    let root = tempfile::tempdir().expect("temporary move root");
    let source = root.path().join("source.bin");
    let destination = root.path().join("destination.bin");
    let journal = root.path().join("move-journal.jsonl");
    std::fs::write(&source, b"reviewed-source").expect("write source fixture");
    std::fs::write(&destination, b"preexisting-destination")
        .expect("write destination fixture");

    let error = move_file(&source, &destination, &journal, 30_001).unwrap_err();

    assert!(matches!(error, SafetyError::Trash(_)));
    assert_eq!(std::fs::read(&source).unwrap(), b"reviewed-source");
    assert_eq!(
        std::fs::read(&destination).unwrap(),
        b"preexisting-destination"
    );
    assert!(
        !journal.exists(),
        "destination collision must fail before a pending mutation journal is written"
    );
}

#[test]
fn move_rejects_parent_traversal_before_parent_creation_or_journaling() {
    let root = tempfile::tempdir().expect("temporary move root");
    let source = root.path().join("source.bin");
    let destination = root
        .path()
        .join("new-parent")
        .join("..")
        .join("destination.bin");
    let journal = root.path().join("move-journal.jsonl");
    std::fs::write(&source, b"reviewed-source").expect("write source fixture");

    let error = move_file(&source, &destination, &journal, 30_002).unwrap_err();

    assert!(matches!(error, SafetyError::Protected(_)));
    assert_eq!(std::fs::read(&source).unwrap(), b"reviewed-source");
    assert!(!root.path().join("new-parent").exists());
    assert!(!journal.exists());
}

#[test]
fn missing_source_records_pending_then_bounded_error_without_destination() {
    let root = tempfile::tempdir().expect("temporary move root");
    let source = root.path().join("missing-source.bin");
    let destination = root.path().join("destination.bin");
    let journal = root.path().join("move-journal.jsonl");

    let error = move_file(&source, &destination, &journal, 30_003).unwrap_err();

    assert!(matches!(error, SafetyError::Trash(_)));
    assert!(!source.exists());
    assert!(!destination.exists());
    let recent = journal_recent(&journal, 10);
    assert_eq!(recent.len(), 2, "move failure must retain pending and terminal audit records");
    assert!(recent[0].outcome.starts_with("error:"));
    assert_eq!(recent[1].outcome, "pending");
    assert_eq!(recent[0].ts_ms, 30_003);
    assert_eq!(recent[1].ts_ms, 30_003);
}

#[test]
fn journal_failure_leaves_source_and_destination_untouched() {
    let root = tempfile::tempdir().expect("temporary move root");
    let source = root.path().join("source.bin");
    let destination = root.path().join("destination.bin");
    std::fs::write(&source, b"reviewed-source").expect("write source fixture");

    let error = move_file(&source, &destination, root.path(), 30_004).unwrap_err();

    assert!(matches!(error, SafetyError::Journal(_)));
    assert_eq!(std::fs::read(&source).unwrap(), b"reviewed-source");
    assert!(!destination.exists());
}

#[cfg(unix)]
#[test]
fn protected_system_destination_is_rejected_before_any_mutation() {
    let root = tempfile::tempdir().expect("temporary move root");
    let source = root.path().join("source.bin");
    let journal = root.path().join("move-journal.jsonl");
    std::fs::write(&source, b"reviewed-source").expect("write source fixture");

    let error = move_file(
        &source,
        Path::new("/usr/disksage-must-not-write.bin"),
        &journal,
        30_005,
    )
    .unwrap_err();

    assert!(matches!(error, SafetyError::Protected(_)));
    assert_eq!(std::fs::read(&source).unwrap(), b"reviewed-source");
    assert!(!journal.exists());
}
