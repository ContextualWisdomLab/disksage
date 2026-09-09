//! Regression for provider-global-sync disk-full state consistency.
//!
//! Bare ENOSPC-style diagnostics are valid File Provider failure evidence even when they are not
//! wrapped in an `error:'…'` marker. The report must not advertise a healthy `clear` state while
//! simultaneously carrying a disk-full blocker, because downstream readiness validation rejects
//! that contradictory envelope instead of preserving the actionable incident reason.

use disksage_lib::cloud::CloudProvider;
use disksage_lib::provider_global_sync::{parse_dump, ProviderGlobalSyncState};

#[test]
fn bare_disk_full_marker_is_an_error_state_with_actionable_blocker() {
    let dump = "com.microsoft.OneDrive.FileProvider\nsync engine state:\n write failed: ENOSPC\n";

    let report = parse_dump(CloudProvider::Onedrive, dump).expect("parse provider dump");

    assert_eq!(report.state, ProviderGlobalSyncState::Error);
    assert!(report
        .blockers
        .iter()
        .any(|blocker| blocker == "provider-global-sync-local-disk-full"));
    assert!(report
        .blockers
        .iter()
        .any(|blocker| blocker == "provider-global-sync-error"));
}
