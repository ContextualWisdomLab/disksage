//! Unix-only public-contract coverage for fail-closed local cloud inventory permission errors.
//!
//! The regression uses only a temporary local filesystem tree. It does not contact a provider,
//! inspect file contents through DiskSage, or authorize any cleanup mutation.

#![cfg(unix)]

use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::cloud_local_inventory::{
    inventory_cloud_local_allocations, CloudLocalInventoryOptions,
};
use std::os::unix::fs::PermissionsExt;

fn root(path: &std::path::Path) -> CloudRoot {
    CloudRoot {
        id: "icloud:permission-coverage".into(),
        provider: CloudProvider::Icloud,
        account_scope: CloudAccountScope::Personal,
        label: "iCloud permission coverage".into(),
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
fn unreadable_child_is_reported_without_inventing_complete_evidence() {
    let temp = tempfile::tempdir().unwrap();
    let blocked = temp.path().join("blocked");
    std::fs::create_dir(&blocked).unwrap();
    std::fs::write(blocked.join("private.bin"), b"private").unwrap();

    let mut permissions = std::fs::metadata(&blocked).unwrap().permissions();
    let original_mode = permissions.mode();
    permissions.set_mode(0o000);
    std::fs::set_permissions(&blocked, permissions).unwrap();

    let result = inventory_cloud_local_allocations(&root(temp.path()), options(), 123);

    // Restore the fixture before assertions so TempDir cleanup remains reliable even if an
    // assertion below fails while unwinding on a platform that enforces directory search bits.
    let mut restored = std::fs::metadata(&blocked).unwrap().permissions();
    restored.set_mode(original_mode);
    std::fs::set_permissions(&blocked, restored).unwrap();

    let report = result.unwrap();
    assert!(!report.evidence_complete);
    assert!(report
        .stop_reasons
        .iter()
        .any(|reason| reason == "entry-errors"));
    assert!(report.issues.iter().any(|issue| {
        issue.relative_scope.as_deref() == Some("blocked")
            && issue.kind == "read-directory-failed"
            && issue.reason == "permission-denied"
    }));
    assert!(report
        .candidates
        .iter()
        .all(|candidate| !candidate.path.contains("private.bin")));
}
