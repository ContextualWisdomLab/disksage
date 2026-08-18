//! Credential-free coverage for readiness gates that bind exported candidate authority.
//!
//! The fixture is produced by the real exporter from deterministic in-memory provider evidence.
//! Each mutation then proves that a contradictory ready/runtime/global-sync claim fails closed.

use disksage_lib::cloud::{
    ArchiveKind, CloudAccountScope, CloudCandidate, CloudPlanOptions, CloudPlanReport, CloudProvider,
    CloudRoot, ExactDuplicateSummary, MetadataEvidence,
};
use disksage_lib::naruon_cloud_copy_readiness::{
    export_naruon_cloud_copy_readiness_with_global_sync, validate_naruon_cloud_copy_readiness,
    CloudCopyReadinessState, CountBytes,
};
use disksage_lib::provider_capacity::{
    assess_capacity, parse_onedrive_capacity, DEFAULT_CAPACITY_RESERVE_BYTES,
};
use disksage_lib::provider_client_runtime::assess_provider_client_runtime;
use disksage_lib::provider_global_sync::{
    ProviderGlobalSyncReport, ProviderGlobalSyncState, PROVIDER_GLOBAL_SYNC_SCHEMA_VERSION,
};

fn ready_candidate() -> CloudCandidate {
    let mut candidate = CloudCandidate {
        metadata_fingerprint: "a".repeat(64),
        review_fingerprint: String::new(),
        src: "/private/source/a.pdf".into(),
        dst: "/private/cloud/DiskSage Archive/2026/08/document/a.pdf".into(),
        provider: CloudProvider::Onedrive,
        destination_account_scope: CloudAccountScope::Personal,
        kind: ArchiveKind::Document,
        bytes: 42,
        age_days: 100,
        created_ms: 1,
        modified_ms: 2,
        production_time_ms: 3,
        production_time_source: "embedded:exiftool:CreateDate".into(),
        production_time_confidence: "high".into(),
        source_root: "/private/source".into(),
        relative_path: "a.pdf".into(),
        source_context: "downloads".into(),
        requires_review: false,
        review_reasons: Vec::new(),
        content_title: None,
        content_authors: Vec::new(),
        content_context: Vec::new(),
        duration_ms: None,
        dataset_profile: None,
        metadata_evidence: vec![MetadataEvidence {
            field: "production-date".into(),
            value: "redacted-test-value".into(),
            source: "embedded:exiftool:CreateDate".into(),
            confidence: "high".into(),
        }],
        blocked_reason: None,
    };
    candidate.review_fingerprint = disksage_lib::cloud::candidate_review_fingerprint(&candidate);
    candidate
}

fn clear_global_sync() -> ProviderGlobalSyncReport {
    ProviderGlobalSyncReport {
        schema_version: PROVIDER_GLOBAL_SYNC_SCHEMA_VERSION,
        provider: CloudProvider::Onedrive,
        evidence_kind: "fileproviderctl-global-dump".into(),
        evidence_complete: true,
        state: ProviderGlobalSyncState::Clear,
        upload_progress_present: false,
        download_progress_present: false,
        pending_indexable_count: Some(0),
        blockers: Vec::new(),
        notices: vec!["provider-global-sync-dump-read-only".into()],
    }
}

fn report_with_capacity_remaining(remaining: u64) -> CloudPlanReport {
    let total = 10_000_000_000u64;
    let used = total.saturating_sub(remaining);
    let snapshot = parse_onedrive_capacity(
        &format!(
            "{{\"id\":\"drive-id\",\"driveType\":\"personal\",\"quota\":{{\"deleted\":0,\"remaining\":{remaining},\"state\":\"normal\",\"total\":{total},\"used\":{used}}}}}"
        ),
        10,
    )
    .unwrap();
    CloudPlanReport {
        cloud_root: CloudRoot {
            id: "ready-gate-coverage-root".into(),
            provider: CloudProvider::Onedrive,
            account_scope: CloudAccountScope::Personal,
            label: "Ready gate coverage root".into(),
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
        candidates: vec![ready_candidate()],
        candidate_bytes: 42,
        potentially_reclaimable_bytes: 42,
        exact_duplicates: ExactDuplicateSummary::default(),
        capacity: Some(assess_capacity(
            snapshot,
            42,
            42,
            DEFAULT_CAPACITY_RESERVE_BYTES,
        )),
        notices: Vec::new(),
    }
}

fn ready_envelope() -> disksage_lib::naruon_cloud_copy_readiness::NaruonCloudCopyReadinessEnvelope {
    let report = report_with_capacity_remaining(9_000_000_000);
    let runtime = assess_provider_client_runtime(
        CloudProvider::Onedrive,
        Some(b"OneDrive Sync Service\n"),
        25,
    );
    export_naruon_cloud_copy_readiness_with_global_sync(
        &report,
        &runtime,
        None,
        Some(&clear_global_sync()),
    )
    .unwrap()
}

#[test]
fn ready_candidate_requires_a_matching_runtime_prerequisite() {
    let unavailable_runtime = assess_provider_client_runtime(CloudProvider::Onedrive, None, 25);
    let mut envelope = ready_envelope();
    envelope.provider_runtime = unavailable_runtime.clone();
    envelope.provider_runtime_prerequisite_met = unavailable_runtime.copy_prerequisite_met;

    assert_eq!(
        validate_naruon_cloud_copy_readiness(&envelope).unwrap_err(),
        "naruon-copy-readiness-ready-gate-invalid"
    );
}

#[test]
fn blocked_candidate_set_must_carry_the_runtime_blocker_from_its_snapshot() {
    let unavailable_runtime = assess_provider_client_runtime(CloudProvider::Onedrive, None, 25);
    let mut envelope = ready_envelope();
    envelope.provider_runtime = unavailable_runtime.clone();
    envelope.provider_runtime_prerequisite_met = unavailable_runtime.copy_prerequisite_met;
    envelope.ready_without_new_review = CountBytes::default();
    envelope.readiness_state = CloudCopyReadinessState::Blocked;

    assert_eq!(
        validate_naruon_cloud_copy_readiness(&envelope).unwrap_err(),
        "naruon-copy-readiness-runtime-binding-invalid"
    );
}

#[test]
fn verified_but_insufficient_capacity_blocks_copy_admission() {
    let report = report_with_capacity_remaining(1);
    let runtime = assess_provider_client_runtime(
        CloudProvider::Onedrive,
        Some(b"OneDrive Sync Service\n"),
        25,
    );
    let envelope = export_naruon_cloud_copy_readiness_with_global_sync(
        &report,
        &runtime,
        None,
        Some(&clear_global_sync()),
    )
    .unwrap();

    assert!(envelope.remote_capacity_verified);
    assert_eq!(envelope.readiness_state, CloudCopyReadinessState::Blocked);
    assert_eq!(envelope.ready_without_new_review, CountBytes::default());
    assert_eq!(
        envelope
            .candidate_blocker_counts
            .get("cloud-capacity-insufficient-with-reserve"),
        Some(&CountBytes { count: 1, bytes: 42 })
    );
    assert!(!envelope.cloud_write_executed);
    assert!(!envelope.source_eviction_authorized);
}

#[test]
fn clear_global_sync_evidence_rejects_an_unbound_global_sync_blocker() {
    let mut envelope = ready_envelope();
    envelope.candidate_blocker_counts.insert(
        "provider-global-sync-evidence-unavailable".into(),
        CountBytes { count: 1, bytes: 42 },
    );

    assert_eq!(
        validate_naruon_cloud_copy_readiness(&envelope).unwrap_err(),
        "naruon-copy-readiness-provider-global-sync-binding-invalid"
    );
}