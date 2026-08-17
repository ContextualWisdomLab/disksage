//! Public-boundary coverage for successful source-eviction approval creation and permit binding.
//!
//! The fixture performs a real local copy into a temporary directory and creates provider-native
//! evidence, but it never trashes the source or contacts a provider.

use disksage_lib::cloud::{
    candidate_review_fingerprint, ArchiveKind, CloudAccountScope, CloudCandidate, CloudProvider,
    CloudRoot, MetadataEvidence,
};
use disksage_lib::cloud_eviction::create_source_eviction_approval;
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

    let source = source_dir.join("approval-success.bin");
    let destination = cloud_dir.join("approval-success.bin");
    std::fs::write(&source, b"approval success coverage bytes").unwrap();
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
        relative_path: "approval-success.bin".into(),
        source_context: ".".into(),
        requires_review: false,
        review_reasons: Vec::new(),
        content_title: Some("Approval success coverage".into()),
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
    let copy_approved_at_ms = modified_ms.saturating_add(1);
    let action = CloudCopyApprovalAction::CopyOnly;
    let copy_approval = create_cloud_copy_approval(
        &candidate,
        &root,
        action,
        copy_approved_at_ms,
        "human:local:coverage",
        "authorize exact source copy for eviction approval coverage",
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
        evidence_id: "approval-success-native-evidence".into(),
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

#[test]
fn source_eviction_approval_success_is_trimmed_integrity_bound_and_serializable() {
    let (_temp, receipt, permit) = fixture();
    let observed_at_ms = permit.approved_at_ms + 1;
    let approved_at_ms = observed_at_ms + 1;

    let approval = create_source_eviction_approval(
        &receipt,
        &permit,
        &receipt.receipt_id,
        approved_at_ms,
        "  human:local:coverage  ",
        "  authorize exact source eviction after fresh idle-use proof  ",
        observed_at_ms,
        idle_evidence(),
    )
    .unwrap();

    assert_eq!(approval.version, 1);
    assert_eq!(approval.receipt_id, receipt.receipt_id);
    assert_eq!(approval.evidence_record_id, permit.evidence_record_id);
    assert_eq!(approval.approved_at_ms, approved_at_ms);
    assert_eq!(approval.approved_by, "human:local:coverage");
    assert_eq!(
        approval.rationale,
        "authorize exact source eviction after fresh idle-use proof"
    );
    assert_eq!(approval.active_use_observed_at_ms, observed_at_ms);
    assert_eq!(approval.approval_id.len(), 64);
    assert!(approval
        .approval_id
        .bytes()
        .all(|byte| byte.is_ascii_hexdigit()));

    let encoded = serde_json::to_value(&approval).unwrap();
    assert_eq!(encoded["approval_id"], approval.approval_id);
    assert_eq!(encoded["active_use"]["method"], "lsof-fp+ps-command");
    assert_eq!(encoded["active_use"]["evidence_complete"], true);
    assert_eq!(encoded["active_use"]["active"], false);
}

#[test]
fn source_eviction_approval_rejects_permits_that_no_longer_bind_the_receipt() {
    let (_temp, receipt, permit) = fixture();
    let observed_at_ms = permit.approved_at_ms + 1;
    let approved_at_ms = observed_at_ms + 1;

    let mut mismatched = permit.clone();
    mismatched.destination.push_str("-other");
    let mismatch = create_source_eviction_approval(
        &receipt,
        &mismatched,
        &receipt.receipt_id,
        approved_at_ms,
        "human:local:coverage",
        "reject a permit no longer bound to this receipt",
        observed_at_ms,
        idle_evidence(),
    )
    .unwrap_err();
    assert_eq!(mismatch, "eviction-permit-receipt-mismatch");

    let mut stale = permit.clone();
    stale.approved_at_ms = receipt.copied_at_ms.saturating_sub(1);
    let stale_error = create_source_eviction_approval(
        &receipt,
        &stale,
        &receipt.receipt_id,
        approved_at_ms,
        "human:local:coverage",
        "reject a stale provider confirmation permit",
        observed_at_ms,
        idle_evidence(),
    )
    .unwrap_err();
    assert_eq!(stale_error, "eviction-permit-invalid");

    let mut missing_evidence = permit.clone();
    missing_evidence.evidence_id.clear();
    let evidence_error = create_source_eviction_approval(
        &receipt,
        &missing_evidence,
        &receipt.receipt_id,
        approved_at_ms,
        "human:local:coverage",
        "reject a permit without provider evidence identity",
        observed_at_ms,
        idle_evidence(),
    )
    .unwrap_err();
    assert_eq!(evidence_error, "eviction-permit-invalid");

    let mut malformed_record = permit;
    malformed_record.evidence_record_id = "not-a-record-id".into();
    let record_error = create_source_eviction_approval(
        &receipt,
        &malformed_record,
        &receipt.receipt_id,
        approved_at_ms,
        "human:local:coverage",
        "reject a permit without an integrity-bound evidence record",
        observed_at_ms,
        idle_evidence(),
    )
    .unwrap_err();
    assert_eq!(record_error, "eviction-permit-invalid");
}
