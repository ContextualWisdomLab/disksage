use disksage_lib::cloud::{
    candidate_review_fingerprint, ArchiveKind, CloudAccountScope, CloudCandidate, CloudProvider,
    CloudRoot, MetadataEvidence,
};
use disksage_lib::cloud_eviction::{
    create_source_eviction_approval, evict_source_with_human_approval,
    write_immutable_source_eviction_approval,
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

fn valid_approval(
    receipt: &disksage_lib::cloud_transfer::CloudCopyReceipt,
    permit: &disksage_lib::cloud_transfer::LocalEvictionPermit,
) -> disksage_lib::cloud_eviction::CloudSourceEvictionApproval {
    let observed_at_ms = permit.approved_at_ms + 1;
    create_source_eviction_approval(
        receipt,
        permit,
        &receipt.receipt_id,
        observed_at_ms + 1,
        "human:local:test",
        "verified cloud copy; move only this source to Trash",
        observed_at_ms,
        idle_active_use(),
    )
    .unwrap()
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
    let approval = valid_approval(&receipt, &permit);

    let error = evict_source_with_human_approval(
        &receipt,
        &permit,
        &approval,
        &receipt.receipt_id,
        &temp.path().join("evictions"),
        &temp.path().join("journal/operations.jsonl"),
        approval.approved_at_ms + 1,
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
    let approval = valid_approval(&receipt, &permit);

    let error = evict_source_with_human_approval(
        &receipt,
        &permit,
        &approval,
        &"0".repeat(64),
        &temp.path().join("evictions"),
        &temp.path().join("journal/operations.jsonl"),
        approval.approved_at_ms + 1,
    )
    .unwrap_err();

    assert_eq!(error, "eviction-confirmation-receipt-id-mismatch");
    assert!(source.exists());
    assert_eq!(std::fs::read(source).unwrap(), original);
    assert!(!staging_dir(&receipt).exists());
    assert!(!temp.path().join("evictions").exists());
    assert!(!temp.path().join("journal").exists());
}

#[test]
fn approval_creation_rejects_each_active_use_and_time_boundary() {
    let temp = tempfile::tempdir().unwrap();
    let (receipt, permit) = valid_receipt(&temp);
    let approved_at_ms = permit.approved_at_ms + 10;

    let assert_rejected = |active_use: ActiveUseEvidence, observed_at_ms: u64| {
        assert_eq!(
            create_source_eviction_approval(
                &receipt,
                &permit,
                &receipt.receipt_id,
                approved_at_ms,
                "human:local:test",
                "specific receipt-bound source approval",
                observed_at_ms,
                active_use,
            )
            .unwrap_err(),
            "source-eviction-active-use-evidence-invalid"
        );
    };

    let mut wrong_method = idle_active_use();
    wrong_method.method = "ps-only".into();
    assert_rejected(wrong_method, permit.approved_at_ms + 1);

    let mut incomplete = idle_active_use();
    incomplete.evidence_complete = false;
    assert_rejected(incomplete, permit.approved_at_ms + 1);

    let mut active = idle_active_use();
    active.active = true;
    assert_rejected(active, permit.approved_at_ms + 1);

    let mut pids_without_active = idle_active_use();
    pids_without_active.observed_pids = vec![4242];
    assert_rejected(pids_without_active, permit.approved_at_ms + 1);

    let mut truncated = idle_active_use();
    truncated.results_truncated = true;
    assert_rejected(truncated, permit.approved_at_ms + 1);

    let mut errored = idle_active_use();
    errored.error = Some("lsof-observation-failed".into());
    assert_rejected(errored, permit.approved_at_ms + 1);

    assert_rejected(
        idle_active_use(),
        permit.approved_at_ms.saturating_sub(1),
    );
    assert_rejected(idle_active_use(), approved_at_ms + 1);
}

#[test]
fn approval_creation_rejects_confirmation_attribution_and_permit_drift() {
    let temp = tempfile::tempdir().unwrap();
    let (receipt, permit) = valid_receipt(&temp);
    let observed_at_ms = permit.approved_at_ms + 1;
    let approved_at_ms = observed_at_ms + 1;

    assert_eq!(
        create_source_eviction_approval(
            &receipt,
            &permit,
            &"0".repeat(64),
            approved_at_ms,
            "human:local:test",
            "specific receipt-bound source approval",
            observed_at_ms,
            idle_active_use(),
        )
        .unwrap_err(),
        "eviction-confirmation-receipt-id-mismatch"
    );
    assert_eq!(
        create_source_eviction_approval(
            &receipt,
            &permit,
            &receipt.receipt_id,
            approved_at_ms,
            "agent:test",
            "specific receipt-bound source approval",
            observed_at_ms,
            idle_active_use(),
        )
        .unwrap_err(),
        "source-eviction-human-approval-attribution-invalid"
    );

    for mut drifted in [permit.clone(), permit.clone(), permit.clone()] {
        let expected = if drifted.receipt_id == permit.receipt_id {
            if drifted.source == permit.source {
                drifted.bytes = drifted.bytes.saturating_add(1);
            } else {
                unreachable!()
            }
        } else {
            unreachable!()
        };
        let _ = expected;
        assert_eq!(
            create_source_eviction_approval(
                &receipt,
                &drifted,
                &receipt.receipt_id,
                approved_at_ms,
                "human:local:test",
                "specific receipt-bound source approval",
                observed_at_ms,
                idle_active_use(),
            )
            .unwrap_err(),
            "eviction-permit-receipt-mismatch"
        );
    }

    let mut receipt_id_drift = permit.clone();
    receipt_id_drift.receipt_id = "0".repeat(64);
    assert_eq!(
        create_source_eviction_approval(
            &receipt,
            &receipt_id_drift,
            &receipt.receipt_id,
            approved_at_ms,
            "human:local:test",
            "specific receipt-bound source approval",
            observed_at_ms,
            idle_active_use(),
        )
        .unwrap_err(),
        "eviction-permit-receipt-mismatch"
    );

    let mut source_drift = permit.clone();
    source_drift.source.push_str(".different");
    assert_eq!(
        create_source_eviction_approval(
            &receipt,
            &source_drift,
            &receipt.receipt_id,
            approved_at_ms,
            "human:local:test",
            "specific receipt-bound source approval",
            observed_at_ms,
            idle_active_use(),
        )
        .unwrap_err(),
        "eviction-permit-receipt-mismatch"
    );

    let mut early_permit = permit.clone();
    early_permit.approved_at_ms = receipt.copied_at_ms.saturating_sub(1);
    assert_eq!(
        create_source_eviction_approval(
            &receipt,
            &early_permit,
            &receipt.receipt_id,
            approved_at_ms,
            "human:local:test",
            "specific receipt-bound source approval",
            observed_at_ms,
            idle_active_use(),
        )
        .unwrap_err(),
        "eviction-permit-invalid"
    );

    let mut missing_evidence = permit.clone();
    missing_evidence.evidence_id = "   ".into();
    assert_eq!(
        create_source_eviction_approval(
            &receipt,
            &missing_evidence,
            &receipt.receipt_id,
            approved_at_ms,
            "human:local:test",
            "specific receipt-bound source approval",
            observed_at_ms,
            idle_active_use(),
        )
        .unwrap_err(),
        "eviction-permit-invalid"
    );

    let mut malformed_evidence_record = permit.clone();
    malformed_evidence_record.evidence_record_id = "not-a-hex64-record-id".into();
    assert_eq!(
        create_source_eviction_approval(
            &receipt,
            &malformed_evidence_record,
            &receipt.receipt_id,
            approved_at_ms,
            "human:local:test",
            "specific receipt-bound source approval",
            observed_at_ms,
            idle_active_use(),
        )
        .unwrap_err(),
        "eviction-permit-invalid"
    );
}

#[test]
fn tampered_human_approval_is_rejected_before_filesystem_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let (receipt, permit) = valid_receipt(&temp);
    let source = Path::new(&receipt.source);
    let original = std::fs::read(source).unwrap();
    let approval = valid_approval(&receipt, &permit);
    let eviction_dir = temp.path().join("evictions");
    let journal_path = temp.path().join("journal/operations.jsonl");

    let mut variants = Vec::new();

    let mut version = approval.clone();
    version.version = version.version.saturating_add(1);
    variants.push(version);

    let mut receipt_id = approval.clone();
    receipt_id.receipt_id = "0".repeat(64);
    variants.push(receipt_id);

    let mut evidence_record_id = approval.clone();
    evidence_record_id.evidence_record_id = "f".repeat(64);
    variants.push(evidence_record_id);

    let mut early_approval = approval.clone();
    early_approval.approved_at_ms = permit.approved_at_ms.saturating_sub(1);
    variants.push(early_approval);

    let mut early_observation = approval.clone();
    early_observation.active_use_observed_at_ms = permit.approved_at_ms.saturating_sub(1);
    variants.push(early_observation);

    let mut future_observation = approval.clone();
    future_observation.active_use_observed_at_ms = future_observation.approved_at_ms + 1;
    variants.push(future_observation);

    let mut unsafe_evidence = approval.clone();
    unsafe_evidence.active_use.results_truncated = true;
    variants.push(unsafe_evidence);

    let mut bad_id = approval.clone();
    bad_id.approval_id = "0".repeat(64);
    variants.push(bad_id);

    for tampered in variants {
        assert_eq!(
            evict_source_with_human_approval(
                &receipt,
                &permit,
                &tampered,
                &receipt.receipt_id,
                &eviction_dir,
                &journal_path,
                approval.approved_at_ms + 1,
            )
            .unwrap_err(),
            "source-eviction-human-approval-invalid"
        );
        assert_eq!(std::fs::read(source).unwrap(), original);
        assert!(!staging_dir(&receipt).exists());
        assert!(!eviction_dir.exists());
        assert!(!journal_path.parent().unwrap().exists());
    }
}

#[test]
fn immutable_approval_publication_is_create_new_and_rejects_invalid_records() {
    let temp = tempfile::tempdir().unwrap();
    let (receipt, permit) = valid_receipt(&temp);
    let approval = valid_approval(&receipt, &permit);
    let approval_dir = temp.path().join("approvals");

    let path = write_immutable_source_eviction_approval(&approval_dir, &approval).unwrap();
    let before = std::fs::read(&path).unwrap();
    assert!(path.metadata().unwrap().permissions().readonly());
    assert!(write_immutable_source_eviction_approval(&approval_dir, &approval).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);

    let invalid_cases = [
        {
            let mut value = approval.clone();
            value.version = value.version.saturating_add(1);
            value
        },
        {
            let mut value = approval.clone();
            value.approval_id = "short".into();
            value
        },
        {
            let mut value = approval.clone();
            value.receipt_id = "short".into();
            value
        },
        {
            let mut value = approval.clone();
            value.evidence_record_id = "short".into();
            value
        },
        {
            let mut value = approval.clone();
            value.active_use_observed_at_ms = value.approved_at_ms + 1;
            value
        },
        {
            let mut value = approval.clone();
            value.active_use.method = "ps-only".into();
            value
        },
    ];

    for (index, invalid) in invalid_cases.into_iter().enumerate() {
        let invalid_dir = temp.path().join(format!("invalid-approvals-{index}"));
        assert_eq!(
            write_immutable_source_eviction_approval(&invalid_dir, &invalid).unwrap_err(),
            "source-eviction-human-approval-invalid"
        );
        assert!(!invalid_dir.exists());
    }
}
