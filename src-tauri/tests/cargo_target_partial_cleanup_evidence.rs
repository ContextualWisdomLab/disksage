#![cfg(unix)]

#[path = "../src/unix_capability_cleanup.rs"]
mod unix_capability_cleanup;

use std::fs::File;
use std::os::unix::fs::PermissionsExt;

#[test]
fn irreversible_partial_cleanup_retains_typed_evidence_before_owner_serialization() {
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
        }
        other => panic!("irreversible mutation must return typed partial evidence, got {other:?}"),
    }

    let rendered = String::from(failure);
    let evidence: serde_json::Value = serde_json::from_str(&rendered)
        .expect("legacy owner boundary must still receive machine-readable JSON");
    assert_eq!(evidence["schema_version"], 1);
    assert_eq!(evidence["code"], "cargo-target-partial-clean-failed");
    assert_eq!(evidence["completion"], "partial");
    assert_eq!(evidence["entries_removed"], 1);
    assert!(evidence["target_view_allocated_bytes_before"].as_u64().is_some_and(|value| value > 0));
    assert_eq!(evidence["target_view_allocated_bytes_after"], 0);
    assert_eq!(
        evidence["observed_target_view_reduction_bytes"],
        evidence["target_view_allocated_bytes_before"]
    );
    assert_eq!(evidence["ledger_reclaim_bytes"], 0);
    assert!(
        evidence["cause"].as_str().is_some_and(|cause| !cause.is_empty()),
        "partial-clean evidence must retain the causal filesystem failure"
    );
}
