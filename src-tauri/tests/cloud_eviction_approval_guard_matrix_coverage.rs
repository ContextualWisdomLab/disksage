//! Public-boundary coverage for source-eviction approval integrity and fail-closed guards.
//!
//! These tests create only temporary local copy/evidence records. They never invoke Trash or a
//! provider network API. The matrix deliberately exercises every public approval guard so exact
//! production coverage can distinguish a genuinely tested denial path from a merely compiled one.

#![cfg(not(coverage))]

use disksage_lib::cloud::{
    candidate_review_fingerprint, ArchiveKind, CloudAccountScope, CloudCandidate, CloudProvider,
    CloudRoot, MetadataEvidence,
};
use disksage_lib::cloud_eviction::{
    create_source_eviction_approval, evict_source_with_human_approval,
    write_immutable_source_eviction_approval, CloudSourceEvictionApproval,
};
use disksage_lib::cloud_local_eviction::ActiveUseEvidence;
use disksage_lib::cloud_transfer::{
    approve_local_eviction, cloud_copy_approval_phrase, create_cloud_copy_approval,
    prepare_cloud_copy_with_approval, CloudCopyApprovalAction, CloudCopyReceipt,
    LocalEvictionPermit, ProviderSyncEvidence, SyncEvidenceKind,
};
use disksage_lib::provider_evidence::create_sync_evidence_record;

fn fixture() -> (tempfile::TempDir, CloudCopyReceipt, LocalEvictionPermit) {
    let temp = tempfile::tempdir().unwrap();
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
            value: "2026-08-17".into(),
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
    let action = CloudCopyApprovalAction::CopyOnly;
    let approved_at_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let copy_approval = create_cloud_copy_approval(
        &candidate,
        &root,
        action,
        approved_at_ms,
        "human:local:coverage",
        "authorize exact local copy for source eviction approval guard coverage",
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
    (temp, receipt, permit)
}

fn idle_evidence() -> ActiveUseEvidence {
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
    receipt: &CloudCopyReceipt,
    permit: &LocalEvictionPermit,
) -> CloudSourceEvictionApproval {
    let observed_at_ms = permit.approved_at_ms + 1;
    create_source_eviction_approval(
        receipt,
        permit,
        &receipt.receipt_id,
        observed_at_ms + 1,
        "human:local:coverage",
        "authorize exact source eviction after fresh idle-use proof",
        observed_at_ms,
        idle_evidence(),
    )
    .unwrap()
}

fn refresh_approval_id(approval: &mut CloudSourceEvictionApproval) {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-cloud-source-eviction-approval-v1\0");
    for value in [
        approval.receipt_id.as_str(),
        approval.evidence_record_id.as_str(),
        approval.approved_by.as_str(),
        approval.rationale.as_str(),
        approval.active_use.method.as_str(),
        approval.active_use.error.as_deref().unwrap_or_default(),
    ] {
        hasher.update(value.as_bytes());
        hasher.update(&[0]);
    }
    hasher.update(&approval.approved_at_ms.to_le_bytes());
    hasher.update(&approval.active_use_observed_at_ms.to_le_bytes());
    hasher.update(&[
        approval.active_use.evidence_complete as u8,
        approval.active_use.active as u8,
        approval.active_use.results_truncated as u8,
        approval.active_use.error.is_some() as u8,
    ]);
    for pid in &approval.active_use.observed_pids {
        hasher.update(&pid.to_le_bytes());
    }
    approval.approval_id = hasher.finalize().to_hex().to_string();
}

fn assert_invalid_write(temp: &tempfile::TempDir, approval: &CloudSourceEvictionApproval) {
    assert_eq!(
        write_immutable_source_eviction_approval(&temp.path().join("approvals"), approval)
            .unwrap_err(),
        "source-eviction-human-approval-invalid"
    );
}

#[test]
fn immutable_approval_validation_covers_integrity_and_active_use_guard_matrix() {
    let (temp, receipt, permit) = fixture();
    let approval = valid_approval(&receipt, &permit);

    let mut invalid = approval.clone();
    invalid.version += 1;
    assert_invalid_write(&temp, &invalid);

    let mut invalid = approval.clone();
    invalid.approval_id = "not-a-hex-id".into();
    assert_invalid_write(&temp, &invalid);

    let mut invalid = approval.clone();
    invalid.receipt_id = "not-a-receipt-id".into();
    assert_invalid_write(&temp, &invalid);

    let mut invalid = approval.clone();
    invalid.evidence_record_id = "not-an-evidence-record-id".into();
    assert_invalid_write(&temp, &invalid);

    let mut invalid = approval.clone();
    invalid.rationale.push_str(" tampered");
    assert_invalid_write(&temp, &invalid);

    let mut invalid = approval.clone();
    invalid.active_use_observed_at_ms = invalid.approved_at_ms + 1;
    refresh_approval_id(&mut invalid);
    assert_invalid_write(&temp, &invalid);

    let mut variants = Vec::new();

    let mut invalid = approval.clone();
    invalid.active_use.method = "unknown-probe".into();
    variants.push(invalid);

    let mut invalid = approval.clone();
    invalid.active_use.evidence_complete = false;
    variants.push(invalid);

    let mut invalid = approval.clone();
    invalid.active_use.active = true;
    variants.push(invalid);

    let mut invalid = approval.clone();
    invalid.active_use.observed_pids = vec![42];
    variants.push(invalid);

    let mut invalid = approval.clone();
    invalid.active_use.results_truncated = true;
    variants.push(invalid);

    let mut invalid = approval.clone();
    invalid.active_use.error = Some("probe-failed".into());
    variants.push(invalid);

    for mut invalid in variants {
        refresh_approval_id(&mut invalid);
        assert_invalid_write(&temp, &invalid);
    }
}

#[test]
fn immutable_approval_reaches_attribution_guard_after_integrity_validation() {
    let (temp, receipt, permit) = fixture();
    let mut approval = valid_approval(&receipt, &permit);
    approval.approved_by = "agent:coverage".into();
    refresh_approval_id(&mut approval);

    assert_eq!(
        write_immutable_source_eviction_approval(&temp.path().join("approvals"), &approval)
            .unwrap_err(),
        "source-eviction-human-approval-attribution-invalid"
    );
}

#[test]
fn immutable_approval_is_private_read_only_and_create_new() {
    let (temp, receipt, permit) = fixture();
    let approval = valid_approval(&receipt, &permit);
    let approval_dir = temp.path().join("approvals");

    let path = write_immutable_source_eviction_approval(&approval_dir, &approval).unwrap();
    assert!(path.starts_with(&approval_dir));
    assert!(path.metadata().unwrap().permissions().readonly());
    let on_disk: CloudSourceEvictionApproval =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(on_disk, approval);

    assert!(write_immutable_source_eviction_approval(&approval_dir, &approval).is_err());
}

#[test]
fn create_approval_rejects_each_active_use_safety_dimension_and_time_bound() {
    let (_temp, receipt, permit) = fixture();
    let approved_at_ms = permit.approved_at_ms + 2;
    let observed_at_ms = permit.approved_at_ms + 1;
    let mut variants = Vec::new();

    let mut evidence = idle_evidence();
    evidence.method = "unknown-probe".into();
    variants.push(evidence);

    let mut evidence = idle_evidence();
    evidence.evidence_complete = false;
    variants.push(evidence);

    let mut evidence = idle_evidence();
    evidence.active = true;
    variants.push(evidence);

    let mut evidence = idle_evidence();
    evidence.observed_pids = vec![7];
    variants.push(evidence);

    let mut evidence = idle_evidence();
    evidence.results_truncated = true;
    variants.push(evidence);

    let mut evidence = idle_evidence();
    evidence.error = Some("probe-failed".into());
    variants.push(evidence);

    for evidence in variants {
        assert_eq!(
            create_source_eviction_approval(
                &receipt,
                &permit,
                &receipt.receipt_id,
                approved_at_ms,
                "human:local:coverage",
                "reject incomplete active-use evidence",
                observed_at_ms,
                evidence,
            )
            .unwrap_err(),
            "source-eviction-active-use-evidence-invalid"
        );
    }

    assert_eq!(
        create_source_eviction_approval(
            &receipt,
            &permit,
            &receipt.receipt_id,
            approved_at_ms,
            "human:local:coverage",
            "reject pre-permit observation",
            permit.approved_at_ms.saturating_sub(1),
            idle_evidence(),
        )
        .unwrap_err(),
        "source-eviction-active-use-evidence-invalid"
    );

    assert_eq!(
        create_source_eviction_approval(
            &receipt,
            &permit,
            &receipt.receipt_id,
            approved_at_ms,
            "human:local:coverage",
            "reject future observation",
            approved_at_ms + 1,
            idle_evidence(),
        )
        .unwrap_err(),
        "source-eviction-active-use-evidence-invalid"
    );
}

#[test]
fn human_approval_entrypoint_rejects_wrong_confirmation_before_live_probe() {
    let (temp, receipt, permit) = fixture();
    let approval = valid_approval(&receipt, &permit);

    assert_eq!(
        evict_source_with_human_approval(
            &receipt,
            &permit,
            &approval,
            &"0".repeat(64),
            &temp.path().join("evictions"),
            &temp.path().join("journal/operations.jsonl"),
            permit.approved_at_ms + 10,
        )
        .unwrap_err(),
        "eviction-confirmation-receipt-id-mismatch"
    );
}
