#![cfg(target_os = "macos")]

use disksage_lib::safety::{filesystem_object_id, journal_recent, trash_delete_if_identity};

#[test]
fn successful_identity_bound_trash_leaves_no_private_staging_directory() {
    let fixture = tempfile::tempdir().expect("create filesystem fixture");
    let victim_name = format!(
        "disksage-macos-staging-recovery-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock after unix epoch")
            .as_nanos()
    );
    let victim = fixture.path().join(&victim_name);
    std::fs::create_dir(&victim).expect("create reviewed directory");
    std::fs::write(victim.join("payload.bin"), b"reviewed-object")
        .expect("write reviewed payload");

    let expected_object_id = filesystem_object_id(&victim).expect("capture reviewed object identity");
    let journal = fixture.path().join("journal.jsonl");

    trash_delete_if_identity(&victim, &expected_object_id, 15, &journal, 1)
        .expect("identity-bound macOS Trash mutation should succeed");

    assert!(!victim.exists(), "the reviewed object must have moved to macOS Trash");
    let trashed: Vec<_> = trash::os_limited::list()
        .expect("list macOS Trash")
        .into_iter()
        .filter(|item| item.name.to_string_lossy() == victim_name)
        .collect();
    assert_eq!(
        trashed.len(),
        1,
        "the reviewed object must be present exactly once in macOS Trash"
    );
    trash::os_limited::purge_all(trashed).expect("purge ephemeral macOS Trash fixture");

    assert!(
        std::fs::read_dir(fixture.path())
            .expect("read fixture parent")
            .all(|entry| !entry
                .expect("read fixture entry")
                .file_name()
                .to_string_lossy()
                .starts_with(".disksage-trash-")),
        "successful macOS Trash must not leave a DiskSage staging directory"
    );

    let entries = journal_recent(&journal, 2);
    assert_eq!(entries.len(), 2, "pending and terminal receipts are required");
    assert_eq!(entries[0].outcome, "ok");
    assert_eq!(entries[1].outcome, "pending");
}
