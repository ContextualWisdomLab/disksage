use disksage_lib::cloud::CloudProvider;
use disksage_lib::provider_global_sync::{parse_dump, ProviderGlobalSyncState};

#[test]
fn prefixed_indexing_and_error_markers_fail_closed() {
    let dump = r#"
com.microsoft.OneDrive.FileProvider
sync engine state:
    + scheduling state: idle
    + pending-indexable-count: 0
    + needs-indexing: yes
    + errors: 4
"#;

    let report = parse_dump(CloudProvider::Onedrive, dump).expect("valid provider dump");

    assert_eq!(report.state, ProviderGlobalSyncState::Error);
    assert!(report
        .blockers
        .contains(&"provider-global-sync-indexing-pending".to_string()));
    assert!(report
        .blockers
        .contains(&"provider-global-sync-error".to_string()));
}
