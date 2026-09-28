//! Provider-global-sync identity and quiet-state evidence must be bound before Naruon readiness.
//!
//! A caller can construct `ProviderGlobalSyncReport` directly. The readiness exporter therefore
//! must reject a report whose state/blocker shape is plausible but whose evidence identity is
//! forged or whose aggregate progress fields contradict a claimed `Clear` state.

use disksage_lib::cloud::{
    CloudAccountScope, CloudPlanOptions, CloudPlanReport, CloudProvider, CloudRoot,
    ExactDuplicateSummary,
};
use disksage_lib::naruon_cloud_copy_readiness::export_naruon_cloud_copy_readiness_with_global_sync;
use disksage_lib::provider_capacity::{
    assess_capacity, unavailable_capacity, DEFAULT_CAPACITY_RESERVE_BYTES,
};
use disksage_lib::provider_client_runtime::assess_provider_client_runtime;
use disksage_lib::provider_global_sync::{
    ProviderGlobalSyncReport, ProviderGlobalSyncState, PROVIDER_GLOBAL_SYNC_SCHEMA_VERSION,
};

fn empty_onedrive_plan() -> CloudPlanReport {
    let provider = CloudProvider::Onedrive;
    CloudPlanReport {
        cloud_root: CloudRoot {
            id: "global-sync-identity-root".into(),
            provider,
            account_scope: CloudAccountScope::Personal,
            label: "Global sync identity root".into(),
            path: "/private/cloud".into(),
            readable: true,
            access_issue: None,
        },
        generated_at_ms: 20,
        source_selection_policy: Some(CloudPlanOptions {
            min_size_bytes: 1,
            min_age_days: 0,
            limit: 1,
        }),
        candidates: Vec::new(),
        candidate_bytes: 0,
        potentially_reclaimable_bytes: 0,
        exact_duplicates: ExactDuplicateSummary::default(),
        capacity: Some(assess_capacity(
            unavailable_capacity(provider, 10, "capacity-unavailable"),
            0,
            0,
            DEFAULT_CAPACITY_RESERVE_BYTES,
        )),
        local_volume: None,
        pre_copy_evidence: None,
        notices: Vec::new(),
    }
}

fn canonical_clear_report() -> ProviderGlobalSyncReport {
    ProviderGlobalSyncReport {
        schema_version: PROVIDER_GLOBAL_SYNC_SCHEMA_VERSION,
        provider: CloudProvider::Onedrive,
        evidence_kind: "fileproviderctl-global-dump".into(),
        observed_at_ms: 1,
        admission_blocked_since_ms: None,
        evidence_complete: true,
        state: ProviderGlobalSyncState::Clear,
        upload_progress_present: false,
        download_progress_present: false,
        pending_indexable_count: Some(0),
        blockers: Vec::new(),
        notices: Vec::new(),
    }
}

fn assert_rejected(report: &ProviderGlobalSyncReport) {
    let plan = empty_onedrive_plan();
    let runtime = assess_provider_client_runtime(
        CloudProvider::Onedrive,
        Some(b"OneDrive Sync Service\n"),
        25,
    );

    assert_eq!(
        export_naruon_cloud_copy_readiness_with_global_sync(
            &plan,
            &runtime,
            None,
            Some(report),
        )
        .unwrap_err(),
        "naruon-copy-readiness-provider-global-sync-invalid"
    );
}

#[test]
fn forged_global_sync_evidence_kind_cannot_enter_readiness() {
    let mut forged = canonical_clear_report();
    forged.evidence_kind = "caller-asserted-clear-state".into();
    assert_rejected(&forged);
}

#[test]
fn contradictory_clear_progress_cannot_enter_readiness() {
    let baseline = canonical_clear_report();

    let mut upload_active = baseline.clone();
    upload_active.upload_progress_present = true;

    let mut download_active = baseline.clone();
    download_active.download_progress_present = true;

    let mut indexing_pending = baseline;
    indexing_pending.pending_indexable_count = Some(1);

    for contradictory in [upload_active, download_active, indexing_pending] {
        assert_rejected(&contradictory);
    }
}
