//! Focused exact-head coverage for safety branches that remain buyer-critical.
//!
//! These regressions stay at shipped public boundaries: durable journal appends must not create
//! phantom blank records, and a symlink into a protected system tree must never become a trash
//! target merely because the caller supplied a local-looking pathname.

use crate::safety::{
    filesystem_object_id, journal_append, journal_recent, trash_delete, trash_delete_if_identity,
    JournalEntry, SafetyError,
};

#[test]
fn journal_append_to_complete_record_does_not_insert_a_blank_audit_line() {
    let root = tempfile::tempdir().expect("temporary journal root");
    let journal = root.path().join("safety-journal.jsonl");
    let first = JournalEntry {
        ts_ms: 50_001,
        op: "move_file".into(),
        path: "/tmp/first-reviewed-source.bin".into(),
        bytes: 11,
        outcome: "pending".into(),
    };
    let second = JournalEntry {
        ts_ms: 50_002,
        op: "move_file".into(),
        path: "/tmp/second-reviewed-source.bin".into(),
        bytes: 22,
        outcome: "ok".into(),
    };

    journal_append(&journal, &first).expect("write first complete journal record");
    journal_append(&journal, &second).expect("append after newline-terminated record");

    let raw = std::fs::read_to_string(&journal).expect("read complete journal");
    let lines: Vec<_> = raw.lines().collect();
    assert_eq!(lines.len(), 2, "a complete tail must not gain an empty audit record");
    assert!(lines.iter().all(|line| !line.is_empty()));
    let recent = journal_recent(&journal, 10);
    assert_eq!(recent.len(), 2);
    assert_eq!(recent[0].ts_ms, second.ts_ms);
    assert_eq!(recent[1].ts_ms, first.ts_ms);
}

#[cfg(unix)]
#[test]
fn trash_rejects_local_symlink_that_resolves_into_protected_system_tree() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().expect("temporary trash root");
    let protected_alias = root.path().join("protected-trash-source");
    let journal = root.path().join("trash-journal.jsonl");
    symlink("/usr/bin", &protected_alias).expect("create protected-system symlink fixture");

    let error = trash_delete(&protected_alias, 0, &journal, 50_003).unwrap_err();

    assert!(matches!(error, SafetyError::Protected(_)));
    assert!(
        std::fs::symlink_metadata(&protected_alias)
            .expect("caller-owned symlink must remain")
            .file_type()
            .is_symlink()
    );
    assert!(
        !journal.exists(),
        "protected symlink aliases must fail before mutation journaling"
    );
}

#[cfg(unix)]
#[test]
fn identity_bound_trash_rejects_protected_symlink_before_identity_or_journal_work() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().expect("temporary identity-trash root");
    let protected_alias = root.path().join("protected-identity-source");
    let journal = root.path().join("identity-trash-journal.jsonl");
    symlink("/usr/bin", &protected_alias).expect("create protected-system symlink fixture");
    let expected_identity = filesystem_object_id(&protected_alias)
        .expect("caller-supplied symlink has a stable filesystem identity");

    let error = trash_delete_if_identity(
        &protected_alias,
        &expected_identity,
        0,
        &journal,
        50_004,
    )
    .unwrap_err();

    assert!(matches!(error, SafetyError::Protected(_)));
    assert!(
        std::fs::symlink_metadata(&protected_alias)
            .expect("caller-owned symlink must remain")
            .file_type()
            .is_symlink()
    );
    assert!(
        !journal.exists(),
        "protected identity-bound aliases must fail before identity mutation journaling"
    );
}

#[cfg(unix)]
#[test]
fn identity_bound_trash_fails_closed_when_private_staging_namespace_is_exhausted() {
    let root = tempfile::tempdir().expect("temporary identity-trash root");
    let reviewed_source = root.path().join("reviewed-source.bin");
    let journal = root.path().join("identity-trash-journal.jsonl");
    let payload = b"reviewed-source-must-survive";
    std::fs::write(&reviewed_source, payload).expect("write reviewed source fixture");
    let expected_identity = filesystem_object_id(&reviewed_source)
        .expect("reviewed source has a stable filesystem identity");
    let now_ms = 50_005;
    let pid = std::process::id();

    // The production allocator gives up after 32 create-only collisions. Pre-create a deliberately
    // generous serial window for this unique timestamp so the test remains deterministic even if
    // earlier identity-trash tests have advanced the process-global staging counter.
    for serial in 0..512u64 {
        std::fs::create_dir(root.path().join(format!(
            ".disksage-trash-{pid}-{now_ms}-{serial}"
        )))
        .expect("reserve staging collision fixture");
    }

    let error = trash_delete_if_identity(
        &reviewed_source,
        &expected_identity,
        payload.len() as u64,
        &journal,
        now_ms,
    )
    .unwrap_err();

    assert!(matches!(error, SafetyError::Trash(_)));
    assert_eq!(
        std::fs::read(&reviewed_source).expect("reviewed source remains readable"),
        payload
    );
    assert!(
        !journal.exists(),
        "staging namespace exhaustion must fail before mutation journaling"
    );
}
