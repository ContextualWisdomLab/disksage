use disksage_lib::cloud::CloudProvider;
use disksage_lib::provider_global_sync::{parse_dump, ProviderGlobalSyncState};

#[test]
fn later_positive_pending_indexable_count_cannot_be_hidden_by_earlier_zero() {
    let dump = r#"
com.microsoft.OneDrive.FileProvider
sync engine state:
    + pending-indexable-count: 0
    + scheduling state: idle
    + pending-indexable-count: 2
"#;

    let report = parse_dump(CloudProvider::Onedrive, dump).expect("provider dump should parse");

    assert_eq!(report.pending_indexable_count, Some(2));
    assert_eq!(report.state, ProviderGlobalSyncState::Pending);
    assert!(report
        .blockers
        .contains(&"provider-global-sync-indexing-pending".to_string()));
}
