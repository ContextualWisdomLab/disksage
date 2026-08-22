//! Permission-boundary regression for cloud source-root admission.
//!
//! This test uses a temporary local directory only. It does not discover cloud accounts, invoke
//! provider APIs, mutate user data, or require credentials.

use disksage_lib::cloud::validate_source_root_readable;

#[cfg(unix)]
#[test]
fn source_root_rejects_directory_that_cannot_be_enumerated() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().expect("temporary fixture");
    let source_root = temp.path().join("permission-denied-source");
    std::fs::create_dir(&source_root).expect("source fixture directory");

    let mut denied = std::fs::metadata(&source_root)
        .expect("source metadata")
        .permissions();
    denied.set_mode(0o000);
    std::fs::set_permissions(&source_root, denied).expect("restrict source fixture");

    let result = validate_source_root_readable(&source_root);

    // Restore traversal before TempDir cleanup even when the assertion below fails.
    let mut restored = std::fs::metadata(&source_root)
        .expect("restricted source metadata")
        .permissions();
    restored.set_mode(0o700);
    std::fs::set_permissions(&source_root, restored).expect("restore source fixture");

    let error = result.expect_err("an unreadable source directory must fail closed");
    assert!(
        error.starts_with("source-root-unreadable:"),
        "unexpected bounded diagnostic: {error}"
    );
}
