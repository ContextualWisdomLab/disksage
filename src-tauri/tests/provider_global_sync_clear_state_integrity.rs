//! Contradictory provider-global-sync evidence must never authorize or advertise a new copy.
//!
//! `ProviderGlobalSyncReport` is a public data contract and callers can construct it directly.
//! A `Clear` report is therefore authoritative only when its aggregate progress fields also prove
//! that no transfer or indexing work remains. State/blocker labels alone are insufficient.

use disksage_lib::cloud::CloudProvider;
use disksage_lib::provider_global_sync::{
    attach_new_copy_admission_notice, require_new_copy_admission, ProviderGlobalSyncReport,
    ProviderGlobalSyncState, PROVIDER_GLOBAL_SYNC_SCHEMA_VERSION,
};

fn clear_report() -> ProviderGlobalSyncReport {
    ProviderGlobalSyncReport {
        schema_version: PROVIDER_GLOBAL_SYNC_SCHEMA_VERSION,
        provider: CloudProvider::Onedrive,
        observed_at_ms: 42,
        evidence_kind: "fileproviderctl-global-dump".into(),
        evidence_complete: true,
        state: ProviderGlobalSyncState::Clear,
        upload_progress_present: false,
        download_progress_present: false,
        pending_indexable_count: Some(0),
        blockers: Vec::new(),
        notices: Vec::new(),
        probe_receipt: None,
    }
}

#[test]
fn clear_state_requires_quiet_aggregate_progress_evidence() {
    let baseline = clear_report();
    assert_eq!(require_new_copy_admission(&baseline), Ok(()));

    let mut upload_active = baseline.clone();
    upload_active.upload_progress_present = true;

    let mut download_active = baseline.clone();
    download_active.download_progress_present = true;

    let mut indexing_pending = baseline;
    indexing_pending.pending_indexable_count = Some(1);

    for contradictory in [upload_active, download_active, indexing_pending] {
        assert_eq!(
            require_new_copy_admission(&contradictory).unwrap_err(),
            "provider-global-sync-evidence-invalid"
        );

        let mut notices = Vec::new();
        attach_new_copy_admission_notice(&mut notices, Some(&contradictory));
        assert!(notices.contains(&"provider-global-sync-blocked".to_string()));
        assert!(!notices.contains(&"provider-global-sync-clear".to_string()));
    }
}

#[test]
fn forged_identity_is_never_advertised_as_clear() {
    let mut forged = clear_report();
    forged.evidence_kind = "caller-asserted-clear-state".into();

    assert_eq!(
        require_new_copy_admission(&forged).unwrap_err(),
        "provider-global-sync-evidence-invalid"
    );

    let mut notices = Vec::new();
    attach_new_copy_admission_notice(&mut notices, Some(&forged));
    assert!(notices.contains(&"provider-global-sync-blocked".to_string()));
    assert!(!notices.contains(&"provider-global-sync-clear".to_string()));
}
