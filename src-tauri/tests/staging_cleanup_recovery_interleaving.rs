use disksage_lib::safety::{
    filesystem_object_id, journal_append, permanent_delete_dir_if_identity, JournalEntry,
};
use std::path::Path;

fn create_private_staging_dir(parent: &Path, name: &str) -> (String, String) {
    let staging_dir = parent.join(name);
    std::fs::create_dir(&staging_dir).expect("create private staging directory");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&staging_dir, std::fs::Permissions::from_mode(0o700))
            .expect("set private staging mode");
    }
    let object_id =
        filesystem_object_id(&staging_dir).expect("capture staging directory identity");
    (staging_dir.to_string_lossy().into_owned(), object_id)
}

fn pending_outcome(staging_name: &str, staging_object_id: &str, target_object_id: &str) -> String {
    let recovery = serde_json::json!({
        "staging_name": staging_name,
        "staging_object_id": staging_object_id,
        "target_object_id": target_object_id,
        "catalog_root_object_id": null,
        "error": "simulated post-mutation cleanup failure"
    });
    format!("mutated_cleanup_pending:{recovery}")
}

#[test]
fn a_bare_terminal_receipt_cannot_falsely_complete_a_distinct_newer_recovery() {
    let fixture = tempfile::tempdir().expect("create recovery fixture");
    let source = fixture.path().join("recreated-same-path");
    let journal = fixture.path().join("journal.jsonl");

    let (staging_a_path, staging_a_id) =
        create_private_staging_dir(fixture.path(), ".disksage-trash-4242-10-0");
    let (staging_b_path, staging_b_id) =
        create_private_staging_dir(fixture.path(), ".disksage-trash-4242-11-0");
    let staging_a = Path::new(&staging_a_path);
    let staging_b = Path::new(&staging_b_path);

    let target_a = "reviewed-target-a";
    let target_b = "reviewed-target-b";
    let op = "permanent_generated_directory_delete";
    let path = source.to_string_lossy().into_owned();

    journal_append(
        &journal,
        &JournalEntry {
            ts_ms: 1,
            op: op.into(),
            path: path.clone(),
            bytes: 0,
            outcome: pending_outcome(
                ".disksage-trash-4242-10-0",
                &staging_a_id,
                target_a,
            ),
        },
    )
    .expect("persist recovery A");
    journal_append(
        &journal,
        &JournalEntry {
            ts_ms: 2,
            op: op.into(),
            path: path.clone(),
            bytes: 0,
            outcome: pending_outcome(
                ".disksage-trash-4242-11-0",
                &staging_b_id,
                target_b,
            ),
        },
    )
    .expect("persist recovery B");

    // This represents an interleaving where retry A observed its pending receipt before B was
    // published, then completed after B. The legacy bare `ok` contains no recovery identity, so it
    // cannot safely authorize B merely because B's pending receipt is adjacent in the journal.
    journal_append(
        &journal,
        &JournalEntry {
            ts_ms: 3,
            op: op.into(),
            path,
            bytes: 0,
            outcome: "ok".into(),
        },
    )
    .expect("persist ambiguous legacy terminal receipt");

    let result = permanent_delete_dir_if_identity(&source, target_b, 0, &journal, 4);

    assert!(staging_a.exists(), "recovery A is unrelated to the B retry");
    assert!(
        result.is_err() || !staging_b.exists(),
        "a terminal receipt without recovery identity must not return success for B while B's staging directory is still present"
    );
}
