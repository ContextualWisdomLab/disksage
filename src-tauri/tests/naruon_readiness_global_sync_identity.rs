//! Provider-global-sync identity must be bound before Naruon readiness consumes its state.
//!
//! A caller can construct `ProviderGlobalSyncReport` directly. The readiness exporter therefore
//! must reject a report whose state/blocker shape is plausible but whose evidence kind is not the
//! canonical read-only File Provider global dump contract.

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
        notices: Vec::new(),
    }
}

#[test]
fn forged_global_sync_evidence_kind_cannot_enter_readiness() {
    let plan = empty_onedrive_plan();
    let runtime = assess_provider_client_runtime(
        CloudProvider::Onedrive,
        Some(b"OneDrive Sync Service\n"),
        25,
    );
    let forged = ProviderGlobalSyncReport {
        schema_version: PROVIDER_GLOBAL_SYNC_SCHEMA_VERSION,
        provider: CloudProvider::Onedrive,
        evidence_kind: "caller-asserted-clear-state".into(),
        evidence_complete: true,
        state: ProviderGlobalSyncState::Clear,
        upload_progress_present: false,
        download_progress_present: false,
        pending_indexable_count: Some(0),
        blockers: Vec::new(),
        notices: Vec::new(),
    };

    assert_eq!(
        export_naruon_cloud_copy_readiness_with_global_sync(
            &plan,
            &runtime,
            None,
            Some(&forged),
        )
        .unwrap_err(),
        "naruon-copy-readiness-provider-global-sync-invalid"
    );
}
