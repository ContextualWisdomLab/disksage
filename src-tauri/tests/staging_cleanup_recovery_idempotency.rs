use disksage_lib::safety::{
    filesystem_object_id, journal_append, journal_recent, permanent_delete_dir_if_identity,
    JournalEntry,
};

#[test]
fn completed_cleanup_is_not_replayed_from_an_older_pending_receipt() {
    let fixture = tempfile::tempdir().expect("create recovery fixture");
    let source = fixture.path().join("already-mutated-target");
    let staging_dir = fixture.path().join(".disksage-trash-4242-7-0");
    std::fs::create_dir(&staging_dir).expect("create private staging directory");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&staging_dir, std::fs::Permissions::from_mode(0o700))
            .expect("set private staging mode");
    }

    let staging_object_id =
        filesystem_object_id(&staging_dir).expect("capture staging directory identity");
    let source_parent_object_id =
        filesystem_object_id(fixture.path()).expect("capture source-parent identity");
    let target_object_id = "reviewed-target-id";
    let journal = fixture.path().join("journal.jsonl");
    let recovery = serde_json::json!({
        "staging_name": ".disksage-trash-4242-7-0",
        "staging_object_id": staging_object_id,
        "source_parent_object_id": source_parent_object_id,
        "target_object_id": target_object_id,
        "catalog_root_object_id": null,
        "error": "simulated post-mutation cleanup failure"
    });

    journal_append(
        &journal,
        &JournalEntry {
            ts_ms: 1,
            op: "permanent_generated_directory_delete".into(),
            path: source.to_string_lossy().into_owned(),
            bytes: 0,
            outcome: format!("mutated_cleanup_pending:{recovery}"),
        },
    )
    .expect("persist cleanup-pending receipt");

    permanent_delete_dir_if_identity(&source, target_object_id, 0, &journal, 2)
        .expect("first retry must finish verified empty staging cleanup");
    assert!(!staging_dir.exists());

    let after_first_retry = journal_recent(&journal, usize::MAX);
    assert_eq!(after_first_retry.len(), 2);
    assert!(after_first_retry[0]
        .outcome
        .starts_with("mutated_cleanup_complete:"));

    permanent_delete_dir_if_identity(&source, target_object_id, 0, &journal, 3)
        .expect("completed recovery must remain idempotently complete");

    let after_second_retry = journal_recent(&journal, usize::MAX);
    assert_eq!(
        after_second_retry.len(),
        after_first_retry.len(),
        "a newer terminal receipt must suppress replay of an older cleanup-pending receipt"
    );
    assert!(after_second_retry[0]
        .outcome
        .starts_with("mutated_cleanup_complete:"));
}
