//! Public-contract coverage for safety identity and journal recovery boundaries.
//!
//! These tests exercise the production APIs with temporary local filesystem state only. They do
//! not invoke trash deletion and therefore cannot remove user data.

use disksage_lib::safety::{
    filesystem_object_id, journal_append, journal_recent, JournalEntry, SafetyError,
};

#[cfg(unix)]
use disksage_lib::safety::object_id_from_metadata;

fn entry(ts_ms: u64, outcome: &str) -> JournalEntry {
    JournalEntry {
        ts_ms,
        op: "coverage-test".into(),
        path: "/temporary/coverage-test".into(),
        bytes: 7,
        outcome: outcome.into(),
    }
}

#[cfg(unix)]
#[test]
fn filesystem_identity_matches_symlink_metadata_identity_and_missing_path_fails() {
    let directory = tempfile::tempdir().expect("create temporary directory");
    let file_path = directory.path().join("identity.txt");
    std::fs::write(&file_path, b"identity").expect("write temporary file");

    let metadata = std::fs::symlink_metadata(&file_path).expect("read temporary metadata");
    let metadata_identity = object_id_from_metadata(&metadata).expect("unix metadata identity");
    let filesystem_identity = filesystem_object_id(&file_path).expect("filesystem identity");

    assert!(metadata_identity.starts_with("unix:"));
    assert_eq!(filesystem_identity, metadata_identity);

    let missing = directory.path().join("missing.txt");
    assert_eq!(
        filesystem_object_id(&missing).unwrap_err().kind(),
        std::io::ErrorKind::NotFound
    );
}

#[test]
fn journal_append_heals_partial_tail_and_recent_returns_newest_valid_entries() {
    let directory = tempfile::tempdir().expect("create temporary directory");
    let journal_path = directory.path().join("safety.journal");

    std::fs::write(&journal_path, b"{\"truncated\":true").expect("seed partial journal tail");
    journal_append(&journal_path, &entry(10, "first")).expect("append first entry");
    journal_append(&journal_path, &entry(20, "second")).expect("append second entry");

    let all = journal_recent(&journal_path, 10);
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].ts_ms, 20);
    assert_eq!(all[0].outcome, "second");
    assert_eq!(all[1].ts_ms, 10);

    let newest = journal_recent(&journal_path, 1);
    assert_eq!(newest.len(), 1);
    assert_eq!(newest[0].ts_ms, 20);
}

#[test]
fn journal_missing_file_is_empty_and_uncreatable_parent_fails_closed() {
    let directory = tempfile::tempdir().expect("create temporary directory");
    let missing_journal = directory.path().join("missing.journal");
    assert!(journal_recent(&missing_journal, 5).is_empty());

    let impossible_journal = directory.path().join("missing-parent").join("journal.log");
    let error = journal_append(&impossible_journal, &entry(30, "never-written"))
        .expect_err("missing parent must fail closed");
    assert!(matches!(error, SafetyError::Journal(_)));
}
