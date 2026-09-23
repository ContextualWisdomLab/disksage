#![cfg(unix)]

#[path = "../src/unix_capability_cleanup.rs"]
mod unix_capability_cleanup;

use std::fs::File;
use std::os::unix::fs::PermissionsExt;

#[test]
fn irreversible_partial_cleanup_emits_machine_readable_evidence() {
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

    let error = outcome.expect_err("root disposition must fail after the child artifact unlink");
    assert!(
        !artifact.exists(),
        "fixture must prove irreversible mutation happened before the failure"
    );
    assert!(
        child.exists(),
        "failed root-relative directory removal must leave the child directory"
    );

    let evidence: serde_json::Value = serde_json::from_str(&error)
        .expect("partial-clean failure evidence must be machine-readable JSON");
    assert_eq!(evidence["schema_version"], 1);
    assert_eq!(evidence["code"], "cargo-target-partial-clean-failed");
    assert_eq!(evidence["completion"], "partial");
    assert_eq!(evidence["entries_removed"], 1);
    assert!(
        evidence["cause"].as_str().is_some_and(|cause| !cause.is_empty()),
        "partial-clean evidence must retain the causal filesystem failure"
    );
}
