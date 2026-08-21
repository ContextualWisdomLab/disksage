#![cfg(not(coverage))]

#[cfg(unix)]
#[test]
fn shared_writable_cloud_review_directory_fails_closed() {
    use disksage_lib::cloud_review::load_latest_decisions;
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().expect("temporary review directory");
    let mut permissions = std::fs::metadata(directory.path())
        .expect("review directory metadata")
        .permissions();
    permissions.set_mode(0o777);
    std::fs::set_permissions(directory.path(), permissions)
        .expect("make review directory shared-writable for regression");

    let error = load_latest_decisions(directory.path())
        .expect_err("shared-writable review authority must fail closed");

    assert_eq!(error, "cloud-review-directory-writable-by-others");
}
