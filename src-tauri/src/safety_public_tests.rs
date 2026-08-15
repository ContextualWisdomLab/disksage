//! Crash-recovery coverage for the destructive-operation audit journal without widening its API.

use crate::safety::{journal_append, journal_recent, JournalEntry};

#[test]
fn append_repairs_missing_trailing_newline_without_merging_audit_records() {
    let root = tempfile::tempdir().unwrap();
    let journal = root.path().join("cleanup-journal.jsonl");
    let first = JournalEntry {
        ts_ms: 101,
        op: "trash_delete".into(),
        path: "/tmp/first".into(),
        bytes: 11,
        outcome: "pending".into(),
    };
    let second = JournalEntry {
        ts_ms: 202,
        op: "trash_delete".into(),
        path: "/tmp/second".into(),
        bytes: 22,
        outcome: "ok".into(),
    };

    let first_json = serde_json::to_string(&first).unwrap();
    std::fs::write(&journal, first_json.as_bytes()).unwrap();
    assert!(!std::fs::read(&journal).unwrap().ends_with(b"\n"));

    journal_append(&journal, &second).unwrap();

    let repaired = std::fs::read_to_string(&journal).unwrap();
    assert!(repaired.ends_with('\n'));
    let lines: Vec<_> = repaired.lines().collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0], first_json);
    assert_eq!(serde_json::from_str::<serde_json::Value>(lines[1]).unwrap()["ts_ms"], 202);

    let recent = journal_recent(&journal, 10);
    assert_eq!(recent.len(), 2);
    assert_eq!(recent[0].ts_ms, second.ts_ms);
    assert_eq!(recent[0].outcome, second.outcome);
    assert_eq!(recent[1].ts_ms, first.ts_ms);
    assert_eq!(recent[1].outcome, first.outcome);
}
