//! Executable contract for private-record replacement mutation authority.
//!
//! Revalidating a staging pathname and then calling `renameat` leaves a final namespace interval in
//! which a same-UID process can replace that pathname. Until DiskSage has an accepted primitive that
//! conditions final publication on the reviewed source object, replacement must fail before creating
//! a staging object or changing the existing record.

const SOURCE: &str = include_str!("../src/object_bound_publication.rs");

#[test]
fn private_replacement_does_not_publish_through_raw_source_name_renameat() {
    assert!(
        !SOURCE.contains("libc::renameat("),
        "private replacement must not publish through a source-name-only rename"
    );
    assert!(
        SOURCE.contains("ObjectBoundReplaceError::SourceIdentityUnavailable"),
        "the unsupported source-identity boundary must remain explicit and fail closed"
    );
}

#[cfg(unix)]
#[path = "../src/object_bound_publication.rs"]
mod object_bound_publication;

#[cfg(unix)]
#[test]
fn valid_private_replacement_preserves_existing_bytes_and_creates_no_staging_name() {
    use std::os::unix::fs::PermissionsExt;

    let root = tempfile::tempdir().expect("tempdir");
    let parent = root.path().join("private");
    std::fs::create_dir(&parent).expect("create private parent");
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700))
        .expect("set private parent mode");
    let record = parent.join("connections.json");
    std::fs::write(&record, b"reviewed-old").expect("seed record");

    let error = object_bound_publication::replace_object_bound_bytes(
        &record,
        b"unpublishable-new",
        0o600,
    )
    .expect_err("source-name-only replacement must remain unavailable");

    assert_eq!(
        error.code(),
        "object-bound-replace-source-identity-unavailable"
    );
    assert_eq!(
        std::fs::read(&record).expect("read preserved record"),
        b"reviewed-old"
    );
    let names = std::fs::read_dir(&parent)
        .expect("read private parent")
        .map(|entry| entry.expect("entry").file_name())
        .collect::<Vec<_>>();
    assert_eq!(names, vec![std::ffi::OsString::from("connections.json")]);
}
