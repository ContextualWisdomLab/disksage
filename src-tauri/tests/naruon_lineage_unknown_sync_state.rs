use disksage_lib::cloud::{
    candidate_review_fingerprint, ArchiveKind, CloudAccountScope, CloudCandidate, CloudProvider,
    CloudRoot, MetadataEvidence,
};
use disksage_lib::cloud_review::{create_attributed_decision, CloudReviewDisposition};
use disksage_lib::cloud_transfer::{
    cloud_copy_approval_phrase, create_cloud_copy_approval, prepare_cloud_copy_with_approval,
    CloudCopyApprovalAction, ProviderSyncEvidence, ProviderSyncState, SyncEvidenceKind,
};
use disksage_lib::naruon_lineage::export_naruon_file_lineage;
use disksage_lib::provider_evidence::create_sync_evidence_record;

#[test]
fn legacy_unknown_sync_state_never_exports_confirmed_provider_sync() {
    let tmp = tempfile::tempdir().unwrap();
    let source_root = tmp.path().join("source");
    let source = source_root.join("reports/report.pdf");
    let cloud = tmp.path().join("cloud");
    let destination = cloud.join("DiskSage Archive/reports/report.pdf");
    std::fs::create_dir_all(source.parent().unwrap()).unwrap();
    std::fs::create_dir_all(&cloud).unwrap();
    std::fs::write(&source, b"legacy-unknown-sync-state").unwrap();
    let metadata = std::fs::metadata(&source).unwrap();
    let modified_ms = metadata
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;

    let mut candidate = CloudCandidate {
        metadata_fingerprint: "b".repeat(64),
        review_fingerprint: String::new(),
        src: source.to_string_lossy().into_owned(),
        dst: destination.to_string_lossy().into_owned(),
        provider: CloudProvider::GoogleDrive,
        destination_account_scope: CloudAccountScope::Organization,
        kind: ArchiveKind::Document,
        bytes: metadata.len(),
        age_days: 90,
        created_ms: 10,
        modified_ms,
        production_time_ms: 5,
        production_time_source: "embedded:exiftool:CreateDate".into(),
        production_time_confidence: "high".into(),
        source_root: source_root.to_string_lossy().into_owned(),
        relative_path: "reports/report.pdf".into(),
        source_context: "download".into(),
        requires_review: true,
        review_reasons: vec!["sensitive-document".into()],
        content_title: Some("Report".into()),
        content_authors: vec!["Author".into()],
        content_context: vec!["Context".into()],
        duration_ms: None,
        dataset_profile: None,
        metadata_evidence: vec![MetadataEvidence {
            field: "production-date".into(),
            value: "2026-01-01".into(),
            source: "embedded:exiftool:CreateDate".into(),
            confidence: "high".into(),
        }],
        blocked_reason: None,
    };
    candidate.review_fingerprint = candidate_review_fingerprint(&candidate);
    let decision = create_attributed_decision(
        &candidate,
        CloudReviewDisposition::Approved,
        25,
        "human:local:test",
        "[organization-tenant-authority-confirmed] embedded metadata checked",
    )
    .unwrap();
    let root = CloudRoot {
        id: "google-drive:test".into(),
        provider: CloudProvider::GoogleDrive,
        account_scope: CloudAccountScope::Organization,
        label: "Google Drive".into(),
        path: cloud.to_string_lossy().into_owned(),
        readable: true,
        access_issue: None,
    };

    let approval_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let copy_approval = create_cloud_copy_approval(
        &candidate,
        &root,
        CloudCopyApprovalAction::CopyOnly,
        approval_time,
        "human:local:test",
        "authorize exact test cloud copy",
        &cloud_copy_approval_phrase(&candidate, CloudCopyApprovalAction::CopyOnly),
    )
    .unwrap();
    let receipt = prepare_cloud_copy_with_approval(
        &candidate,
        &root,
        &tmp.path().join("receipts"),
        Some(&decision),
        &copy_approval,
    )
    .unwrap()
    .0;
    let record = create_sync_evidence_record(&ProviderSyncEvidence {
        receipt_id: receipt.receipt_id.clone(),
        provider: receipt.provider,
        destination: receipt.destination.clone(),
        observed_bytes: receipt.bytes,
        destination_blake3: receipt.blake3.clone(),
        confirmed_at_ms: 40,
        kind: SyncEvidenceKind::ProviderNativeStatus,
        evidence_id: format!("legacy-provider-state:{}", "1".repeat(64)),
        sync_complete: true,
        sync_state: ProviderSyncState::Unknown,
        remote_content: None,
    })
    .unwrap();

    let error = export_naruon_file_lineage(&receipt, Some(&record)).unwrap_err();
    assert_eq!(
        error, "provider-sync-incomplete",
        "legacy evidence without an explicit complete sync state must fail closed"
    );
}
