use disksage_lib::cloud::{
    candidate_review_fingerprint, ArchiveKind, CloudAccountScope, CloudCandidate, CloudProvider,
    MetadataEvidence,
};
use disksage_lib::cloud_review::{create_attributed_decision, CloudReviewDisposition};
use disksage_lib::cloud_transfer::{
    CloudCopyReceipt, CloudCopyVerificationMethod, CloudLineageSnapshot, ProviderSyncEvidence,
    SyncEvidenceKind, PRE_APPROVAL_RECEIPT_VERSION,
};
use disksage_lib::naruon_lineage::export_naruon_file_lineage;
use disksage_lib::provider_evidence::create_sync_evidence_record;

fn reviewed_candidate() -> CloudCandidate {
    let mut candidate = CloudCandidate {
        metadata_fingerprint: "b".repeat(64),
        review_fingerprint: String::new(),
        src: "/source/report.pdf".into(),
        dst: "/cloud/report.pdf".into(),
        provider: CloudProvider::GoogleDrive,
        destination_account_scope: CloudAccountScope::Organization,
        kind: ArchiveKind::Document,
        bytes: 42,
        age_days: 90,
        created_ms: 10,
        modified_ms: 20,
        production_time_ms: 5,
        production_time_source: "embedded:exiftool:CreateDate".into(),
        production_time_confidence: "high".into(),
        source_root: "/source".into(),
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
    candidate
}

fn receipt() -> CloudCopyReceipt {
    let candidate = reviewed_candidate();
    let decision = create_attributed_decision(
        &candidate,
        CloudReviewDisposition::Approved,
        25,
        "human:local:test",
        "embedded metadata checked",
    )
    .expect("fixture review must be attributable");

    CloudCopyReceipt {
        version: PRE_APPROVAL_RECEIPT_VERSION,
        receipt_id: "a".repeat(64),
        candidate_fingerprint: candidate.metadata_fingerprint.clone(),
        provider: candidate.provider,
        source: candidate.src.clone(),
        destination: candidate.dst.clone(),
        bytes: candidate.bytes,
        blake3: "c".repeat(64),
        sha256: "d".repeat(64),
        quick_xor_base64: "quick-xor".into(),
        source_modified_ms: 20,
        copied_at_ms: 30,
        copy_verified: true,
        provider_sync_confirmed: false,
        lineage_fingerprint: Some("e".repeat(64)),
        lineage: Some(CloudLineageSnapshot {
            candidate_fingerprint: decision.candidate_fingerprint,
            review_fingerprint: decision.review_fingerprint,
            copy_verification_method: CloudCopyVerificationMethod::CopiedByDiskSage,
            review_decision_id: Some(decision.decision_id),
            review_disposition: Some(decision.disposition),
            reviewed_at_ms: Some(decision.reviewed_at_ms),
            reviewed_by: Some(decision.reviewed_by),
            review_rationale: Some(decision.rationale),
            destination_account_scope: candidate.destination_account_scope,
            kind: candidate.kind,
            created_ms: candidate.created_ms,
            modified_ms: candidate.modified_ms,
            production_time_ms: candidate.production_time_ms,
            production_time_source: candidate.production_time_source,
            production_time_confidence: candidate.production_time_confidence,
            source_root: candidate.source_root,
            relative_path: candidate.relative_path,
            source_context: candidate.source_context,
            requires_review: candidate.requires_review,
            review_reasons: candidate.review_reasons,
            content_title: candidate.content_title,
            content_authors: candidate.content_authors,
            content_context: candidate.content_context,
            duration_ms: candidate.duration_ms,
            dataset_profile: candidate.dataset_profile,
            metadata_evidence: candidate.metadata_evidence,
            copy_approval: None,
        }),
    }
}

fn provider_evidence(receipt: &CloudCopyReceipt) -> ProviderSyncEvidence {
    ProviderSyncEvidence {
        receipt_id: receipt.receipt_id.clone(),
        provider: receipt.provider,
        destination: receipt.destination.clone(),
        observed_bytes: receipt.bytes,
        destination_blake3: receipt.blake3.clone(),
        confirmed_at_ms: 40,
        kind: SyncEvidenceKind::ProviderNativeStatus,
        evidence_id: format!("file-provider:{}", "1".repeat(64)),
        sync_complete: true,
        remote_content: None,
    }
}

#[test]
fn lineage_export_rejects_unverified_copy_and_identity_digest_drift() {
    let mut unverified = receipt();
    unverified.copy_verified = false;
    assert_eq!(
        export_naruon_file_lineage(&unverified, None).unwrap_err(),
        "naruon-lineage-copy-not-verified"
    );

    let mut bad_receipt_id = receipt();
    bad_receipt_id.receipt_id = "g".repeat(64);
    assert_eq!(
        export_naruon_file_lineage(&bad_receipt_id, None).unwrap_err(),
        "naruon-lineage-receipt-digest-invalid"
    );

    let mut missing_lineage_fingerprint = receipt();
    missing_lineage_fingerprint.lineage_fingerprint = None;
    assert_eq!(
        export_naruon_file_lineage(&missing_lineage_fingerprint, None).unwrap_err(),
        "naruon-lineage-receipt-lineage-missing"
    );

    let mut bad_lineage_fingerprint = receipt();
    bad_lineage_fingerprint.lineage_fingerprint = Some("g".repeat(64));
    assert_eq!(
        export_naruon_file_lineage(&bad_lineage_fingerprint, None).unwrap_err(),
        "naruon-lineage-receipt-lineage-missing"
    );

    let mut candidate_binding_drift = receipt();
    candidate_binding_drift.candidate_fingerprint = "f".repeat(64);
    assert_eq!(
        export_naruon_file_lineage(&candidate_binding_drift, None).unwrap_err(),
        "naruon-lineage-candidate-binding-mismatch"
    );

    let mut review_fingerprint_drift = receipt();
    review_fingerprint_drift
        .lineage
        .as_mut()
        .expect("fixture lineage")
        .review_fingerprint = "g".repeat(64);
    assert_eq!(
        export_naruon_file_lineage(&review_fingerprint_drift, None).unwrap_err(),
        "naruon-lineage-candidate-binding-mismatch"
    );
}

#[test]
fn lineage_export_rejects_inconsistent_review_shapes_and_future_reviews() {
    let mut unexpected_review = receipt();
    unexpected_review
        .lineage
        .as_mut()
        .expect("fixture lineage")
        .requires_review = false;
    assert_eq!(
        export_naruon_file_lineage(&unexpected_review, None).unwrap_err(),
        "naruon-lineage-review-decision-unexpected"
    );

    let mut missing_reasons = receipt();
    missing_reasons
        .lineage
        .as_mut()
        .expect("fixture lineage")
        .review_reasons
        .clear();
    assert_eq!(
        export_naruon_file_lineage(&missing_reasons, None).unwrap_err(),
        "naruon-lineage-review-reasons-missing"
    );

    let mut rejected_review = receipt();
    rejected_review
        .lineage
        .as_mut()
        .expect("fixture lineage")
        .review_disposition = Some(CloudReviewDisposition::Rejected);
    assert_eq!(
        export_naruon_file_lineage(&rejected_review, None).unwrap_err(),
        "naruon-lineage-review-decision-invalid"
    );

    let mut future_review = receipt();
    future_review
        .lineage
        .as_mut()
        .expect("fixture lineage")
        .reviewed_at_ms = Some(31);
    assert_eq!(
        export_naruon_file_lineage(&future_review, None).unwrap_err(),
        "naruon-lineage-review-decision-invalid"
    );
}

#[test]
fn lineage_export_rejects_unsafe_source_path_shapes() {
    for unsafe_path in ["", "reports/\nreport.pdf", "/absolute/report.pdf", "../report.pdf"] {
        let mut current = receipt();
        current
            .lineage
            .as_mut()
            .expect("fixture lineage")
            .relative_path = unsafe_path.into();
        assert_eq!(
            export_naruon_file_lineage(&current, None).unwrap_err(),
            "naruon-lineage-source-relative-path-invalid",
            "unsafe path must fail closed: {unsafe_path:?}"
        );
    }
}

#[test]
fn lineage_export_rejects_each_provider_evidence_binding_drift() {
    let receipt = receipt();

    let mut provider_drift = provider_evidence(&receipt);
    provider_drift.provider = CloudProvider::Onedrive;
    let provider_drift = create_sync_evidence_record(&provider_drift).expect("valid drift record");
    assert_eq!(
        export_naruon_file_lineage(&receipt, Some(&provider_drift)).unwrap_err(),
        "naruon-lineage-provider-evidence-mismatch"
    );

    let mut destination_drift = provider_evidence(&receipt);
    destination_drift.destination = "/cloud/other.pdf".into();
    let destination_drift =
        create_sync_evidence_record(&destination_drift).expect("valid drift record");
    assert_eq!(
        export_naruon_file_lineage(&receipt, Some(&destination_drift)).unwrap_err(),
        "naruon-lineage-provider-evidence-mismatch"
    );

    let mut bytes_drift = provider_evidence(&receipt);
    bytes_drift.observed_bytes += 1;
    let bytes_drift = create_sync_evidence_record(&bytes_drift).expect("valid drift record");
    assert_eq!(
        export_naruon_file_lineage(&receipt, Some(&bytes_drift)).unwrap_err(),
        "naruon-lineage-provider-evidence-mismatch"
    );

    let mut digest_drift = provider_evidence(&receipt);
    digest_drift.destination_blake3 = "f".repeat(64);
    let digest_drift = create_sync_evidence_record(&digest_drift).expect("valid drift record");
    assert_eq!(
        export_naruon_file_lineage(&receipt, Some(&digest_drift)).unwrap_err(),
        "naruon-lineage-provider-evidence-mismatch"
    );
}
