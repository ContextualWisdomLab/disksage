use disksage_lib::cloud::{
    CloudPlanOptions, CloudPlanReport, CloudProvider, CloudRoot, ExactDuplicateSummary,
};
use disksage_lib::icloud_sync_health::{
    IcloudFileProviderActivityEvidence, IcloudSyncHealthReport, IcloudUploadQueueSummary,
    ManagedDatabaseFileEvidence, ICLOUD_FILE_PROVIDER_ACTIVITY_SCHEMA_VERSION,
    ICLOUD_SYNC_HEALTH_SCHEMA_VERSION,
};
use disksage_lib::naruon_cloud_copy_readiness::{
    export_naruon_cloud_copy_readiness, validate_naruon_cloud_copy_readiness,
};
use disksage_lib::provider_capacity::{
    assess_capacity, unavailable_capacity, DEFAULT_CAPACITY_RESERVE_BYTES,
};
use disksage_lib::provider_client_runtime::assess_provider_client_runtime;

fn icloud_report() -> CloudPlanReport {
    let capacity = unavailable_capacity(CloudProvider::Icloud, 10, "capacity-unavailable");
    CloudPlanReport {
        cloud_root: CloudRoot {
            id: "icloud-test-root".into(),
            provider: CloudProvider::Icloud,
            account_scope: disksage_lib::cloud::CloudAccountScope::Personal,
            label: "iCloud Drive".into(),
            path: "/private/icloud".into(),
            readable: true,
            access_issue: None,
        },
        generated_at_ms: 20,
        source_selection_policy: Some(CloudPlanOptions {
            min_size_bytes: 90 * 1024 * 1024,
            min_age_days: 30,
            limit: 200,
        }),
        candidates: Vec::new(),
        candidate_bytes: 0,
        potentially_reclaimable_bytes: 0,
        exact_duplicates: ExactDuplicateSummary::default(),
        capacity: Some(assess_capacity(
            capacity,
            0,
            0,
            DEFAULT_CAPACITY_RESERVE_BYTES,
        )),
        local_volume: None,
        pre_copy_evidence: None,
        notices: Vec::new(),
    }
}

fn active_transfer_health() -> IcloudSyncHealthReport {
    let blocker = "icloud-file-provider-transfer-active".to_string();
    let disk_import_blocker = "icloud-file-provider-disk-import-active".to_string();
    IcloudSyncHealthReport {
        schema_version: ICLOUD_SYNC_HEALTH_SCHEMA_VERSION,
        output_mode: "icloud-local-sync-health".into(),
        observed_at_ms: 30,
        admission_blocked_since_ms: None,
        provider: "icloud".into(),
        evidence_kind: "supplementary-local-cloud-docs-private-schema".into(),
        evidence_complete: true,
        database_snapshot_includes_wal: true,
        database_sidecar_write_permitted: false,
        managed_database_files: vec![ManagedDatabaseFileEvidence {
            role: "client.db".into(),
            present: true,
            logical_bytes: 1,
            allocated_bytes: 1,
            modified_ms: Some(1),
        }],
        managed_database_allocated_bytes: 1,
        upload_queue: IcloudUploadQueueSummary::default(),
        native_status: None,
        file_provider_activity: Some(IcloudFileProviderActivityEvidence {
            schema_version: ICLOUD_FILE_PROVIDER_ACTIVITY_SCHEMA_VERSION,
            observed_at_ms: 30,
            command_succeeded: true,
            timed_out: false,
            output_truncated: false,
            no_progress_fetch_count: 0,
            no_progress_create_count: 0,
            materialization_failure_count: 0,
            staged_item_missing_count: 0,
            sync_excluded_filename_count: 0,
            sync_excluded_root_count: 0,
            pending_indexable_count: None,
            active_upload_count: 1,
            active_download_count: 0,
            active_upload_progress_millionths: Some(500_000),
            active_download_progress_millionths: None,
            notices: vec![
                "icloud-file-provider-dump-read-only".into(),
                disk_import_blocker.clone(),
            ],
        }),
        sync_backlog_present: true,
        new_copy_admission_state: "blocked".into(),
        new_copy_admission_blockers: vec![disk_import_blocker.clone(), blocker.clone()],
        blockers: vec![disk_import_blocker, blocker],
        notices: Vec::new(),
        paths_redacted: true,
        user_filenames_read: false,
        user_file_contents_read: false,
        remote_capacity_verified: false,
        provider_sync_attested: false,
        local_eviction_authorized: false,
        mutation_performed: false,
    }
}

#[test]
fn active_fileprovider_transfer_exports_blocked_readiness() {
    let report = icloud_report();
    let runtime = assess_provider_client_runtime(CloudProvider::Icloud, None, 25);
    let health = active_transfer_health();

    let envelope = export_naruon_cloud_copy_readiness(&report, &runtime, Some(&health))
        .expect("an active transfer is valid blocked readiness evidence");

    assert_eq!(envelope.icloud_new_copy_admission_met, Some(false));
    let admission = envelope
        .icloud_new_copy_admission
        .as_ref()
        .expect("iCloud readiness summary must be present");
    assert_eq!(admission.state, "blocked");
    assert!(admission
        .blockers
        .iter()
        .any(|blocker| blocker == "icloud-file-provider-transfer-active"));
    assert!(admission
        .blockers
        .iter()
        .any(|blocker| blocker == "icloud-file-provider-disk-import-active"));
    assert!(validate_naruon_cloud_copy_readiness(&envelope).is_ok());
}
