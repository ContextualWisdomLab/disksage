#![cfg(unix)]

#[path = "../src/object_bound_publication.rs"]
mod object_bound_publication;

use std::os::unix::fs::PermissionsExt;

#[test]
fn private_record_replacement_rejects_group_or_other_permissions_before_mutation() {
    let root = tempfile::tempdir().expect("tempdir");
    let parent = root.path().join("private");
    std::fs::create_dir(&parent).expect("create private parent");
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700))
        .expect("set private parent mode");
    let record = parent.join("connections.json");

    let error = object_bound_publication::replace_object_bound_bytes(&record, b"private", 0o644)
        .expect_err("private publication must reject group/other-readable record modes");

    assert_eq!(
        error.code(),
        "object-bound-replace-mode-invalid",
        "the reusable private-publication foundation must fail closed on a non-private mode"
    );
    assert!(
        !record.exists(),
        "mode admission must fail before any final record is published"
    );
    assert_eq!(
        std::fs::read_dir(&parent).expect("read private parent").count(),
        0,
        "mode admission must fail before any staging record is created"
    );
}
