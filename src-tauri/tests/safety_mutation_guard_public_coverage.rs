//! Public-contract coverage for destructive-operation admission guards.
//!
//! Every test stops before an OS trash or move mutation can occur. Temporary fixtures are used to
//! prove fail-closed behavior for lexical traversal, stale object identity, destination collision,
//! and volume probing.

use disksage_lib::safety::{
    move_file, same_volume, trash_delete, trash_delete_if_identity, SafetyError,
};
use std::path::Path;

#[test]
fn trash_delete_rejects_parent_traversal_and_filesystem_root_before_journaling() {
    let directory = tempfile::tempdir().expect("create temporary directory");
    let journal_path = directory.path().join("journal.jsonl");
    let traversing = directory.path().join("child").join("..").join("victim.txt");

    assert!(matches!(
        trash_delete(&traversing, 1, &journal_path, 1),
        Err(SafetyError::Protected(_))
    ));

    #[cfg(unix)]
    assert!(matches!(
        trash_delete(Path::new("/"), 1, &journal_path, 2),
        Err(SafetyError::Protected(_))
    ));

    assert!(!journal_path.exists());
}

#[test]
fn identity_bound_trash_rejects_stale_identity_without_staging_or_journal() {
    let directory = tempfile::tempdir().expect("create temporary directory");
    let file_path = directory.path().join("reviewed.txt");
    let journal_path = directory.path().join("journal.jsonl");
    std::fs::write(&file_path, b"preserve me").expect("write temporary fixture");

    let error = trash_delete_if_identity(
        &file_path,
        "stale-object-identity",
        11,
        &journal_path,
        3,
    )
    .expect_err("stale identity must fail closed");

    assert!(matches!(error, SafetyError::Trash(_)));
    assert_eq!(
        std::fs::read(&file_path).expect("fixture must remain"),
        b"preserve me"
    );
    assert!(!journal_path.exists());
    let names: Vec<_> = std::fs::read_dir(directory.path())
        .expect("list temporary directory")
        .map(|entry| entry.expect("directory entry").file_name())
        .collect();
    assert!(!names.iter().any(|name| name.to_string_lossy().starts_with(".disksage-trash-")));
}

#[test]
fn move_rejects_parent_traversal_and_existing_destination_without_mutation() {
    let directory = tempfile::tempdir().expect("create temporary directory");
    let src = directory.path().join("source.txt");
    let dst = directory.path().join("destination.txt");
    let journal_path = directory.path().join("journal.jsonl");
    std::fs::write(&src, b"source").expect("write source fixture");
    std::fs::write(&dst, b"destination").expect("write destination fixture");

    let traversing_dst = directory.path().join("child").join("..").join("new.txt");
    assert!(matches!(
        move_file(&src, &traversing_dst, &journal_path, 4),
        Err(SafetyError::Protected(_))
    ));
    assert!(matches!(
        move_file(&src, &dst, &journal_path, 5),
        Err(SafetyError::Trash(_))
    ));

    assert_eq!(std::fs::read(&src).expect("source must remain"), b"source");
    assert_eq!(
        std::fs::read(&dst).expect("destination must remain"),
        b"destination"
    );
    assert!(!journal_path.exists());
}

#[test]
fn same_volume_reports_same_temporary_filesystem_and_fails_closed_for_missing_source() {
    let directory = tempfile::tempdir().expect("create temporary directory");
    let src = directory.path().join("source.txt");
    let dst = directory.path().join("nested").join("destination.txt");
    std::fs::write(&src, b"source").expect("write source fixture");
    std::fs::create_dir_all(dst.parent().expect("destination parent"))
        .expect("create destination parent");

    assert!(same_volume(&src, &dst));
    assert!(!same_volume(&directory.path().join("missing-source"), &dst));
}
