//! Read-only export of verified DiskSage copy evidence for Naruon ingestion.
//!
//! Naruon's existing RFC 822 lineage envelope is email-specific. This module keeps the same
//! metadata-first and fail-closed principles while using a distinct schema name for general files.

use std::path::{Component, Path};

use crate::cloud::{ArchiveKind, CloudAccountScope, CloudProvider, MetadataEvidence};
use crate::cloud_review::CloudReviewDisposition;
use crate::cloud_transfer::{CloudCopyReceipt, CloudCopyVerificationMethod, SyncEvidenceKind};
use crate::provider_evidence::{validate_sync_evidence_record, ProviderSyncEvidenceRecord};

pub const NARUON_FILE_LINEAGE_SCHEMA_VERSION: u32 = 1;
pub const NARUON_FILE_LINEAGE_SCHEMA_KIND: &str = "disksage.file-lineage";

const EVIDENCE_PRECEDENCE: [&str; 4] = [
    "embedded_metadata",
    "explicit_filename_date",
    "filesystem_created_at",
    "filesystem_modified_at",
];

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NaruonProductionTimeLineage {
    pub selected_value_ms: u64,
    pub selected_source: String,
    pub confidence: String,
    pub evidence_precedence: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NaruonFilesystemTimeLineage {
    pub created_at_ms: u64,
    pub modified_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NaruonReviewLineage {
    pub candidate_fingerprint: String,
    pub review_fingerprint: String,
    pub requires_review: bool,
    pub reason_codes: Vec<String>,
    pub decision_id: Option<String>,
    pub disposition: Option<CloudReviewDisposition>,
    pub reviewed_at_ms: Option<u64>,
    pub reviewed_by: Option<String>,
    pub rationale: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NaruonCloudCopyLineage {
    pub receipt_id: String,
    pub lineage_fingerprint: String,
    pub provider: CloudProvider,
    pub destination_account_scope: CloudAccountScope,
    pub destination: String,
    pub copied_at_ms: u64,
    pub copy_verification_method: CloudCopyVerificationMethod,
    pub local_copy_verified: bool,
    /// DiskSage's local File Provider copy is not proof that a provider API write executed.
    pub provider_write_executed: bool,
    pub provider_sync_confirmed: bool,
    pub sync_evidence_record_id: Option<String>,
    pub sync_evidence_kind: Option<SyncEvidenceKind>,
    pub sync_evidence_id: Option<String>,
    pub sync_confirmed_at_ms: Option<u64>,
    pub remote_object_id: Option<String>,
    pub remote_revision: Option<String>,
    pub remote_location_bound: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NaruonFileLineageEnvelope {
    pub schema_version: u32,
    pub schema_kind: String,
    pub source_kind: String,
    pub archive_kind: ArchiveKind,
    pub source_filename: String,
    pub source_relative_path: String,
    pub source_context: String,
    pub raw_content_sha256: String,
    pub raw_content_blake3: String,
    pub bytes: u64,
    pub production_time: NaruonProductionTimeLineage,
    pub filesystem_time: NaruonFilesystemTimeLineage,
    pub metadata_evidence: Vec<MetadataEvidence>,
    pub content_title: Option<String>,
    pub content_authors: Vec<String>,
    pub content_context: Vec<String>,
    pub duration_ms: Option<u64>,
    pub review: NaruonReviewLineage,
    pub cloud_copy: NaruonCloudCopyLineage,
}

fn valid_hex64(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn source_filename(relative_path: &str) -> Result<String, String> {
    if relative_path.is_empty() || relative_path.chars().any(char::is_control) {
        return Err("naruon-lineage-source-relative-path-invalid".into());
    }
    let path = Path::new(relative_path);
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err("naruon-lineage-source-relative-path-invalid".into());
    }
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "naruon-lineage-source-filename-invalid".to_string())
}

fn validate_receipt_shape(receipt: &CloudCopyReceipt) -> Result<(), String> {
    if !receipt.copy_verified {
        return Err("naruon-lineage-copy-not-verified".into());
    }
    if !valid_hex64(&receipt.receipt_id)
        || !valid_hex64(&receipt.candidate_fingerprint)
        || !valid_hex64(&receipt.sha256)
        || !valid_hex64(&receipt.blake3)
    {
        return Err("naruon-lineage-receipt-digest-invalid".into());
    }
    let fingerprint = receipt
        .lineage_fingerprint
        .as_deref()
        .ok_or_else(|| "naruon-lineage-receipt-lineage-missing".to_string())?;
    if !valid_hex64(fingerprint) || receipt.lineage.is_none() {
        return Err("naruon-lineage-receipt-lineage-missing".into());
    }
    Ok(())
}

fn validate_evidence_binding(
    receipt: &CloudCopyReceipt,
    record: &ProviderSyncEvidenceRecord,
) -> Result<(), String> {
    validate_sync_evidence_record(record)?;
    let evidence = &record.evidence;
    if evidence.receipt_id != receipt.receipt_id
        || evidence.provider != receipt.provider
        || evidence.destination != receipt.destination
        || evidence.observed_bytes != receipt.bytes
        || evidence.destination_blake3 != receipt.blake3
    {
        return Err("naruon-lineage-provider-evidence-mismatch".into());
    }
    Ok(())
}

/// Convert an integrity-validated immutable receipt and optional provider evidence into Naruon's
/// general-file lineage envelope. This function performs no provider or filesystem mutation.
pub fn export_naruon_file_lineage(
    receipt: &CloudCopyReceipt,
    evidence_record: Option<&ProviderSyncEvidenceRecord>,
) -> Result<NaruonFileLineageEnvelope, String> {
    validate_receipt_shape(receipt)?;
    if let Some(record) = evidence_record {
        validate_evidence_binding(receipt, record)?;
    }
    let lineage = receipt
        .lineage
        .as_ref()
        .ok_or_else(|| "naruon-lineage-receipt-lineage-missing".to_string())?;
    if lineage.candidate_fingerprint != receipt.candidate_fingerprint
        || !valid_hex64(&lineage.review_fingerprint)
    {
        return Err("naruon-lineage-candidate-binding-mismatch".into());
    }
    let source_filename = source_filename(&lineage.relative_path)?;
    let evidence = evidence_record.map(|record| &record.evidence);
    let remote_content = evidence.and_then(|item| item.remote_content.as_ref());

    Ok(NaruonFileLineageEnvelope {
        schema_version: NARUON_FILE_LINEAGE_SCHEMA_VERSION,
        schema_kind: NARUON_FILE_LINEAGE_SCHEMA_KIND.into(),
        source_kind: "file".into(),
        archive_kind: lineage.kind,
        source_filename,
        source_relative_path: lineage.relative_path.clone(),
        source_context: lineage.source_context.clone(),
        raw_content_sha256: receipt.sha256.clone(),
        raw_content_blake3: receipt.blake3.clone(),
        bytes: receipt.bytes,
        production_time: NaruonProductionTimeLineage {
            selected_value_ms: lineage.production_time_ms,
            selected_source: lineage.production_time_source.clone(),
            confidence: lineage.production_time_confidence.clone(),
            evidence_precedence: EVIDENCE_PRECEDENCE.map(str::to_string).to_vec(),
        },
        filesystem_time: NaruonFilesystemTimeLineage {
            created_at_ms: lineage.created_ms,
            modified_at_ms: lineage.modified_ms,
        },
        metadata_evidence: lineage.metadata_evidence.clone(),
        content_title: lineage.content_title.clone(),
        content_authors: lineage.content_authors.clone(),
        content_context: lineage.content_context.clone(),
        duration_ms: lineage.duration_ms,
        review: NaruonReviewLineage {
            candidate_fingerprint: lineage.candidate_fingerprint.clone(),
            review_fingerprint: lineage.review_fingerprint.clone(),
            requires_review: lineage.requires_review,
            reason_codes: lineage.review_reasons.clone(),
            decision_id: lineage.review_decision_id.clone(),
            disposition: lineage.review_disposition,
            reviewed_at_ms: lineage.reviewed_at_ms,
            reviewed_by: lineage.reviewed_by.clone(),
            rationale: lineage.review_rationale.clone(),
        },
        cloud_copy: NaruonCloudCopyLineage {
            receipt_id: receipt.receipt_id.clone(),
            lineage_fingerprint: receipt
                .lineage_fingerprint
                .clone()
                .ok_or_else(|| "naruon-lineage-receipt-lineage-missing".to_string())?,
            provider: receipt.provider,
            destination_account_scope: lineage.destination_account_scope,
            destination: receipt.destination.clone(),
            copied_at_ms: receipt.copied_at_ms,
            copy_verification_method: lineage.copy_verification_method,
            local_copy_verified: receipt.copy_verified,
            provider_write_executed: false,
            provider_sync_confirmed: evidence.is_some_and(|item| item.sync_complete),
            sync_evidence_record_id: evidence_record.map(|record| record.record_id.clone()),
            sync_evidence_kind: evidence.map(|item| item.kind),
            sync_evidence_id: evidence.map(|item| item.evidence_id.clone()),
            sync_confirmed_at_ms: evidence.map(|item| item.confirmed_at_ms),
            remote_object_id: remote_content.map(|proof| proof.object_id.clone()),
            remote_revision: remote_content.map(|proof| proof.revision.clone()),
            remote_location_bound: remote_content.map(|proof| proof.location_bound),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::{ArchiveKind, CloudAccountScope, CloudProvider, MetadataEvidence};
    use crate::cloud_transfer::{
        CloudCopyReceipt, CloudCopyVerificationMethod, CloudLineageSnapshot, ProviderSyncEvidence,
        SyncEvidenceKind, RECEIPT_VERSION,
    };
    use crate::provider_evidence::create_sync_evidence_record;

    fn receipt() -> CloudCopyReceipt {
        CloudCopyReceipt {
            version: RECEIPT_VERSION,
            receipt_id: "a".repeat(64),
            candidate_fingerprint: "b".repeat(64),
            provider: CloudProvider::GoogleDrive,
            source: "/source/report.pdf".into(),
            destination: "/cloud/report.pdf".into(),
            bytes: 42,
            blake3: "c".repeat(64),
            sha256: "d".repeat(64),
            quick_xor_base64: "quick-xor".into(),
            source_modified_ms: 20,
            copied_at_ms: 30,
            copy_verified: true,
            provider_sync_confirmed: false,
            lineage_fingerprint: Some("e".repeat(64)),
            lineage: Some(CloudLineageSnapshot {
                candidate_fingerprint: "b".repeat(64),
                review_fingerprint: "f".repeat(64),
                copy_verification_method: CloudCopyVerificationMethod::CopiedByDiskSage,
                review_decision_id: Some("decision-1".into()),
                review_disposition: Some(CloudReviewDisposition::Approved),
                reviewed_at_ms: Some(25),
                reviewed_by: Some("human:local:test".into()),
                review_rationale: Some("embedded metadata checked".into()),
                destination_account_scope: CloudAccountScope::Organization,
                kind: ArchiveKind::Document,
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
                    field: "production_time".into(),
                    value: "2026-01-01".into(),
                    source: "exiftool:CreateDate".into(),
                    confidence: "high".into(),
                }],
            }),
        }
    }

    fn evidence(receipt: &CloudCopyReceipt) -> ProviderSyncEvidenceRecord {
        create_sync_evidence_record(&ProviderSyncEvidence {
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
        })
        .unwrap()
    }

    #[test]
    fn export_preserves_metadata_precedence_and_does_not_invent_provider_write() {
        let receipt = receipt();
        let envelope = export_naruon_file_lineage(&receipt, Some(&evidence(&receipt))).unwrap();

        assert_eq!(envelope.schema_version, 1);
        assert_eq!(envelope.schema_kind, "disksage.file-lineage");
        assert_eq!(envelope.source_filename, "report.pdf");
        assert_eq!(envelope.raw_content_sha256, "d".repeat(64));
        assert_eq!(
            envelope.production_time.evidence_precedence,
            [
                "embedded_metadata",
                "explicit_filename_date",
                "filesystem_created_at",
                "filesystem_modified_at",
            ]
        );
        assert!(envelope.cloud_copy.local_copy_verified);
        assert!(envelope.cloud_copy.provider_sync_confirmed);
        assert!(!envelope.cloud_copy.provider_write_executed);
        assert!(envelope.cloud_copy.sync_evidence_record_id.is_some());
    }

    #[test]
    fn export_without_provider_evidence_remains_unconfirmed() {
        let envelope = export_naruon_file_lineage(&receipt(), None).unwrap();

        assert!(!envelope.cloud_copy.provider_sync_confirmed);
        assert_eq!(envelope.cloud_copy.sync_evidence_id, None);
        assert_eq!(envelope.cloud_copy.sync_confirmed_at_ms, None);
    }

    #[test]
    fn export_rejects_missing_lineage_bad_digest_and_mismatched_evidence() {
        let mut missing = receipt();
        missing.lineage = None;
        assert_eq!(
            export_naruon_file_lineage(&missing, None).unwrap_err(),
            "naruon-lineage-receipt-lineage-missing"
        );

        let mut bad_digest = receipt();
        bad_digest.sha256 = "not-a-digest".into();
        assert_eq!(
            export_naruon_file_lineage(&bad_digest, None).unwrap_err(),
            "naruon-lineage-receipt-digest-invalid"
        );

        let receipt = receipt();
        let mut mismatched = evidence(&receipt);
        mismatched.evidence.observed_bytes += 1;
        mismatched = create_sync_evidence_record(&mismatched.evidence).unwrap();
        assert_eq!(
            export_naruon_file_lineage(&receipt, Some(&mismatched)).unwrap_err(),
            "naruon-lineage-provider-evidence-mismatch"
        );
    }

    #[test]
    fn export_rejects_unsafe_source_relative_path() {
        let mut receipt = receipt();
        receipt.lineage.as_mut().unwrap().relative_path = "../report.pdf".into();
        assert_eq!(
            export_naruon_file_lineage(&receipt, None).unwrap_err(),
            "naruon-lineage-source-relative-path-invalid"
        );
    }
}
