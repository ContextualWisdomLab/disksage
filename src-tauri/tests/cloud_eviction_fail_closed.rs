use disksage::cloud::{
    candidate_review_fingerprint, ArchiveKind, CloudAccountScope, CloudCandidate, CloudProvider,
    CloudRoot, MetadataEvidence,
};
use disksage::cloud_eviction::{
    create_source_eviction_approval, evict_source_with_human_approval,
};
use disksage::cloud_local_eviction::ActiveUseEvidence;
use disksage::cloud_transfer::{
    approve_local_eviction, prepare_cloud_copy, ProviderSyncEvidence, SyncEvidenceKind,
};
use disksage::provider_evidence::create_sync_evidence_record;
use std::path::Path;

fn valid_receipt(
    temp: &tempfile::TempDir,
) -> (
    disksage::cloud_transfer::CloudCopyReceipt,
    disksage::cloud_transfer::LocalEvictionPermit,
) {
    let source_dir = temp.path().join("source");
    let cloud_dir = temp.path().join("cloud");
    let receipt_dir = temp.path().join("receipts");
    std::fs::create_dir_all(&source_dir).unwrap();
    std::fs::create_dir_all(&cloud_dir).unwrap();

    let source = source_dir.join("report.bin");
    let destination = cloud_dir.join("report.bin");
    std::fs::write(&source, b"verified source bytes").unwrap();
    let metadata = std::fs::metadata(&source).unwrap();
    let modified_ms = metadata
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;

    let mut candidate = CloudCandidate {
        metadata_fingerprint: "a".repeat(64),
        review_fingerprint: String::new(),
        src: source.to_string_lossy().into_owned(),
        dst: destination.to_string_lossy().into_owned(),
        provider: CloudProvider::Onedrive,
        destination_account_scope: CloudAccountScope::Personal,
        kind: ArchiveKind::Document,
        bytes: metadata.len(),
        age_days: 1,
        created_ms: modified_ms,
        modified_ms,
        production_time_ms: modified_ms,
        production_time_source: "embedded:test:CreateDate".into(),
        production_time_confidence: "high".into(),
        source_root: source_dir.to_string_lossy().into_owned(),
        relative_path: "report.bin".into(),
        source_context: ".".into(),
        requires_review: false,
        review_reasons: Vec::new(),
        content_title: Some("Report".into()),
        content_authors: Vec::new(),
        content_context: Vec::new(),
        duration_ms: None,
        dataset_profile: None,
        metadata_evidence: vec![MetadataEvidence {
            field: "production-date".into(),
            value: "2026-07-17".into(),
            source: "embedded:test:CreateDate".into(),
            confidence: "high".into(),
        }],
        blocked_reason: None,
    };
    candidate.review_fingerprint = candidate_review_fingerprint(&candidate);

    let root = CloudRoot {
        id: cloud_dir.to_string_lossy().into_owned(),
        provider: CloudProvider::Onedrive,
        account_scope: CloudAccountScope::Personal,
        label: "test".into(),
        path: cloud_dir.to_string_lossy().into_owned(),
        readable: true,
        access_issue: None,
    };
    let (receipt, _) = prepare_cloud_copy(&candidate, &root, &receipt_dir, 100).unwrap();
    let evidence = ProviderSyncEvidence {
        receipt_id: receipt.receipt_id.clone(),
        provider: receipt.provider,
        destination: receipt.destination.clone(),
        observed_bytes: receipt.bytes,
        destination_blake3: receipt.blake3.clone(),
        confirmed_at_ms: 101,
        kind: SyncEvidenceKind::ProviderNativeStatus,
        evidence_id: "native-test-evidence".into(),
        sync_complete: true,
        remote_content: None,
    };
    let evidence_record = create_sync_evidence_record(&evidence).unwrap();
    let permit = approve_local_eviction(&receipt, &evidence_record).unwrap();
    (receipt, permit)
}

fn idle_active_use() -> ActiveUseEvidence {
    ActiveUseEvidence {
        method: "lsof-fp+ps-command".into(),
        evidence_complete: true,
        active: false,
        observed_pids: Vec::new(),
        results_truncated: false,
        error: None,
    }
}

#[test]
fn production_cloud_eviction_fails_closed_without_identity_bound_recycle() {
    let temp = tempfile::tempdir().unwrap();
    let (receipt, permit) = valid_receipt(&temp);
    let source = Path::new(&receipt.source);
    let original = std::fs::read(source).unwrap();
    let approval = create_source_eviction_approval(
        &receipt,
        &permit,
        &receipt.receipt_id,
        160,
        "human:local:test",
        "verified cloud copy; move only this source to Trash",
        150,
        idle_active_use(),
    )
    .unwrap();

    let error = evict_source_with_human_approval(
        &receipt,
        &permit,
        &approval,
        &receipt.receipt_id,
        &temp.path().join("evictions"),
        &temp.path().join("journal/operations.jsonl"),
        200,
    )
    .unwrap_err();

    assert_eq!(error, "source-eviction-identity-bound-recycle-unavailable");
    assert!(source.exists());
    assert_eq!(std::fs::read(source).unwrap(), original);
    assert!(!temp.path().join("evictions").exists());
    assert!(!temp.path().join("journal").exists());
}

#[test]
fn invalid_confirmation_is_rejected_before_capability_gate_without_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let (receipt, permit) = valid_receipt(&temp);
    let source = Path::new(&receipt.source);
    let original = std::fs::read(source).unwrap();
    let approval = create_source_eviction_approval(
        &receipt,
        &permit,
        &receipt.receipt_id,
        160,
        "human:local:test",
        "verified cloud copy; move only this source to Trash",
        150,
        idle_active_use(),
    )
    .unwrap();

    let error = evict_source_with_human_approval(
        &receipt,
        &permit,
        &approval,
        &"0".repeat(64),
        &temp.path().join("evictions"),
        &temp.path().join("journal/operations.jsonl"),
        200,
    )
    .unwrap_err();

    assert_eq!(error, "eviction-confirmation-receipt-id-mismatch");
    assert!(source.exists());
    assert_eq!(std::fs::read(source).unwrap(), original);
    assert!(!temp.path().join("evictions").exists());
    assert!(!temp.path().join("journal").exists());
}
