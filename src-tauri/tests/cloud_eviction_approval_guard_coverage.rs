use disksage_lib::cloud::{
    candidate_review_fingerprint, ArchiveKind, CloudAccountScope, CloudCandidate, CloudProvider,
    CloudRoot, MetadataEvidence,
};
use disksage_lib::cloud_eviction::create_source_eviction_approval;
use disksage_lib::cloud_local_eviction::ActiveUseEvidence;
use disksage_lib::cloud_transfer::{
    approve_local_eviction, cloud_copy_approval_phrase, create_cloud_copy_approval,
    prepare_cloud_copy_with_approval, CloudCopyApprovalAction, ProviderSyncEvidence,
    SyncEvidenceKind,
};
use disksage_lib::provider_evidence::create_sync_evidence_record;

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

fn valid_receipt_and_permit(
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

    let source = source_dir.join("approval-guard.bin");
    let destination = cloud_dir.join("approval-guard.bin");
    std::fs::write(&source, b"approval guard coverage bytes").unwrap();
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
        relative_path: "approval-guard.bin".into(),
        source_context: ".".into(),
        requires_review: false,
        review_reasons: Vec::new(),
        content_title: Some("Approval guard coverage".into()),
        content_authors: Vec::new(),
        content_context: Vec::new(),
        duration_ms: None,
        dataset_profile: None,
        metadata_evidence: vec![MetadataEvidence {
            field: "production-date".into(),
            value: "2026-08-16".into(),
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
        label: "coverage".into(),
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
        "human:local:coverage",
        "authorize exact coverage copy",
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
        evidence_id: "approval-guard-native-evidence".into(),
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
fn source_eviction_approval_rejects_each_unsafe_active_use_shape() {
    let temp = tempfile::tempdir().unwrap();
    let (receipt, permit) = valid_receipt_and_permit(&temp);
    let observed_at_ms = permit.approved_at_ms + 1;
    let approved_at_ms = observed_at_ms + 1;

    let mut cases = Vec::new();

    let mut wrong_method = idle_active_use();
    wrong_method.method = "unknown".into();
    cases.push(wrong_method);

    let mut incomplete = idle_active_use();
    incomplete.evidence_complete = false;
    cases.push(incomplete);

    let mut active = idle_active_use();
    active.active = true;
    cases.push(active);

    let mut pids = idle_active_use();
    pids.observed_pids = vec![1234];
    cases.push(pids);

    let mut truncated = idle_active_use();
    truncated.results_truncated = true;
    cases.push(truncated);

    let mut errored = idle_active_use();
    errored.error = Some("probe-incomplete".into());
    cases.push(errored);

    for evidence in cases {
        let error = create_source_eviction_approval(
            &receipt,
            &permit,
            &receipt.receipt_id,
            approved_at_ms,
            "human:local:coverage",
            "authorize exact source eviction after idle-use proof",
            observed_at_ms,
            evidence,
        )
        .unwrap_err();
        assert_eq!(error, "source-eviction-active-use-evidence-invalid");
    }
}

#[test]
fn source_eviction_approval_rejects_stale_or_future_active_use_evidence() {
    let temp = tempfile::tempdir().unwrap();
    let (receipt, permit) = valid_receipt_and_permit(&temp);
    let approved_at_ms = permit.approved_at_ms + 10;

    let stale = create_source_eviction_approval(
        &receipt,
        &permit,
        &receipt.receipt_id,
        approved_at_ms,
        "human:local:coverage",
        "authorize exact source eviction after fresh idle-use proof",
        permit.approved_at_ms.saturating_sub(1),
        idle_active_use(),
    )
    .unwrap_err();
    assert_eq!(stale, "source-eviction-active-use-evidence-invalid");

    let future = create_source_eviction_approval(
        &receipt,
        &permit,
        &receipt.receipt_id,
        approved_at_ms,
        "human:local:coverage",
        "authorize exact source eviction after fresh idle-use proof",
        approved_at_ms + 1,
        idle_active_use(),
    )
    .unwrap_err();
    assert_eq!(future, "source-eviction-active-use-evidence-invalid");
}

#[test]
fn source_eviction_approval_rejects_bad_confirmation_and_attribution() {
    let temp = tempfile::tempdir().unwrap();
    let (receipt, permit) = valid_receipt_and_permit(&temp);
    let observed_at_ms = permit.approved_at_ms + 1;
    let approved_at_ms = observed_at_ms + 1;

    let confirmation_error = create_source_eviction_approval(
        &receipt,
        &permit,
        "different-receipt",
        approved_at_ms,
        "human:local:coverage",
        "authorize exact source eviction after fresh idle-use proof",
        observed_at_ms,
        idle_active_use(),
    )
    .unwrap_err();
    assert_eq!(confirmation_error, "eviction-confirmation-receipt-id-mismatch");

    let actor_error = create_source_eviction_approval(
        &receipt,
        &permit,
        &receipt.receipt_id,
        approved_at_ms,
        "",
        "authorize exact source eviction after fresh idle-use proof",
        observed_at_ms,
        idle_active_use(),
    )
    .unwrap_err();
    assert_eq!(
        actor_error,
        "source-eviction-human-approval-attribution-invalid"
    );

    let rationale_error = create_source_eviction_approval(
        &receipt,
        &permit,
        &receipt.receipt_id,
        approved_at_ms,
        "human:local:coverage",
        "",
        observed_at_ms,
        idle_active_use(),
    )
    .unwrap_err();
    assert_eq!(
        rationale_error,
        "source-eviction-human-approval-attribution-invalid"
    );
}
