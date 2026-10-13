#![cfg(unix)]

#[path = "../src/unix_capability_cleanup.rs"]
mod unix_capability_cleanup;

use std::fs::File;
use std::os::unix::fs::PermissionsExt;

#[test]
fn irreversible_partial_cleanup_retains_typed_filesystem_evidence() {
    assert_ne!(
        unsafe { libc::geteuid() },
        0,
        "partial-clean acceptance requires the unprivileged product execution boundary"
    );

    let temp = tempfile::tempdir().expect("temp root");
    let target = temp.path().join("target");
    let child = target.join("blocked-child");
    let artifact = child.join("artifact");
    std::fs::create_dir_all(&child).expect("child tree");
    std::fs::write(&artifact, vec![7u8; 4096]).expect("artifact");

    let reviewed_root = File::open(&target).expect("reviewed target capability");
    let original_mode = std::fs::metadata(&target)
        .expect("target metadata")
        .permissions()
        .mode();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o555))
        .expect("make retained root non-writable");

    let outcome = unix_capability_cleanup::remove_contents(&reviewed_root);

    std::fs::set_permissions(
        &target,
        std::fs::Permissions::from_mode(original_mode & 0o7777),
    )
    .expect("restore retained root permissions");

    let failure = outcome.expect_err("root disposition must fail after the child artifact unlink");
    assert!(
        !artifact.exists(),
        "fixture must prove irreversible mutation happened before the failure"
    );
    assert!(
        child.exists(),
        "failed root-relative directory removal must leave the child directory"
    );

    match &failure {
        unix_capability_cleanup::CleanupFailure::Partial {
            entries_removed,
            cause,
            target_view_allocated_bytes_before,
            target_view_allocated_bytes_after,
        } => {
            assert_eq!(*entries_removed, 1);
            assert!(!cause.is_empty());
            assert!(*target_view_allocated_bytes_before > 0);
            assert_eq!(
                *target_view_allocated_bytes_after,
                Some(0),
                "the remaining empty directory carries no regular-file allocation in the target view"
            );
            assert!(
                target_view_allocated_bytes_before
                    .saturating_sub(target_view_allocated_bytes_after.unwrap_or(0))
                    > 0,
                "typed filesystem evidence must retain the observed target-view reduction"
            );
        }
        other => panic!("irreversible mutation must return typed partial evidence, got {other:?}"),
    }
}
