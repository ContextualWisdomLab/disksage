//! Regression for bounded provider-global-sync disk-full diagnostics.
//!
//! Numeric OS/provider error markers must classify the exact ENOSPC code 28 without
//! mislabeling unrelated codes that merely begin with the same digits.

use disksage_lib::cloud::CloudProvider;
use disksage_lib::provider_global_sync::{parse_dump, ProviderGlobalSyncState};

#[test]
fn code_28_marker_requires_a_numeric_boundary() {
    let unrelated = "com.google.drivefs.fpext\nsync engine state:\n error:'NSFileProviderErrorDomain Code=280 unrelated provider failure'\n";
    let unrelated_report = parse_dump(CloudProvider::GoogleDrive, unrelated).unwrap();
    assert_eq!(unrelated_report.state, ProviderGlobalSyncState::Error);
    assert!(unrelated_report
        .blockers
        .contains(&"provider-global-sync-error".into()));
    assert!(!unrelated_report
        .blockers
        .contains(&"provider-global-sync-local-disk-full".into()));

    for marker in [
        "NSFileProviderErrorDomain Code=28 write failed",
        "NSFileProviderErrorDomain Code 28 write failed",
        "NSFileProviderErrorDomain Code=28",
    ] {
        let dump = format!("com.google.drivefs.fpext\nsync engine state:\n {marker}\n");
        let report = parse_dump(CloudProvider::GoogleDrive, &dump).unwrap();
        assert_eq!(report.state, ProviderGlobalSyncState::Error);
        assert!(report
            .blockers
            .contains(&"provider-global-sync-local-disk-full".into()));
    }
}
