//! Credential-free coverage for the complete iCloud readiness blocker matrix.
//!
//! These regressions exercise only deterministic in-memory readiness evidence. They do not contact
//! iCloud, open user content, write cloud state, or grant source-eviction authority.

use disksage_lib::cloud::{
    CloudAccountScope, CloudPlanOptions, CloudPlanReport, CloudProvider, CloudRoot,
    ExactDuplicateSummary,
};
use disksage_lib::icloud_sync_health::{
    IcloudSyncHealthReport, IcloudUploadQueueSummary, ManagedDatabaseFileEvidence,
    ICLOUD_SYNC_HEALTH_SCHEMA_VERSION,
};
use disksage_lib::naruon_cloud_copy_readiness::{
    export_naruon_cloud_copy_readiness, validate_naruon_cloud_copy_readiness,
    NaruonCloudCopyReadinessEnvelope,
};
use disksage_lib::provider_capacity::{
    assess_capacity, unavailable_capacity, DEFAULT_CAPACITY_RESERVE_BYTES,
};
use disksage_lib::provider_client_runtime::assess_provider_client_runtime;

fn report() -> CloudPlanReport {
    let provider = CloudProvider::Icloud;
    CloudPlanReport {
        cloud_root: CloudRoot {
            id: "icloud-matrix-coverage-root".into(),
            provider,
            account_scope: CloudAccountScope::Personal,
            label: "iCloud matrix coverage root".into(),
            path: "/coverage/icloud".into(),
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
        notices: Vec::new(),
    }
}

fn fully_blocked_health() -> IcloudSyncHealthReport {
    let blockers = vec![
        "icloud-upload-queue-nonempty".into(),
        "icloud-upload-in-flight".into(),
        "icloud-upload-blocked-on-sync-up".into(),
        "icloud-upload-out-of-quota".into(),
        "icloud-upload-queue-state-unclassified".into(),
        "icloud-local-sync-item-error-present".into(),
    ];
    IcloudSyncHealthReport {
        schema_version: ICLOUD_SYNC_HEALTH_SCHEMA_VERSION,
        output_mode: "icloud-local-sync-health".into(),
        observed_at_ms: 30,
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
        upload_queue: IcloudUploadQueueSummary {
            scheduled_waiting_count: 1,
            scheduled_waiting_bytes: 10,
            scheduled_active_count: 1,
            scheduled_active_bytes: 20,
            scheduled_count: 2,
            scheduled_bytes: 30,
            blocked_on_sync_up_count: 1,
            out_of_quota_count: 1,
            out_of_quota_bytes: 10,
            other_state_count: 1,
            item_error_count: 2,
            item_error_octagon_not_signed_in_count: 1,
            item_error_unclassified_count: 1,
            newest_item_error_timestamp_ms: Some(25),
            ..IcloudUploadQueueSummary::default()
        },
        sync_backlog_present: true,
        new_copy_admission_state: "blocked".into(),
        new_copy_admission_blockers: blockers.clone(),
        blockers,
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

fn blocked_envelope() -> NaruonCloudCopyReadinessEnvelope {
    let runtime = assess_provider_client_runtime(CloudProvider::Icloud, None, 25);
    export_naruon_cloud_copy_readiness(&report(), &runtime, Some(&fully_blocked_health())).unwrap()
}

#[test]
fn complete_icloud_blocker_matrix_is_exported_without_authority() {
    let envelope = blocked_envelope();
    let admission = envelope
        .icloud_new_copy_admission
        .as_ref()
        .expect("iCloud health must be exported");

    assert_eq!(admission.state, "blocked");
    assert_eq!(admission.scheduled_waiting_count, 1);
    assert_eq!(admission.scheduled_waiting_bytes, 10);
    assert_eq!(admission.scheduled_active_count, 1);
    assert_eq!(admission.scheduled_active_bytes, 20);
    assert_eq!(admission.scheduled_count, 2);
    assert_eq!(admission.scheduled_bytes, 30);
    assert_eq!(admission.blocked_on_sync_up_count, 1);
    assert_eq!(admission.out_of_quota_count, 1);
    assert_eq!(admission.out_of_quota_bytes, 10);
    assert_eq!(admission.other_state_count, 1);
    assert_eq!(admission.item_error_count, 2);
    assert_eq!(admission.item_error_octagon_not_signed_in_count, 1);
    assert_eq!(admission.item_error_unclassified_count, 1);
    assert_eq!(admission.newest_item_error_timestamp_ms, Some(25));
    assert_eq!(admission.newest_item_error_age_ms, Some(5));
    assert_eq!(
        admission.blockers,
        vec![
            "icloud-upload-queue-nonempty",
            "icloud-upload-in-flight",
            "icloud-upload-blocked-on-sync-up",
            "icloud-upload-out-of-quota",
            "icloud-upload-queue-state-unclassified",
            "icloud-local-sync-item-error-present",
        ]
    );
    assert_eq!(envelope.icloud_new_copy_admission_met, Some(false));
    assert!(!envelope.cloud_write_executed);
    assert!(!envelope.source_eviction_authorized);
    assert!(validate_naruon_cloud_copy_readiness(&envelope).is_ok());
}

#[test]
fn icloud_summary_arithmetic_and_shape_fail_closed_independently() {
    let baseline = blocked_envelope();

    let mut count_overflow = baseline.clone();
    let summary = count_overflow.icloud_new_copy_admission.as_mut().unwrap();
    summary.scheduled_waiting_count = u64::MAX;
    summary.scheduled_active_count = 1;
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&count_overflow).unwrap_err(),
        "naruon-copy-readiness-icloud-count-overflow"
    );

    let mut bytes_overflow = baseline.clone();
    let summary = bytes_overflow.icloud_new_copy_admission.as_mut().unwrap();
    summary.scheduled_waiting_bytes = u64::MAX;
    summary.scheduled_active_bytes = 1;
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&bytes_overflow).unwrap_err(),
        "naruon-copy-readiness-icloud-bytes-overflow"
    );

    let mut item_error_overflow = baseline.clone();
    let summary = item_error_overflow.icloud_new_copy_admission.as_mut().unwrap();
    summary.item_error_octagon_not_signed_in_count = u64::MAX;
    summary.item_error_unclassified_count = 1;
    assert_eq!(
        validate_naruon_cloud_copy_readiness(&item_error_overflow).unwrap_err(),
        "naruon-copy-readiness-icloud-item-error-overflow"
    );

    let mut variants = Vec::new();

    let mut value = baseline.clone();
    value.icloud_new_copy_admission.as_mut().unwrap().scheduled_count += 1;
    variants.push(value);

    let mut value = baseline.clone();
    value.icloud_new_copy_admission.as_mut().unwrap().scheduled_bytes += 1;
    variants.push(value);

    let mut value = baseline.clone();
    let summary = value.icloud_new_copy_admission.as_mut().unwrap();
    summary.scheduled_waiting_count = 0;
    variants.push(value);

    let mut value = baseline.clone();
    let summary = value.icloud_new_copy_admission.as_mut().unwrap();
    summary.scheduled_active_count = 0;
    variants.push(value);

    let mut value = baseline.clone();
    let summary = value.icloud_new_copy_admission.as_mut().unwrap();
    summary.out_of_quota_count = 0;
    variants.push(value);

    let mut value = baseline.clone();
    value.icloud_new_copy_admission.as_mut().unwrap().item_error_count += 1;
    variants.push(value);

    let mut value = baseline.clone();
    value
        .icloud_new_copy_admission
        .as_mut()
        .unwrap()
        .newest_item_error_age_ms = Some(4);
    variants.push(value);

    let mut value = baseline.clone();
    value.icloud_new_copy_admission.as_mut().unwrap().state = "clear".into();
    variants.push(value);

    let mut value = baseline;
    value.icloud_new_copy_admission.as_mut().unwrap().blockers.pop();
    variants.push(value);

    for envelope in variants {
        assert_eq!(
            validate_naruon_cloud_copy_readiness(&envelope).unwrap_err(),
            "naruon-copy-readiness-icloud-shape-invalid"
        );
    }
}

#[test]
fn icloud_export_rejects_schema_privacy_and_authority_claim_drift() {
    let plan = report();
    let runtime = assess_provider_client_runtime(CloudProvider::Icloud, None, 25);
    let mut variants = Vec::new();

    let mut value = fully_blocked_health();
    value.schema_version = value.schema_version.saturating_add(1);
    variants.push(value);

    let mut value = fully_blocked_health();
    value.provider = "onedrive".into();
    variants.push(value);

    let mut value = fully_blocked_health();
    value.output_mode = "wrong-output-mode".into();
    variants.push(value);

    let mut value = fully_blocked_health();
    value.evidence_kind = "wrong-evidence-kind".into();
    variants.push(value);

    let mut value = fully_blocked_health();
    value.paths_redacted = false;
    variants.push(value);

    let mut value = fully_blocked_health();
    value.user_filenames_read = true;
    variants.push(value);

    let mut value = fully_blocked_health();
    value.user_file_contents_read = true;
    variants.push(value);

    let mut value = fully_blocked_health();
    value.remote_capacity_verified = true;
    variants.push(value);

    let mut value = fully_blocked_health();
    value.provider_sync_attested = true;
    variants.push(value);

    let mut value = fully_blocked_health();
    value.local_eviction_authorized = true;
    variants.push(value);

    let mut value = fully_blocked_health();
    value.mutation_performed = true;
    variants.push(value);

    let mut value = fully_blocked_health();
    value.database_sidecar_write_permitted = true;
    variants.push(value);

    let mut value = fully_blocked_health();
    value.evidence_complete = false;
    variants.push(value);

    for health in variants {
        assert_eq!(
            export_naruon_cloud_copy_readiness(&plan, &runtime, Some(&health)).unwrap_err(),
            "naruon-copy-readiness-icloud-claim-invalid"
        );
    }
}

#[test]
fn icloud_export_arithmetic_overflow_fails_at_input_projection_boundary() {
    let plan = report();
    let runtime = assess_provider_client_runtime(CloudProvider::Icloud, None, 25);

    let mut count_overflow = fully_blocked_health();
    count_overflow.upload_queue.scheduled_waiting_count = u64::MAX;
    count_overflow.upload_queue.scheduled_active_count = 1;
    assert_eq!(
        export_naruon_cloud_copy_readiness(&plan, &runtime, Some(&count_overflow)).unwrap_err(),
        "naruon-copy-readiness-icloud-count-overflow"
    );

    let mut bytes_overflow = fully_blocked_health();
    bytes_overflow.upload_queue.scheduled_waiting_bytes = u64::MAX;
    bytes_overflow.upload_queue.scheduled_active_bytes = 1;
    assert_eq!(
        export_naruon_cloud_copy_readiness(&plan, &runtime, Some(&bytes_overflow)).unwrap_err(),
        "naruon-copy-readiness-icloud-bytes-overflow"
    );

    let mut item_error_overflow = fully_blocked_health();
    item_error_overflow
        .upload_queue
        .item_error_octagon_not_signed_in_count = u64::MAX;
    item_error_overflow.upload_queue.item_error_unclassified_count = 1;
    assert_eq!(
        export_naruon_cloud_copy_readiness(&plan, &runtime, Some(&item_error_overflow)).unwrap_err(),
        "naruon-copy-readiness-icloud-item-error-overflow"
    );
}

#[test]
fn incomplete_icloud_evidence_exports_unavailable_admission_without_authority() {
    let plan = report();
    let runtime = assess_provider_client_runtime(CloudProvider::Icloud, None, 25);
    let mut health = fully_blocked_health();
    health.evidence_complete = false;
    health.database_snapshot_includes_wal = false;
    health
        .new_copy_admission_blockers
        .insert(0, "icloud-sync-health-evidence-incomplete".into());
    health
        .blockers
        .insert(0, "icloud-sync-health-evidence-incomplete".into());

    let envelope = export_naruon_cloud_copy_readiness(&plan, &runtime, Some(&health)).unwrap();
    let admission = envelope
        .icloud_new_copy_admission
        .as_ref()
        .expect("incomplete iCloud evidence must remain visible as a blocked admission summary");

    assert_eq!(admission.state, "blocked");
    assert!(!admission.evidence_complete);
    assert!(!admission.database_snapshot_includes_wal);
    assert!(admission
        .blockers
        .iter()
        .any(|blocker| blocker == "icloud-sync-health-evidence-incomplete"));
    assert!(admission
        .blockers
        .iter()
        .any(|blocker| blocker == "icloud-new-copy-admission-evidence-unavailable"));
    assert_eq!(envelope.icloud_new_copy_admission_met, Some(false));
    assert!(!envelope.cloud_write_executed);
    assert!(!envelope.source_eviction_authorized);
    assert!(validate_naruon_cloud_copy_readiness(&envelope).is_ok());
}
