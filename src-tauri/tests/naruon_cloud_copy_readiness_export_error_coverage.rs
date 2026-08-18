//! Credential-free fail-closed coverage for NarUon readiness export input bindings.
//!
//! These tests use only deterministic in-memory evidence. They do not invoke provider tools,
//! contact cloud APIs, write cloud state, or authorize source eviction.

use disksage_lib::cloud::{
    CloudAccountScope, CloudPlanOptions, CloudPlanReport, CloudProvider, CloudRoot,
    ExactDuplicateSummary,
};
use disksage_lib::naruon_cloud_copy_readiness::{
    export_naruon_cloud_copy_readiness, export_naruon_cloud_copy_readiness_with_global_sync,
};
use disksage_lib::provider_capacity::{
    assess_capacity, unavailable_capacity, DEFAULT_CAPACITY_RESERVE_BYTES,
};
use disksage_lib::provider_client_runtime::assess_provider_client_runtime;
use disksage_lib::provider_global_sync::{
    ProviderGlobalSyncReport, ProviderGlobalSyncState, PROVIDER_GLOBAL_SYNC_SCHEMA_VERSION,
};

fn report(provider: CloudProvider) -> CloudPlanReport {
    let scope = if provider == CloudProvider::GoogleDrive {
        CloudAccountScope::Unknown
    } else {
        CloudAccountScope::Personal
    };
    CloudPlanReport {
        cloud_root: CloudRoot {
            id: "naruon-export-error-root".into(),
            provider,
            account_scope: scope,
            label: "NarUon export error root".into(),
            path: "/private/cloud".into(),
            readable: true,
            access_issue: None,
        },
        generated_at_ms: 20,
        source_selection_policy: Some(CloudPlanOptions {
            min_size_bytes: 1,
            min_age_days: 0,
            limit: 10,
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
        notices: Vec::new(),
    }
}

fn global_sync(
    provider: CloudProvider,
    state: ProviderGlobalSyncState,
    evidence_complete: bool,
    blockers: &[&str],
) -> ProviderGlobalSyncReport {
    ProviderGlobalSyncReport {
        schema_version: PROVIDER_GLOBAL_SYNC_SCHEMA_VERSION,
        provider,
        evidence_kind: "fileproviderctl-global-dump".into(),
        evidence_complete,
        state,
        upload_progress_present: false,
        download_progress_present: false,
        pending_indexable_count: Some(0),
        blockers: blockers.iter().map(|value| (*value).into()).collect(),
        notices: vec!["provider-global-sync-dump-read-only".into()],
    }
}

#[test]
fn exporter_rejects_runtime_provider_mismatch_and_missing_selection_policy() {
    let onedrive = report(CloudProvider::Onedrive);
    let google_runtime = assess_provider_client_runtime(CloudProvider::GoogleDrive, None, 25);
    assert_eq!(
        export_naruon_cloud_copy_readiness(&onedrive, &google_runtime, None).unwrap_err(),
        "naruon-copy-readiness-runtime-provider-mismatch"
    );

    let mut missing_policy = report(CloudProvider::Onedrive);
    missing_policy.source_selection_policy = None;
    let onedrive_runtime = assess_provider_client_runtime(
        CloudProvider::Onedrive,
        Some(b"OneDrive Sync Service\n"),
        25,
    );
    assert_eq!(
        export_naruon_cloud_copy_readiness(&missing_policy, &onedrive_runtime, None).unwrap_err(),
        "naruon-copy-readiness-selection-policy-missing"
    );
}

#[test]
fn exporter_rejects_malformed_provider_global_sync_evidence() {
    let plan = report(CloudProvider::Onedrive);
    let runtime = assess_provider_client_runtime(
        CloudProvider::Onedrive,
        Some(b"OneDrive Sync Service\n"),
        25,
    );

    let mut schema_drift = global_sync(
        CloudProvider::Onedrive,
        ProviderGlobalSyncState::Clear,
        true,
        &[],
    );
    schema_drift.schema_version = schema_drift.schema_version.saturating_add(1);

    let variants = [
        schema_drift,
        global_sync(
            CloudProvider::Onedrive,
            ProviderGlobalSyncState::Pending,
            true,
            &["-invalid"],
        ),
        global_sync(CloudProvider::Onedrive, ProviderGlobalSyncState::Clear, false, &[]),
        global_sync(
            CloudProvider::Onedrive,
            ProviderGlobalSyncState::Clear,
            true,
            &["provider-global-sync-error"],
        ),
        global_sync(
            CloudProvider::Onedrive,
            ProviderGlobalSyncState::Pending,
            true,
            &[],
        ),
        global_sync(
            CloudProvider::GoogleDrive,
            ProviderGlobalSyncState::Clear,
            true,
            &[],
        ),
    ];

    for evidence in variants {
        assert_eq!(
            export_naruon_cloud_copy_readiness_with_global_sync(
                &plan,
                &runtime,
                None,
                Some(&evidence),
            )
            .unwrap_err(),
            "naruon-copy-readiness-provider-global-sync-invalid"
        );
    }
}

#[test]
fn icloud_export_rejects_non_icloud_global_sync_channel() {
    let plan = report(CloudProvider::Icloud);
    let runtime = assess_provider_client_runtime(CloudProvider::Icloud, None, 25);
    let evidence = global_sync(
        CloudProvider::Onedrive,
        ProviderGlobalSyncState::Clear,
        true,
        &[],
    );

    assert_eq!(
        export_naruon_cloud_copy_readiness_with_global_sync(
            &plan,
            &runtime,
            None,
            Some(&evidence),
        )
        .unwrap_err(),
        "naruon-copy-readiness-provider-global-sync-icloud-invalid"
    );
}
