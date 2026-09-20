use crate::safety::{filesystem_object_id, journal_recent, move_file};

#[test]
fn reviewed_move_source_substitution_never_becomes_mutation_subject() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("reviewed-source.bin");
    let reviewed_source = tmp.path().join("reviewed-source-original.bin");
    let destination = tmp.path().join("organized-source.bin");
    let journal = tmp.path().join("journal.jsonl");

    std::fs::write(&source, b"reviewed-object").unwrap();
    let reviewed_object_id = filesystem_object_id(&source).unwrap();

    // Deterministically model a same-user pathname substitution after review. The previously
    // reviewed object remains available only as evidence; the pathname now names a distinct
    // object that was never authorized for movement or deletion.
    std::fs::rename(&source, &reviewed_source).unwrap();
    std::fs::write(&source, b"replacement-obj").unwrap();
    let replacement_object_id = filesystem_object_id(&source).unwrap();
    assert_ne!(replacement_object_id, reviewed_object_id);

    let result = move_file(&source, &destination, &journal, 1);

    assert!(
        result.is_err(),
        "move execution must fail closed when the reviewed pathname names a different filesystem object"
    );
    assert_eq!(
        filesystem_object_id(&source).unwrap(),
        replacement_object_id,
        "the unreviewed replacement must remain the source pathname target"
    );
    assert_eq!(std::fs::read(&source).unwrap(), b"replacement-obj");
    assert!(
        !destination.exists(),
        "no destination may be created from an unreviewed replacement"
    );
    assert_eq!(
        filesystem_object_id(&reviewed_source).unwrap(),
        reviewed_object_id,
        "the reviewed object retained for evidence must remain unchanged"
    );
    assert_eq!(std::fs::read(&reviewed_source).unwrap(), b"reviewed-object");
    assert!(
        journal_recent(&journal, usize::MAX).is_empty(),
        "a rejected replacement must not produce move/deletion authority receipts"
    );
}
