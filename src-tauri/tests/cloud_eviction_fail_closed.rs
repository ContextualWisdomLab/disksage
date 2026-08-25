use disksage_lib::cloud::{
    candidate_review_fingerprint, ArchiveKind, CloudAccountScope, CloudCandidate, CloudProvider,
    CloudRoot, MetadataEvidence,
};
use disksage_lib::cloud_eviction::{
    create_source_eviction_approval, evict_source_with_human_approval,
};
use disksage_lib::cloud_local_eviction::ActiveUseEvidence;
use disksage_lib::cloud_transfer::{
    approve_local_eviction, cloud_copy_approval_phrase, create_cloud_copy_approval,
    prepare_cloud_copy_with_approval, CloudCopyApprovalAction, ProviderSyncEvidence,
    SyncEvidenceKind,
};
use disksage_lib::provider_evidence::create_sync_evidence_record;
use std::path::Path;

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

fn valid_receipt(
    temp: &tempfile::TempDir,
) -> (
    disksage_lib::cloud_transfer::CloudCopyReceipt,
    disksage_lib::cloud_transfer::LocalEvictionPermit,
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
    let approval_time = now_ms();
    let action = CloudCopyApprovalAction::CopyOnly;
    let copy_approval = create_cloud_copy_approval(
        &candidate,
        &root,
        action,
        approval_time,
        "human:local:test",
        "authorize exact test cloud copy",
        &cloud_copy_approval_phrase(&candidate, action),
    )
    .unwrap();
    let (receipt, _) =
        prepare_cloud_copy_with_approval(&candidate, &root, &receipt_dir, None, &copy_approval)
            .unwrap();
    let evidence = ProviderSyncEvidence {
        receipt_id: receipt.receipt_id.clone(),
        provider: receipt.provider,
        destination: receipt.destination.clone(),
        observed_bytes: receipt.bytes,
        destination_blake3: receipt.blake3.clone(),
        confirmed_at_ms: receipt.copied_at_ms + 1,
        kind: SyncEvidenceKind::ProviderNativeStatus,
        evidence_id: "native-test-evidence".into(),
        sync_complete: true,
        sync_state: disksage_lib::cloud_transfer::ProviderSyncState::Complete,
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

fn staging_dir(receipt: &disksage_lib::cloud_transfer::CloudCopyReceipt) -> std::path::PathBuf {
    Path::new(&receipt.source)
        .parent()
        .unwrap()
        .join(format!(".disksage-evict-{}", receipt.receipt_id))
}

#[test]
fn production_cloud_eviction_fails_closed_without_identity_bound_recycle() {
    let temp = tempfile::tempdir().unwrap();
    let (receipt, permit) = valid_receipt(&temp);
    let source = Path::new(&receipt.source);
    let original = std::fs::read(source).unwrap();
    let observed_at_ms = permit.approved_at_ms + 1;
    let approved_at_ms = observed_at_ms + 1;
    let approval = create_source_eviction_approval(
        &receipt,
        &permit,
        &receipt.receipt_id,
        approved_at_ms,
        "human:local:test",
        "verified cloud copy; move only this source to Trash",
        observed_at_ms,
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
        approved_at_ms + 1,
    )
    .unwrap_err();

    assert_eq!(error, "source-eviction-identity-bound-recycle-unavailable");
    assert!(source.exists());
    assert_eq!(std::fs::read(source).unwrap(), original);
    assert!(!staging_dir(&receipt).exists());
    assert!(!temp.path().join("evictions").exists());
    assert!(!temp.path().join("journal").exists());
}

#[test]
fn invalid_confirmation_is_rejected_before_capability_gate_without_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let (receipt, permit) = valid_receipt(&temp);
    let source = Path::new(&receipt.source);
    let original = std::fs::read(source).unwrap();
    let observed_at_ms = permit.approved_at_ms + 1;
    let approved_at_ms = observed_at_ms + 1;
    let approval = create_source_eviction_approval(
        &receipt,
        &permit,
        &receipt.receipt_id,
        approved_at_ms,
        "human:local:test",
        "verified cloud copy; move only this source to Trash",
        observed_at_ms,
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
        approved_at_ms + 1,
    )
    .unwrap_err();

    assert_eq!(error, "eviction-confirmation-receipt-id-mismatch");
    assert!(source.exists());
    assert_eq!(std::fs::read(source).unwrap(), original);
    assert!(!staging_dir(&receipt).exists());
    assert!(!temp.path().join("evictions").exists());
    assert!(!temp.path().join("journal").exists());
}
