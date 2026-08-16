//! Crash-recovery and fail-closed coverage for safety boundaries without widening their API.

use crate::safety::{
    filesystem_object_id, is_protected, journal_append, journal_recent, object_id_from_metadata,
    same_volume, trash_delete, trash_delete_if_identity, JournalEntry, SafetyError,
};
use std::path::Path;

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

#[test]
fn recent_journal_ignores_malformed_records_and_honors_zero_and_bounded_limits() {
    let root = tempfile::tempdir().unwrap();
    let journal = root.path().join("cleanup-journal.jsonl");
    let older = JournalEntry {
        ts_ms: 1,
        op: "trash_delete".into(),
        path: "/tmp/older".into(),
        bytes: 1,
        outcome: "pending".into(),
    };
    let newer = JournalEntry {
        ts_ms: 2,
        op: "trash_delete".into(),
        path: "/tmp/newer".into(),
        bytes: 2,
        outcome: "ok".into(),
    };
    std::fs::write(
        &journal,
        format!(
            "{}\nnot-json\n{}\n",
            serde_json::to_string(&older).unwrap(),
            serde_json::to_string(&newer).unwrap()
        ),
    )
    .unwrap();

    assert!(journal_recent(&journal, 0).is_empty());
    let one = journal_recent(&journal, 1);
    assert_eq!(one.len(), 1);
    assert_eq!(one[0].ts_ms, newer.ts_ms);
    let all = journal_recent(&journal, 10);
    assert_eq!(all.iter().map(|entry| entry.ts_ms).collect::<Vec<_>>(), vec![2, 1]);
    assert!(journal_recent(&root.path().join("missing.jsonl"), 10).is_empty());
}

#[test]
fn journal_open_failures_are_typed_and_displayed_without_panicking() {
    let root = tempfile::tempdir().unwrap();
    let entry = JournalEntry {
        ts_ms: 3,
        op: "trash_delete".into(),
        path: "/tmp/example".into(),
        bytes: 3,
        outcome: "pending".into(),
    };
    let error = journal_append(root.path(), &entry).unwrap_err();
    match &error {
        SafetyError::Journal(message) => assert!(!message.is_empty()),
        other => panic!("expected journal error, received {other}"),
    }

    let protected = SafetyError::Protected(root.path().to_path_buf());
    assert!(protected.to_string().contains(root.path().to_string_lossy().as_ref()));
    assert!(SafetyError::Trash("bounded".into()).to_string().contains("bounded"));
    assert!(!error.to_string().is_empty());
}

#[test]
fn trash_delete_rejects_parent_traversal_before_journal_or_mutation() {
    let root = tempfile::tempdir().unwrap();
    let fixture = root.path().join("fixture");
    std::fs::write(&fixture, b"keep").unwrap();
    let traversal = root.path().join("child").join("..").join("fixture");
    let journal = root.path().join("cleanup-journal.jsonl");

    let error = trash_delete(&traversal, 4, &journal, 10).unwrap_err();

    assert!(matches!(error, SafetyError::Protected(_)));
    assert_eq!(std::fs::read(&fixture).unwrap(), b"keep");
    assert!(!journal.exists());
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn trash_delete_success_records_pending_then_success_and_is_recoverable() {
    let root = tempfile::tempdir().unwrap();
    let unique = root.path().file_name().unwrap().to_string_lossy();
    let fixture_name = format!("disksage-reversible-trash-{unique}.bin");
    let fixture = root.path().join(&fixture_name);
    let journal = root.path().join("cleanup-journal.jsonl");
    std::fs::write(&fixture, b"reversible").unwrap();

    trash_delete(&fixture, b"reversible".len() as u64, &journal, 10_001).unwrap();

    assert!(!fixture.exists(), "successful trash must vacate the source pathname");
    let recent = journal_recent(&journal, 10);
    assert_eq!(recent.len(), 2);
    assert_eq!(recent[0].outcome, "ok");
    assert_eq!(recent[1].outcome, "pending");
    assert_eq!(recent[0].path, fixture.to_string_lossy());
    assert_eq!(recent[1].path, fixture.to_string_lossy());

    let items: Vec<_> = trash::os_limited::list()
        .unwrap()
        .into_iter()
        .filter(|item| item.name.to_string_lossy().as_ref() == fixture_name.as_str())
        .collect();
    assert_eq!(items.len(), 1, "the trashed fixture must remain reversibly identifiable");
    trash::os_limited::purge_all(items).unwrap();
}

#[test]
fn identity_bound_trash_rejects_parent_traversal_before_identity_or_journal_work() {
    let root = tempfile::tempdir().unwrap();
    let fixture = root.path().join("fixture");
    std::fs::write(&fixture, b"keep").unwrap();
    let expected_identity = filesystem_object_id(&fixture).unwrap();
    let traversal = root.path().join("child").join("..").join("fixture");
    let journal = root.path().join("cleanup-journal.jsonl");

    let error =
        trash_delete_if_identity(&traversal, &expected_identity, 4, &journal, 11).unwrap_err();

    assert!(matches!(error, SafetyError::Protected(_)));
    assert_eq!(std::fs::read(&fixture).unwrap(), b"keep");
    assert!(!journal.exists());
}

#[test]
fn identity_bound_trash_rejects_stale_identity_before_journal_or_staging() {
    let root = tempfile::tempdir().unwrap();
    let fixture = root.path().join("fixture");
    std::fs::write(&fixture, b"keep").unwrap();
    let journal = root.path().join("cleanup-journal.jsonl");
    let current_identity = filesystem_object_id(&fixture).unwrap();
    let stale_identity = format!("{current_identity}-stale");

    let error = trash_delete_if_identity(&fixture, &stale_identity, 4, &journal, 12).unwrap_err();

    assert!(matches!(error, SafetyError::Trash(_)));
    assert_eq!(std::fs::read(&fixture).unwrap(), b"keep");
    assert!(!journal.exists());
    let staging_prefix = format!(".disksage-trash-{}-12-", std::process::id());
    assert!(!std::fs::read_dir(root.path())
        .unwrap()
        .filter_map(Result::ok)
        .any(|entry| entry.file_name().to_string_lossy().starts_with(&staging_prefix)));
}

#[test]
fn identity_bound_trash_cleans_staging_when_journal_open_fails() {
    let root = tempfile::tempdir().unwrap();
    let fixture = root.path().join("fixture");
    std::fs::write(&fixture, b"keep").unwrap();
    let expected_identity = filesystem_object_id(&fixture).unwrap();
    let staging_prefix = format!(".disksage-trash-{}-13-", std::process::id());

    let error =
        trash_delete_if_identity(&fixture, &expected_identity, 4, root.path(), 13).unwrap_err();

    assert!(matches!(error, SafetyError::Journal(_)));
    assert_eq!(std::fs::read(&fixture).unwrap(), b"keep");
    assert!(!std::fs::read_dir(root.path())
        .unwrap()
        .filter_map(Result::ok)
        .any(|entry| entry.file_name().to_string_lossy().starts_with(&staging_prefix)));
}

#[test]
fn identity_bound_trash_rejects_missing_source_before_journal_or_staging() {
    let root = tempfile::tempdir().unwrap();
    let missing = root.path().join("missing-fixture");
    let journal = root.path().join("cleanup-journal.jsonl");
    let staging_prefix = format!(".disksage-trash-{}-14-", std::process::id());

    let error = trash_delete_if_identity(&missing, "missing-object", 4, &journal, 14).unwrap_err();

    assert!(matches!(error, SafetyError::Trash(_)));
    assert!(!journal.exists());
    assert!(!std::fs::read_dir(root.path())
        .unwrap()
        .filter_map(Result::ok)
        .any(|entry| entry.file_name().to_string_lossy().starts_with(&staging_prefix)));
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn identity_bound_trash_success_stages_exact_object_and_journals_success() {
    let root = tempfile::tempdir().unwrap();
    let unique = root.path().file_name().unwrap().to_string_lossy();
    let fixture_name = format!("disksage-identity-bound-{unique}.bin");
    let fixture = root.path().join(&fixture_name);
    let journal = root.path().join("cleanup-journal.jsonl");
    std::fs::write(&fixture, b"reviewed-object").unwrap();
    let expected_identity = filesystem_object_id(&fixture).unwrap();
    let staging_prefix = format!(".disksage-trash-{}-15-", std::process::id());

    trash_delete_if_identity(
        &fixture,
        &expected_identity,
        b"reviewed-object".len() as u64,
        &journal,
        15,
    )
    .unwrap();

    assert!(!fixture.exists(), "reviewed source pathname must be vacated");
    let recent = journal_recent(&journal, 10);
    assert_eq!(recent.len(), 2);
    assert_eq!(recent[0].outcome, "ok");
    assert_eq!(recent[1].outcome, "pending");

    let staging_dirs: Vec<_> = std::fs::read_dir(root.path())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with(&staging_prefix))
        .collect();
    assert_eq!(staging_dirs.len(), 1, "successful identity trash keeps one undo parent");
    assert_eq!(
        std::fs::read_dir(staging_dirs[0].path()).unwrap().count(),
        0,
        "staging directory must be empty after OS trash accepts the object"
    );

    let items: Vec<_> = trash::os_limited::list()
        .unwrap()
        .into_iter()
        .filter(|item| item.name.to_string_lossy().as_ref() == fixture_name.as_str())
        .collect();
    assert_eq!(items.len(), 1, "the exact staged object must be recoverable from trash");
    trash::os_limited::purge_all(items).unwrap();
    std::fs::remove_dir(staging_dirs[0].path()).unwrap();
}

#[cfg(unix)]
#[test]
fn unix_home_root_is_protected_while_home_descendants_remain_eligible() {
    let home = std::env::var("HOME").expect("Unix desktop test runner must define HOME");
    let home_path = Path::new(&home);

    assert!(is_protected(home_path));
    assert!(!is_protected(
        &home_path.join("disksage-home-descendant-coverage")
    ));
}

#[cfg(unix)]
#[test]
fn unix_protection_and_object_identity_cover_root_system_and_local_objects() {
    assert!(is_protected(Path::new("/")));
    assert!(is_protected(Path::new("/usr")));
    assert!(is_protected(Path::new("/usr/local/share")));

    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("identity-fixture");
    std::fs::write(&file, b"identity").unwrap();
    assert!(!is_protected(&file));

    let metadata = std::fs::symlink_metadata(&file).unwrap();
    let from_metadata = object_id_from_metadata(&metadata).expect("Unix metadata has identity");
    let from_path = filesystem_object_id(&file).unwrap();
    assert_eq!(from_metadata, from_path);
    assert!(from_path.starts_with("unix:"));

    let missing = root.path().join("missing");
    assert!(filesystem_object_id(&missing).is_err());
}

#[cfg(unix)]
#[test]
fn unix_same_volume_accepts_local_destination_parent_and_rejects_missing_metadata() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let destination_parent = root.path().join("nested");
    let destination = destination_parent.join("destination");
    std::fs::write(&source, b"source").unwrap();
    std::fs::create_dir(&destination_parent).unwrap();

    assert!(same_volume(&source, &destination));
    assert!(!same_volume(&root.path().join("missing"), &destination));
    assert!(!same_volume(
        &source,
        &root.path().join("missing-parent").join("destination")
    ));
}
