//! Public-boundary regressions for safety failure paths that protect user data.
//!
//! These tests intentionally exercise real filesystem objects through the shipped public APIs.
//! They verify that collisions, malformed lexical paths, missing sources, and audit-journal
//! failures stop before DiskSage can overwrite or silently lose user data.

use crate::safety::{journal_recent, move_file, trash_delete, SafetyError};
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

#[test]
fn destination_parent_creation_failure_preserves_source_and_audit_state() {
    let root = tempfile::tempdir().expect("temporary move root");
    let source = root.path().join("source.bin");
    let blocking_parent = root.path().join("not-a-directory");
    let destination = blocking_parent.join("destination.bin");
    let journal = root.path().join("move-journal.jsonl");
    std::fs::write(&source, b"reviewed-source").expect("write source fixture");
    std::fs::write(&blocking_parent, b"blocking-file").expect("write blocking parent fixture");

    let error = move_file(&source, &destination, &journal, 30_005).unwrap_err();

    assert!(matches!(error, SafetyError::Trash(_)));
    assert_eq!(std::fs::read(&source).unwrap(), b"reviewed-source");
    assert_eq!(std::fs::read(&blocking_parent).unwrap(), b"blocking-file");
    assert!(!destination.exists());
    assert!(
        !journal.exists(),
        "parent creation failure must occur before the pending move journal is written"
    );
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
        30_006,
    )
    .unwrap_err();

    assert!(matches!(error, SafetyError::Protected(_)));
    assert_eq!(std::fs::read(&source).unwrap(), b"reviewed-source");
    assert!(!journal.exists());
}

#[test]
fn trash_missing_source_records_pending_then_terminal_error() {
    let root = tempfile::tempdir().expect("temporary trash root");
    let missing = root.path().join("missing-source.bin");
    let journal = root.path().join("trash-journal.jsonl");

    let error = trash_delete(&missing, 17, &journal, 30_007).unwrap_err();

    assert!(matches!(error, SafetyError::Trash(_)));
    assert!(!missing.exists());
    let recent = journal_recent(&journal, 10);
    assert_eq!(recent.len(), 2, "trash failure must preserve pending and terminal audit records");
    assert!(recent[0].outcome.starts_with("error:"));
    assert_eq!(recent[1].outcome, "pending");
    assert_eq!(recent[0].op, "trash_delete");
    assert_eq!(recent[1].op, "trash_delete");
    assert_eq!(recent[0].ts_ms, 30_007);
    assert_eq!(recent[1].ts_ms, 30_007);
    assert_eq!(recent[0].path, missing.to_string_lossy());
    assert_eq!(recent[1].path, missing.to_string_lossy());
}

#[test]
fn trash_journal_failure_preserves_reviewed_source() {
    let root = tempfile::tempdir().expect("temporary trash root");
    let source = root.path().join("source.bin");
    std::fs::write(&source, b"reviewed-source").expect("write source fixture");

    let error = trash_delete(&source, 15, root.path(), 30_008).unwrap_err();

    assert!(matches!(error, SafetyError::Journal(_)));
    assert_eq!(std::fs::read(&source).unwrap(), b"reviewed-source");
}

#[cfg(unix)]
#[test]
fn trash_rejects_protected_system_path_before_journaling() {
    let root = tempfile::tempdir().expect("temporary trash root");
    let journal = root.path().join("trash-journal.jsonl");

    let error = trash_delete(
        Path::new("/usr/disksage-must-not-trash.bin"),
        0,
        &journal,
        30_009,
    )
    .unwrap_err();

    assert!(matches!(error, SafetyError::Protected(_)));
    assert!(!journal.exists());
}
