//! Focused exact-head coverage for safety branches that remain buyer-critical.
//!
//! These regressions stay at shipped public boundaries: durable journal appends must not create
//! phantom blank records, crash-truncated tails must self-heal without losing audit lineage,
//! malformed records must not become evidence, object identity lookup must fail closed when the
//! object is absent, and a symlink into a protected system tree must never become a trash target
//! merely because the caller supplied a local-looking pathname.

use crate::safety::{
    filesystem_object_id, journal_append, journal_recent, trash_delete, JournalEntry, SafetyError,
};

fn journal_entry(ts_ms: u64, path: &str, outcome: &str) -> JournalEntry {
    JournalEntry {
        ts_ms,
        op: "move_file".into(),
        path: path.into(),
        bytes: ts_ms % 100,
        outcome: outcome.into(),
    }
}

#[test]
fn journal_append_to_complete_record_does_not_insert_a_blank_audit_line() {
    let root = tempfile::tempdir().expect("temporary journal root");
    let journal = root.path().join("safety-journal.jsonl");
    let first = journal_entry(50_001, "/tmp/first-reviewed-source.bin", "pending");
    let second = journal_entry(50_002, "/tmp/second-reviewed-source.bin", "ok");

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

#[test]
fn journal_append_repairs_unterminated_valid_tail_before_next_record() {
    let root = tempfile::tempdir().expect("temporary journal root");
    let journal = root.path().join("crash-tail-journal.jsonl");
    let interrupted = journal_entry(50_004, "/tmp/interrupted-source.bin", "pending");
    let next = journal_entry(50_005, "/tmp/next-source.bin", "ok");
    let interrupted_json = serde_json::to_string(&interrupted).expect("serialize crash-tail fixture");
    std::fs::write(&journal, interrupted_json.as_bytes()).expect("write unterminated crash tail");

    journal_append(&journal, &next).expect("append must heal a missing trailing newline");

    let raw = std::fs::read_to_string(&journal).expect("read healed journal");
    assert!(raw.ends_with('\n'));
    assert_eq!(raw.lines().count(), 2, "healing must preserve both audit records");
    let recent = journal_recent(&journal, 10);
    assert_eq!(recent.len(), 2);
    assert_eq!(recent[0].ts_ms, next.ts_ms);
    assert_eq!(recent[1].ts_ms, interrupted.ts_ms);
}

#[test]
fn journal_recent_ignores_malformed_lines_instead_of_promoting_them_to_evidence() {
    let root = tempfile::tempdir().expect("temporary journal root");
    let journal = root.path().join("mixed-journal.jsonl");
    let valid = journal_entry(50_006, "/tmp/verified-source.bin", "ok");
    let valid_json = serde_json::to_string(&valid).expect("serialize valid audit fixture");
    std::fs::write(
        &journal,
        format!("not-json\n{valid_json}\n{{truncated\n"),
    )
    .expect("write mixed audit fixture");

    let recent = journal_recent(&journal, 10);

    assert_eq!(recent.len(), 1, "malformed lines are not authoritative audit evidence");
    assert_eq!(recent[0].ts_ms, valid.ts_ms);
    assert_eq!(recent[0].path, valid.path);
}

#[test]
fn filesystem_object_identity_fails_closed_when_reviewed_object_is_absent() {
    let root = tempfile::tempdir().expect("temporary identity root");
    let absent = root.path().join("object-removed-after-review");

    let error = filesystem_object_id(&absent).expect_err("missing object cannot have stable identity");

    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    assert!(!absent.exists());
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
