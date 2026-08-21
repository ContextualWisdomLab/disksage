use disksage_lib::cloud::{
    ArchiveKind, CloudAccountScope, CloudCandidate, CloudPlanOptions, CloudPlanReport, CloudProvider,
    CloudRoot, ExactDuplicateSummary, MetadataEvidence,
};
use disksage_lib::naruon_cloud_copy_readiness::export_naruon_cloud_copy_readiness;
use disksage_lib::provider_capacity::{
    assess_capacity, unavailable_capacity, DEFAULT_CAPACITY_RESERVE_BYTES,
};
use disksage_lib::provider_client_runtime::assess_provider_client_runtime;

#[test]
fn non_icloud_readiness_rejects_indexing_pending_blocker_namespace() {
    let provider = CloudProvider::Onedrive;
    let mut candidate = CloudCandidate {
        metadata_fingerprint: "a".repeat(64),
        review_fingerprint: String::new(),
        src: "/private/source/report.pdf".into(),
        dst: "/private/cloud/DiskSage Archive/2026/08/document/report.pdf".into(),
        provider,
        destination_account_scope: CloudAccountScope::Personal,
        kind: ArchiveKind::Document,
        bytes: 42,
        age_days: 100,
        created_ms: 1,
        modified_ms: 2,
        production_time_ms: 3,
        production_time_source: "filesystem:created".into(),
        production_time_confidence: "low".into(),
        source_root: "/private/source".into(),
        relative_path: "report.pdf".into(),
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
            value: "2026-08-21".into(),
            source: "filesystem:created".into(),
            confidence: "low".into(),
        }],
        blocked_reason: Some("icloud-file-provider-indexing-pending".into()),
    };
    candidate.review_fingerprint = disksage_lib::cloud::candidate_review_fingerprint(&candidate);

    let snapshot = unavailable_capacity(provider, 10, "capacity-unavailable");
    let report = CloudPlanReport {
        cloud_root: CloudRoot {
            id: "onedrive-test-root".into(),
            provider,
            account_scope: CloudAccountScope::Personal,
            label: "OneDrive".into(),
            path: "/private/cloud".into(),
            readable: true,
            access_issue: None,
        },
        generated_at_ms: 20,
        source_selection_policy: Some(CloudPlanOptions {
            min_size_bytes: 1,
            min_age_days: 1,
            limit: 10,
        }),
        candidates: vec![candidate],
        candidate_bytes: 42,
        potentially_reclaimable_bytes: 0,
        exact_duplicates: ExactDuplicateSummary::default(),
        capacity: Some(assess_capacity(
            snapshot,
            0,
            42,
            DEFAULT_CAPACITY_RESERVE_BYTES,
        )),
        local_volume: None,
        pre_copy_evidence: None,
        notices: Vec::new(),
    };
    let runtime = assess_provider_client_runtime(
        provider,
        Some(b"OneDrive Sync Service\n"),
        25,
    );

    assert_eq!(
        export_naruon_cloud_copy_readiness(&report, &runtime, None).unwrap_err(),
        "naruon-copy-readiness-icloud-binding-invalid"
    );
}
