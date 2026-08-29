//! Regression coverage for legacy provider evidence at the destructive eviction boundary.
//!
//! Historical provider evidence can deserialize with `sync_complete=true` while the newer
//! explicit `sync_state` field defaults to `unknown`. Such evidence may remain readable for
//! compatibility, but it must never authorize source eviction.

use disksage_lib::cloud::{
    candidate_review_fingerprint, ArchiveKind, CloudAccountScope, CloudCandidate, CloudProvider,
    CloudRoot, MetadataEvidence,
};
use disksage_lib::cloud_transfer::{
    approve_local_eviction, cloud_copy_approval_phrase, create_cloud_copy_approval,
    prepare_provider_api_source_receipt, CloudCopyApprovalAction, ProviderSyncEvidence,
    ProviderSyncState, SyncEvidenceKind,
};
use disksage_lib::provider_evidence::create_sync_evidence_record;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn legacy_unknown_sync_state_cannot_authorize_eviction_permit() {
    let tmp = tempfile::tempdir().expect("create isolated filesystem fixture");
    let source_root = tmp.path().join("source");
    let source = source_root.join("report.pdf");
    let cloud_root_path = tmp.path().join("cloud");
    let destination = cloud_root_path.join("DiskSage Archive/report.pdf");
    std::fs::create_dir_all(&source_root).expect("create source directory");
    std::fs::create_dir_all(&cloud_root_path).expect("create cloud directory");
    std::fs::write(&source, b"legacy-sync-state-regression").expect("write source fixture");

    let metadata = std::fs::metadata(&source).expect("read source metadata");
    let modified_ms = metadata
        .modified()
        .expect("source modified time")
        .duration_since(UNIX_EPOCH)
        .expect("post-epoch source time")
        .as_millis() as u64;
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("post-epoch test clock")
        .as_millis() as u64;

    let mut candidate = CloudCandidate {
        metadata_fingerprint: "a".repeat(64),
        review_fingerprint: String::new(),
        src: source.to_string_lossy().into_owned(),
        dst: destination.to_string_lossy().into_owned(),
        provider: CloudProvider::Icloud,
        destination_account_scope: CloudAccountScope::Personal,
        kind: ArchiveKind::Document,
        bytes: metadata.len(),
        age_days: 90,
        created_ms: modified_ms,
        modified_ms,
        production_time_ms: modified_ms,
        production_time_source: "embedded:integration-fixture:CreateDate".into(),
        production_time_confidence: "high".into(),
        source_root: source_root.to_string_lossy().into_owned(),
        relative_path: "report.pdf".into(),
        source_context: "integration-fixture".into(),
        requires_review: false,
        review_reasons: Vec::new(),
        content_title: Some("Legacy sync state regression".into()),
        content_authors: Vec::new(),
        content_context: Vec::new(),
        duration_ms: None,
        dataset_profile: None,
        metadata_evidence: vec![MetadataEvidence {
            field: "production_time".into(),
            value: modified_ms.to_string(),
            source: "embedded:integration-fixture:CreateDate".into(),
            confidence: "high".into(),
        }],
        blocked_reason: None,
    };
    candidate.review_fingerprint = candidate_review_fingerprint(&candidate);

    let cloud_root = CloudRoot {
        id: "icloud:legacy-eviction-regression".into(),
        provider: CloudProvider::Icloud,
        account_scope: CloudAccountScope::Personal,
        label: "iCloud Drive".into(),
        path: cloud_root_path.to_string_lossy().into_owned(),
        readable: true,
        access_issue: None,
    };
    let action = CloudCopyApprovalAction::CopyOnly;
    let approval = create_cloud_copy_approval(
        &candidate,
        &cloud_root,
        action,
        now_ms,
        "human:integration-reviewer",
        "Exact source and destination reviewed for the regression fixture.",
        &cloud_copy_approval_phrase(&candidate, action),
    )
    .expect("create exact copy approval");
    let (receipt, _) = prepare_provider_api_source_receipt(
        &candidate,
        &cloud_root,
        None,
        &approval,
        now_ms,
    )
    .expect("create production receipt through the public boundary");

    let legacy_evidence = ProviderSyncEvidence {
        receipt_id: receipt.receipt_id.clone(),
        provider: receipt.provider,
        destination: receipt.destination.clone(),
        observed_bytes: receipt.bytes,
        destination_blake3: receipt.blake3.clone(),
        confirmed_at_ms: now_ms.saturating_add(1),
        kind: SyncEvidenceKind::ProviderNativeStatus,
        evidence_id: "legacy-provider-native-status".into(),
        sync_complete: true,
        sync_state: ProviderSyncState::Unknown,
        remote_content: None,
    };
    let record = create_sync_evidence_record(&legacy_evidence)
        .expect("legacy unknown-state evidence remains readable and integrity-bound");

    let blockers = approve_local_eviction(&receipt, &record)
        .expect_err("unknown sync state must never authorize source eviction");
    assert!(
        blockers.iter().any(|blocker| blocker == "provider-sync-incomplete"),
        "legacy unknown sync state must fail closed at the eviction permit boundary: {blockers:?}"
    );
}
