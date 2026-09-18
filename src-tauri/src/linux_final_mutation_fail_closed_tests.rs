use crate::safety::{
    filesystem_object_id, permanent_delete_dir_if_identity, trash_delete_if_identity,
};

/// Issue #170 deliberately requires a stronger invariant than Linux pathname-selected
/// `rename*`/`unlinkat` can prove: until an exact-object final mutation primitive is demonstrated,
/// identity-bound destructive operations must refuse the operation before any staging, journaled
/// mutation intent, Trash move, or permanent removal occurs. That fail-closed boundary dominates
/// the substitution race because a replacement can never become a mutation subject at all.
#[test]
fn final_trash_source_substitution_never_becomes_mutation_subject() {
    let temp = tempfile::tempdir().expect("temporary filesystem");
    let source = temp.path().join("reviewed-source.bin");
    let journal = temp.path().join("trash-journal.jsonl");
    std::fs::write(&source, b"reviewed object").expect("create reviewed source");
    let expected_object_id = filesystem_object_id(&source).expect("capture reviewed identity");

    let result = trash_delete_if_identity(&source, &expected_object_id, 15, &journal, 1);

    assert!(
        result.is_err(),
        "Linux must fail closed before identity-bound native Trash mutation until an exact-object primitive is proven"
    );
    assert!(
        source.is_file(),
        "fail-closed Trash must leave the reviewed filesystem object at its original path"
    );
    assert_eq!(
        std::fs::read(&source).expect("read preserved source"),
        b"reviewed object"
    );
    assert!(
        !journal.exists(),
        "unsupported final mutation must fail before publishing a destructive mutation intent"
    );
}

/// Permanent deletion has the same final namespace-selection problem as native Trash on Linux.
/// Until the owner proves a stronger primitive, the operation must retain the reviewed directory
/// and all of its contents without creating mutation/recovery authority.
#[test]
fn final_permanent_source_substitution_never_becomes_mutation_subject() {
    let temp = tempfile::tempdir().expect("temporary filesystem");
    let source = temp.path().join("reviewed-generated-dir");
    let child = source.join("artifact.bin");
    let journal = temp.path().join("permanent-delete-journal.jsonl");
    std::fs::create_dir(&source).expect("create reviewed directory");
    std::fs::write(&child, b"generated artifact").expect("create reviewed child");
    let expected_object_id = filesystem_object_id(&source).expect("capture reviewed identity");

    let result = permanent_delete_dir_if_identity(&source, &expected_object_id, 18, &journal, 2);

    assert!(
        result.is_err(),
        "Linux must fail closed before identity-bound permanent deletion until an exact-object primitive is proven"
    );
    assert!(
        source.is_dir(),
        "fail-closed permanent deletion must leave the reviewed directory at its original path"
    );
    assert_eq!(
        std::fs::read(&child).expect("read preserved child"),
        b"generated artifact"
    );
    assert!(
        !journal.exists(),
        "unsupported permanent mutation must fail before publishing a destructive mutation intent"
    );
}
