//! Coverage-visible cloud-local inventory error propagation and special-entry policy.
//!
//! The regressions use only temporary local roots, never open file content, never contact a cloud
//! provider, and never authorize eviction.

use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::cloud_local_inventory::{
    inventory_cloud_local_allocations, inventory_cloud_local_allocations_with_checkpoints,
    CloudLocalInventoryOptions,
};
use std::path::Path;

fn root(path: &Path) -> CloudRoot {
    CloudRoot {
        id: "icloud:checkpoint-failure-coverage".into(),
        provider: CloudProvider::Icloud,
        account_scope: CloudAccountScope::Personal,
        label: "iCloud checkpoint failure coverage".into(),
        path: path.to_string_lossy().into_owned(),
        readable: true,
        access_issue: None,
    }
}

fn options() -> CloudLocalInventoryOptions {
    CloudLocalInventoryOptions {
        min_allocated_bytes: 1,
        max_entries: 100,
        max_results: 10,
        max_depth: 4,
        max_duration_ms: 10_000,
        max_issues: 10,
    }
}

#[test]
fn checkpoint_callback_failure_propagates_before_inventory_claims_success() {
    let directory = tempfile::tempdir().unwrap();
    let error = inventory_cloud_local_allocations_with_checkpoints(
        &root(directory.path()),
        options(),
        123,
        |_| Err("coverage-checkpoint-sink-failed".into()),
    )
    .unwrap_err();
    assert_eq!(error, "coverage-checkpoint-sink-failed");
    assert!(directory.path().read_dir().unwrap().next().is_none());
}

#[cfg(unix)]
#[test]
fn unix_socket_entry_is_reported_as_unsupported_without_following_or_opening_it() {
    use std::os::unix::net::UnixListener;

    let directory = tempfile::tempdir().unwrap();
    let socket_path = directory.path().join("provider.sock");
    let listener = UnixListener::bind(&socket_path).unwrap();

    let report = inventory_cloud_local_allocations(&root(directory.path()), options(), 456).unwrap();
    assert_eq!(report.visited_entries, 1);
    assert_eq!(report.visited_files, 0);
    assert_eq!(report.skipped_entries, 1);
    assert!(report.candidates.is_empty());
    assert!(!report.evidence_complete);
    assert!(report
        .notices
        .contains(&"inventory-incomplete".to_string()));
    assert_eq!(report.issues.len(), 1);
    assert_eq!(report.issues[0].relative_scope.as_deref(), Some("provider.sock"));
    assert_eq!(report.issues[0].kind, "unsupported-entry-type");
    assert_eq!(report.issues[0].reason, "policy-not-file-or-directory");
    assert!(socket_path.exists());

    drop(listener);
}
