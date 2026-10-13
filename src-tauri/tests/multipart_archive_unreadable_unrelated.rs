#[path = "../src/bound_read_root.rs"]
mod bound_read_root;

mod duplicate_audit {
    pub(crate) use super::bound_read_root;
}

#[path = "../src/multipart_archive.rs"]
mod multipart_archive;

#[cfg(unix)]
#[test]
fn unreadable_unrelated_file_does_not_invalidate_split_archive_evidence() {
    use std::os::unix::fs::PermissionsExt;

    let root = tempfile::tempdir().expect("temporary root");
    std::fs::write(root.path().join("bundle.zip.part000"), b"part").expect("multipart part");
    let unrelated = root.path().join("unrelated.bin");
    std::fs::write(&unrelated, b"not a split archive").expect("unrelated file");
    std::fs::set_permissions(&unrelated, std::fs::Permissions::from_mode(0o000))
        .expect("make unrelated file unreadable");

    let report = multipart_archive::collect_multipart_archive_audit(root.path(), 10, 100)
        .expect("read-only multipart audit");

    std::fs::set_permissions(&unrelated, std::fs::Permissions::from_mode(0o600))
        .expect("restore cleanup permission");

    assert!(
        report.evidence_complete,
        "an unrelated non-part file must be skipped before read permission is required: {:?}",
        report.issue_counts
    );
    assert_eq!(report.set_count, 1);
    assert_eq!(report.part_count, 1);
}
