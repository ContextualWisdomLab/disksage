//! Credential-free public-boundary coverage for NarUon readiness export states.
//!
//! These tests exercise the real deterministic exporter with in-memory provider evidence. They do
//! not contact a provider, read user content, write cloud state, or grant source-eviction authority.

use disksage_lib::cloud::{
    ArchiveKind, CloudAccountScope, CloudCandidate, CloudPlanOptions, CloudPlanReport, CloudProvider,
    CloudRoot, ExactDuplicateSummary, MetadataEvidence,
};
use disksage_lib::naruon_cloud_copy_readiness::{
    export_naruon_cloud_copy_readiness_with_global_sync, CloudCopyReadinessState, CountBytes,
};
use disksage_lib::provider_capacity::{
    assess_capacity, parse_onedrive_capacity, DEFAULT_CAPACITY_RESERVE_BYTES,
};
use disksage_lib::provider_client_runtime::assess_provider_client_runtime;
use disksage_lib::provider_global_sync::{
    ProviderGlobalSyncReport, ProviderGlobalSyncState, PROVIDER_GLOBAL_SYNC_SCHEMA_VERSION,
};

fn candidate(source: &str, review: bool, marker: char) -> CloudCandidate {
    let mut value = CloudCandidate {
        metadata_fingerprint: marker.to_string().repeat(64),
        review_fingerprint: String::new(),
        src: format!("/private/source/{marker}.pdf"),
        dst: format!("/private/cloud/DiskSage Archive/2026/08/document/{marker}.pdf"),
        provider: CloudProvider::Onedrive,
        destination_account_scope: CloudAccountScope::Personal,
        kind: ArchiveKind::Document,
        bytes: 42,
        age_days: 100,
        created_ms: 1,
        modified_ms: 2,
        production_time_ms: 3,
        production_time_source: source.into(),
        production_time_confidence: "high".into(),
        source_root: "/private/source".into(),
        relative_path: format!("{marker}.pdf"),
        source_context: "downloads".into(),
        requires_review: review,
        review_reasons: if review {
            vec!["metadata-review-required".into()]
        } else {
            Vec::new()
        },
        content_title: None,
        content_authors: Vec::new(),
        content_context: Vec::new(),
        duration_ms: None,
        dataset_profile: None,
        metadata_evidence: vec![MetadataEvidence {
            field: "production-date".into(),
            value: "redacted-test-value".into(),
            source: source.into(),
            confidence: "high".into(),
        }],
        blocked_reason: None,
    };
    value.review_fingerprint = disksage_lib::cloud::candidate_review_fingerprint(&value);
    value
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

fn report(candidates: Vec<CloudCandidate>) -> CloudPlanReport {
    let candidate_count = u64::try_from(candidates.len()).unwrap();
    let candidate_bytes = candidate_count * 42;
    let largest_candidate_bytes = if candidates.is_empty() { 0 } else { 42 };
    let snapshot = parse_onedrive_capacity(
        r#"{"id":"drive-id","driveType":"personal","quota":{"deleted":0,"remaining":9000000000,"state":"normal","total":10000000000,"used":1000000000}}"#,
        10,
    )
    .unwrap();

    CloudPlanReport {
        cloud_root: CloudRoot {
            id: "naruon-export-state-root".into(),
            provider: CloudProvider::Onedrive,
            account_scope: CloudAccountScope::Personal,
            label: "NarUon export state root".into(),
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
        candidates,
        candidate_bytes,
        potentially_reclaimable_bytes: candidate_bytes,
        exact_duplicates: ExactDuplicateSummary::default(),
        capacity: Some(assess_capacity(
            snapshot,
            candidate_bytes,
            largest_candidate_bytes,
            DEFAULT_CAPACITY_RESERVE_BYTES,
        )),
        notices: Vec::new(),
    }
}

#[test]
fn mixed_ready_and_review_candidates_export_partially_ready_with_all_time_buckets() {
    let plan = report(vec![
        candidate("filename:2026-08-18", false, 'a'),
        candidate("filesystem:modified-fallback", true, 'b'),
        candidate("unclassified:test-source", false, 'c'),
    ]);
    let runtime = assess_provider_client_runtime(
        CloudProvider::Onedrive,
        Some(b"OneDrive Sync Service\n"),
        25,
    );
    let global_sync = clear_global_sync();

    let envelope = export_naruon_cloud_copy_readiness_with_global_sync(
        &plan,
        &runtime,
        None,
        Some(&global_sync),
    )
    .unwrap();

    assert_eq!(envelope.readiness_state, CloudCopyReadinessState::PartiallyReady);
    assert_eq!(envelope.candidate_count, 3);
    assert_eq!(envelope.candidate_bytes, 126);
    assert_eq!(
        envelope.production_time_evidence.explicit_filename_date,
        CountBytes { count: 1, bytes: 42 }
    );
    assert_eq!(
        envelope.production_time_evidence.filesystem_modified,
        CountBytes { count: 1, bytes: 42 }
    );
    assert_eq!(
        envelope.production_time_evidence.unclassified,
        CountBytes { count: 1, bytes: 42 }
    );
    assert_eq!(
        envelope.ready_without_new_review,
        CountBytes { count: 2, bytes: 84 }
    );
    assert_eq!(
        envelope.requires_human_review,
        CountBytes { count: 1, bytes: 42 }
    );
    assert_eq!(
        envelope.candidate_blocker_counts.get("review-required"),
        Some(&CountBytes { count: 1, bytes: 42 })
    );
    assert!(envelope.provider_runtime_prerequisite_met);
    assert!(envelope.remote_capacity_verified);
    assert_eq!(envelope.icloud_new_copy_admission_met, None);
    assert!(!envelope.cloud_write_executed);
    assert!(!envelope.source_eviction_authorized);
}

#[test]
fn all_clear_candidates_export_ready_without_new_review() {
    let plan = report(vec![
        candidate("embedded:exiftool:CreateDate", false, 'd'),
        candidate("filesystem:created", false, 'e'),
    ]);
    let runtime = assess_provider_client_runtime(
        CloudProvider::Onedrive,
        Some(b"OneDrive Sync Service\n"),
        25,
    );
    let global_sync = clear_global_sync();

    let envelope = export_naruon_cloud_copy_readiness_with_global_sync(
        &plan,
        &runtime,
        None,
        Some(&global_sync),
    )
    .unwrap();

    assert_eq!(
        envelope.readiness_state,
        CloudCopyReadinessState::ReadyWithoutNewReview
    );
    assert_eq!(envelope.ready_without_new_review.count, 2);
    assert_eq!(envelope.ready_without_new_review.bytes, 84);
    assert!(envelope.candidate_blocker_counts.is_empty());
    assert!(envelope.provider_runtime_prerequisite_met);
    assert!(envelope.remote_capacity_verified);
    assert!(!envelope.cloud_write_executed);
    assert!(!envelope.source_eviction_authorized);
}
