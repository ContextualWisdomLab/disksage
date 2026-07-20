//! Fail-closed cloud transfer safety gates.
//!
//! A verified copy is deliberately not a move. The source remains untouched until a later
//! provider-native synchronization attestation matches the immutable copy receipt. This module
//! produces an eviction permit but intentionally exposes no source deletion API.

use crate::cloud::{
    candidate_review_fingerprint, ArchiveKind, CloudAccountScope, CloudCandidate, CloudProvider,
    CloudRoot, MetadataEvidence,
};
use crate::cloud_review::{validate_decision, CloudReviewDecision, CloudReviewDisposition};
use crate::dataset_metadata::DatasetProfile;
use crate::provider_evidence::{validate_sync_evidence_record, ProviderSyncEvidenceRecord};
use std::path::Path;

#[cfg(not(coverage))]
use crate::content_digest::{ContentDigests, ContentHasher};
#[cfg(not(coverage))]
use std::io::{Read, Write};
#[cfg(not(coverage))]
use std::path::PathBuf;

pub const LEGACY_RECEIPT_VERSION: u32 = 2;
pub const RECEIPT_VERSION: u32 = 3;
#[cfg(not(coverage))]
const MAX_RECEIPT_BYTES: u64 = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SyncEvidenceKind {
    ProviderApi,
    ProviderNativeStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RemoteChecksumAlgorithm {
    Sha256,
    QuickXor,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CloudCopyVerificationMethod {
    #[default]
    CopiedByDiskSage,
    AdoptedExisting,
}

impl CloudCopyVerificationMethod {
    fn is_copied_by_disksage(&self) -> bool {
        *self == Self::CopiedByDiskSage
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteContentProof {
    pub object_id: String,
    pub revision: String,
    pub algorithm: RemoteChecksumAlgorithm,
    pub checksum: String,
    /// True only when the authenticated provider lookup addressed the exact receipt destination,
    /// rather than an operator-supplied object ID that could name equal content elsewhere.
    #[serde(default)]
    pub location_bound: bool,
    /// Integrity-bound description of how the exact destination was resolved. OneDrive records a
    /// canonical path-addressed lookup; Google Drive records the verified parent chain to My Drive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location_proof: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CloudLineageSnapshot {
    pub candidate_fingerprint: String,
    pub review_fingerprint: String,
    /// How the destination content entered this receipt. The default is omitted so persisted v3
    /// receipts created before existing-copy adoption retain the same lineage fingerprint.
    #[serde(
        default,
        skip_serializing_if = "CloudCopyVerificationMethod::is_copied_by_disksage"
    )]
    pub copy_verification_method: CloudCopyVerificationMethod,
    pub review_decision_id: Option<String>,
    pub review_disposition: Option<CloudReviewDisposition>,
    pub reviewed_at_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reviewed_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_rationale: Option<String>,
    pub destination_account_scope: CloudAccountScope,
    pub kind: ArchiveKind,
    pub created_ms: u64,
    pub modified_ms: u64,
    pub production_time_ms: u64,
    pub production_time_source: String,
    pub production_time_confidence: String,
    pub source_root: String,
    pub relative_path: String,
    pub source_context: String,
    pub requires_review: bool,
    pub review_reasons: Vec<String>,
    pub content_title: Option<String>,
    pub content_authors: Vec<String>,
    pub content_context: Vec<String>,
    pub duration_ms: Option<u64>,
    pub dataset_profile: Option<DatasetProfile>,
    pub metadata_evidence: Vec<MetadataEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CloudCopyReceipt {
    pub version: u32,
    pub receipt_id: String,
    pub candidate_fingerprint: String,
    pub provider: CloudProvider,
    pub source: String,
    pub destination: String,
    pub bytes: u64,
    pub blake3: String,
    pub sha256: String,
    pub quick_xor_base64: String,
    pub source_modified_ms: u64,
    pub copied_at_ms: u64,
    pub copy_verified: bool,
    pub provider_sync_confirmed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lineage_fingerprint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lineage: Option<CloudLineageSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderSyncEvidence {
    pub receipt_id: String,
    pub provider: CloudProvider,
    pub destination: String,
    pub observed_bytes: u64,
    pub destination_blake3: String,
    pub confirmed_at_ms: u64,
    pub kind: SyncEvidenceKind,
    pub evidence_id: String,
    pub sync_complete: bool,
    pub remote_content: Option<RemoteContentProof>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalEvictionPermit {
    pub receipt_id: String,
    pub provider: CloudProvider,
    pub source: String,
    pub destination: String,
    pub bytes: u64,
    pub blake3: String,
    pub approved_at_ms: u64,
    pub evidence_kind: SyncEvidenceKind,
    pub evidence_id: String,
    pub evidence_record_id: String,
}

fn absolute_without_parent(path: &Path) -> bool {
    path.is_absolute()
        && !path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
}

fn embedded_high_confidence(candidate: &CloudCandidate) -> bool {
    candidate.production_time_confidence == "high"
        && candidate.production_time_source.starts_with("embedded:")
}

/// Validate that a dry-run candidate is still eligible to enter the copy-only phase.
///
/// The function collects every reason so the UI can explain why a candidate remains blocked.
fn candidate_blockers_for_action(
    candidate: &CloudCandidate,
    cloud_root: &CloudRoot,
    review_decision: Option<&CloudReviewDecision>,
    allow_existing_destination: bool,
) -> Vec<String> {
    let source = Path::new(&candidate.src);
    let destination = Path::new(&candidate.dst);
    let root = Path::new(&cloud_root.path);
    let mut blockers = Vec::new();
    let mut exact_review_approved = false;

    if candidate.review_fingerprint.len() != 64
        || !candidate
            .review_fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        blockers.push("review-fingerprint-invalid".into());
    } else if candidate.review_fingerprint != candidate_review_fingerprint(candidate) {
        blockers.push("review-fingerprint-mismatch".into());
    }
    if candidate.requires_review {
        match review_decision {
            None => blockers.push("review-required".into()),
            Some(decision) if validate_decision(decision).is_err() => {
                blockers.push("review-decision-invalid".into());
            }
            Some(decision) if decision.candidate_fingerprint != candidate.metadata_fingerprint => {
                blockers.push("review-decision-candidate-mismatch".into());
            }
            Some(decision) if decision.review_fingerprint != candidate.review_fingerprint => {
                blockers.push("review-decision-stale".into());
            }
            Some(decision) if decision.disposition == CloudReviewDisposition::Held => {
                blockers.push("review-held".into());
            }
            Some(_) => exact_review_approved = true,
        }
    }
    let existing_destination_candidate =
        candidate.blocked_reason.as_deref() == Some("destination-exists");
    if candidate.blocked_reason.is_some()
        && !(allow_existing_destination && existing_destination_candidate)
    {
        blockers.push("planner-blocked".into());
    }
    if allow_existing_destination && !existing_destination_candidate {
        blockers.push("existing-destination-plan-required".into());
    }
    // Embedded, high-confidence production time remains the only evidence that can pass without
    // an operator decision. A low-confidence explicit filename date, filesystem creation time, or
    // modification time may enter the copy-only phase only when an approval is bound to the exact
    // candidate evidence and destination above. The headless CLI never supplies a decision.
    if !embedded_high_confidence(candidate) && !exact_review_approved {
        blockers.push("embedded-high-confidence-date-required".into());
    }
    if candidate.metadata_fingerprint.trim().is_empty() {
        blockers.push("metadata-fingerprint-missing".into());
    } else if candidate.metadata_fingerprint.len() != 64
        || !candidate
            .metadata_fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        blockers.push("metadata-fingerprint-invalid".into());
    }
    if candidate.provider != cloud_root.provider {
        blockers.push("provider-mismatch".into());
    }
    if candidate.destination_account_scope != cloud_root.account_scope {
        blockers.push("destination-account-scope-mismatch".into());
    }
    if !absolute_without_parent(source) {
        blockers.push("source-path-not-safe-absolute".into());
    }
    if !absolute_without_parent(destination) {
        blockers.push("destination-path-not-safe-absolute".into());
    }
    if !absolute_without_parent(root) {
        blockers.push("cloud-root-not-safe-absolute".into());
    }
    if source == destination {
        blockers.push("source-equals-destination".into());
    }
    if source.starts_with(root) {
        blockers.push("source-already-in-cloud-root".into());
    }
    if !destination.starts_with(root) {
        blockers.push("destination-outside-cloud-root".into());
    }
    blockers
}

pub fn candidate_blockers_with_review(
    candidate: &CloudCandidate,
    cloud_root: &CloudRoot,
    review_decision: Option<&CloudReviewDecision>,
) -> Vec<String> {
    candidate_blockers_for_action(candidate, cloud_root, review_decision, false)
}

/// Validate a fresh planner candidate for adopting a destination that already exists. This clears
/// only the exact `destination-exists` planner condition; every metadata, review, account-scope,
/// and path gate remains identical to a DiskSage-created copy.
pub fn existing_copy_candidate_blockers_with_review(
    candidate: &CloudCandidate,
    cloud_root: &CloudRoot,
    review_decision: Option<&CloudReviewDecision>,
) -> Vec<String> {
    candidate_blockers_for_action(candidate, cloud_root, review_decision, true)
}

pub fn candidate_blockers(candidate: &CloudCandidate, cloud_root: &CloudRoot) -> Vec<String> {
    candidate_blockers_with_review(candidate, cloud_root, None)
}

fn receipt_id_for(
    version: u32,
    candidate_fingerprint: &str,
    provider: CloudProvider,
    source: &str,
    destination: &str,
    bytes: u64,
    content_hash: &str,
    sha256: &str,
    quick_xor_base64: &str,
    source_modified_ms: u64,
    copied_at_ms: u64,
    copy_verified: bool,
    provider_sync_confirmed: bool,
    lineage_fingerprint: Option<&str>,
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&version.to_le_bytes());
    hasher.update(candidate_fingerprint.as_bytes());
    hasher.update(&[0]);
    hasher.update(provider.as_str().as_bytes());
    hasher.update(&[0]);
    hasher.update(source.as_bytes());
    hasher.update(&[0]);
    hasher.update(destination.as_bytes());
    hasher.update(&[0]);
    hasher.update(&bytes.to_le_bytes());
    hasher.update(content_hash.as_bytes());
    hasher.update(sha256.as_bytes());
    hasher.update(quick_xor_base64.as_bytes());
    hasher.update(&source_modified_ms.to_le_bytes());
    hasher.update(&copied_at_ms.to_le_bytes());
    hasher.update(&[copy_verified as u8, provider_sync_confirmed as u8]);
    if version >= RECEIPT_VERSION {
        hasher.update(b"\0lineage\0");
        hasher.update(lineage_fingerprint.unwrap_or_default().as_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

fn lineage_snapshot(
    candidate: &CloudCandidate,
    review_decision: Option<&CloudReviewDecision>,
    copy_verification_method: CloudCopyVerificationMethod,
) -> CloudLineageSnapshot {
    CloudLineageSnapshot {
        candidate_fingerprint: candidate.metadata_fingerprint.clone(),
        review_fingerprint: candidate.review_fingerprint.clone(),
        copy_verification_method,
        review_decision_id: review_decision.map(|decision| decision.decision_id.clone()),
        review_disposition: review_decision.map(|decision| decision.disposition),
        reviewed_at_ms: review_decision.map(|decision| decision.reviewed_at_ms),
        reviewed_by: review_decision
            .filter(|decision| !decision.reviewed_by.is_empty())
            .map(|decision| decision.reviewed_by.clone()),
        review_rationale: review_decision
            .filter(|decision| !decision.rationale.is_empty())
            .map(|decision| decision.rationale.clone()),
        destination_account_scope: candidate.destination_account_scope,
        kind: candidate.kind,
        created_ms: candidate.created_ms,
        modified_ms: candidate.modified_ms,
        production_time_ms: candidate.production_time_ms,
        production_time_source: candidate.production_time_source.clone(),
        production_time_confidence: candidate.production_time_confidence.clone(),
        source_root: candidate.source_root.clone(),
        relative_path: candidate.relative_path.clone(),
        source_context: candidate.source_context.clone(),
        requires_review: candidate.requires_review,
        review_reasons: candidate.review_reasons.clone(),
        content_title: candidate.content_title.clone(),
        content_authors: candidate.content_authors.clone(),
        content_context: candidate.content_context.clone(),
        duration_ms: candidate.duration_ms,
        dataset_profile: candidate.dataset_profile.clone(),
        metadata_evidence: candidate.metadata_evidence.clone(),
    }
}

fn lineage_fingerprint(snapshot: &CloudLineageSnapshot) -> Result<String, String> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-cloud-lineage-v1\0");
    let encoded = serde_json::to_vec(snapshot)
        .map_err(|_| "receipt-lineage-serialization-failed".to_string())?;
    hasher.update(&(encoded.len() as u64).to_le_bytes());
    hasher.update(&encoded);
    Ok(hasher.finalize().to_hex().to_string())
}

fn receipt_integrity_valid(receipt: &CloudCopyReceipt) -> bool {
    receipt.receipt_id
        == receipt_id_for(
            receipt.version,
            &receipt.candidate_fingerprint,
            receipt.provider,
            &receipt.source,
            &receipt.destination,
            receipt.bytes,
            &receipt.blake3,
            &receipt.sha256,
            &receipt.quick_xor_base64,
            receipt.source_modified_ms,
            receipt.copied_at_ms,
            receipt.copy_verified,
            receipt.provider_sync_confirmed,
            receipt.lineage_fingerprint.as_deref(),
        )
}

/// Validate a persisted copy receipt before any provider-specific filesystem or API probe.
///
/// This function is read-only and deliberately excludes provider evidence. It prevents callers
/// from trusting receipt-controlled paths before the receipt's structure and integrity pass.
pub fn receipt_blockers(receipt: &CloudCopyReceipt) -> Vec<String> {
    let mut blockers = Vec::new();
    if !matches!(receipt.version, LEGACY_RECEIPT_VERSION | RECEIPT_VERSION) {
        blockers.push("receipt-version-unsupported".into());
    }
    match receipt.version {
        LEGACY_RECEIPT_VERSION => {
            if receipt.lineage.is_some() || receipt.lineage_fingerprint.is_some() {
                blockers.push("legacy-receipt-lineage-unexpected".into());
            }
        }
        RECEIPT_VERSION => match (&receipt.lineage, &receipt.lineage_fingerprint) {
            (Some(lineage), Some(fingerprint)) => {
                if lineage.candidate_fingerprint != receipt.candidate_fingerprint {
                    blockers.push("receipt-lineage-candidate-mismatch".into());
                }
                if lineage.modified_ms != receipt.source_modified_ms {
                    blockers.push("receipt-lineage-modified-time-mismatch".into());
                }
                if lineage.review_fingerprint.len() != 64
                    || !lineage
                        .review_fingerprint
                        .bytes()
                        .all(|byte| byte.is_ascii_hexdigit())
                {
                    blockers.push("receipt-lineage-review-fingerprint-invalid".into());
                }
                let complete_review = lineage.review_decision_id.is_some()
                    && lineage.review_disposition.is_some()
                    && lineage.reviewed_at_ms.is_some();
                let empty_review = lineage.review_decision_id.is_none()
                    && lineage.review_disposition.is_none()
                    && lineage.reviewed_at_ms.is_none();
                let complete_attribution =
                    lineage.reviewed_by.is_some() && lineage.review_rationale.is_some();
                let empty_attribution =
                    lineage.reviewed_by.is_none() && lineage.review_rationale.is_none();
                if (lineage.requires_review && !complete_review)
                    || (!lineage.requires_review && !empty_review)
                    || (!complete_attribution && !empty_attribution)
                {
                    blockers.push("receipt-lineage-review-decision-mismatch".into());
                }
                let lineage_matches = lineage_fingerprint(lineage)
                    .map(|observed| observed == *fingerprint)
                    .unwrap_or(false);
                if fingerprint.len() != 64
                    || !fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit())
                    || !lineage_matches
                {
                    blockers.push("receipt-lineage-integrity-mismatch".into());
                }
            }
            _ => blockers.push("receipt-lineage-missing".into()),
        },
        _ => {}
    }
    if !receipt_integrity_valid(receipt) {
        blockers.push("receipt-integrity-mismatch".into());
    }
    if !receipt.copy_verified {
        blockers.push("copy-not-verified".into());
    }
    if receipt.provider_sync_confirmed {
        blockers.push("receipt-already-consumed".into());
    }
    let source = Path::new(&receipt.source);
    let destination = Path::new(&receipt.destination);
    if !absolute_without_parent(source) {
        blockers.push("receipt-source-path-not-safe-absolute".into());
    }
    if !absolute_without_parent(destination) {
        blockers.push("receipt-destination-path-not-safe-absolute".into());
    }
    if source == destination {
        blockers.push("receipt-source-equals-destination".into());
    }
    blockers
}

#[cfg(not(coverage))]
fn same_receipt_file_identity(expected: &std::fs::Metadata, observed: &std::fs::Metadata) -> bool {
    let common = expected.file_type().is_file()
        && observed.file_type().is_file()
        && !expected.file_type().is_symlink()
        && !observed.file_type().is_symlink()
        && expected.len() == observed.len()
        && expected.permissions().readonly()
        && observed.permissions().readonly()
        && expected.modified().ok() == observed.modified().ok();
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        common && expected.dev() == observed.dev() && expected.ino() == observed.ino()
    }
    #[cfg(not(unix))]
    {
        common
    }
}

/// Read and validate a copy receipt before any provider-specific status probe.
///
/// Receipts must be bounded, read-only regular files whose filename is bound to the validated
/// receipt id. This keeps UI and CLI callers from trusting receipt-controlled paths first.
#[cfg(not(coverage))]
pub fn read_immutable_receipt(path: &Path) -> Result<CloudCopyReceipt, String> {
    let metadata = std::fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("receipt-must-be-read-only-regular-file".into());
    }
    if !metadata.permissions().readonly() {
        return Err("receipt-must-be-read-only-regular-file".into());
    }
    if metadata.len() > MAX_RECEIPT_BYTES {
        return Err("receipt-too-large".into());
    }
    let mut file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let opened = file.metadata().map_err(|error| error.to_string())?;
    if !same_receipt_file_identity(&metadata, &opened) {
        return Err("receipt-changed-during-read".into());
    }
    let mut encoded = Vec::with_capacity(metadata.len() as usize);
    std::io::Read::by_ref(&mut file)
        .take(MAX_RECEIPT_BYTES + 1)
        .read_to_end(&mut encoded)
        .map_err(|error| error.to_string())?;
    if encoded.len() as u64 > MAX_RECEIPT_BYTES {
        return Err("receipt-too-large".into());
    }
    let after = std::fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if !same_receipt_file_identity(&metadata, &after) {
        return Err("receipt-changed-during-read".into());
    }
    let receipt: CloudCopyReceipt =
        serde_json::from_slice(&encoded).map_err(|_| "receipt-json-invalid".to_string())?;
    let blockers = receipt_blockers(&receipt);
    if !blockers.is_empty() {
        return Err(blockers.join(","));
    }
    let expected_name = format!("{}.json", receipt.receipt_id);
    if path.file_name().and_then(|name| name.to_str()) != Some(expected_name.as_str()) {
        return Err("receipt-filename-id-mismatch".into());
    }
    Ok(receipt)
}

/// Convert provider-native sync evidence into a permit for a later trash-only eviction step.
///
/// This does not delete, move, hydrate, or modify either file.
pub fn approve_local_eviction(
    receipt: &CloudCopyReceipt,
    evidence_record: &ProviderSyncEvidenceRecord,
) -> Result<LocalEvictionPermit, Vec<String>> {
    let mut blockers = receipt_blockers(receipt);
    if let Err(error) = validate_sync_evidence_record(evidence_record) {
        blockers.push(error);
    }
    let evidence = &evidence_record.evidence;
    if !evidence.sync_complete {
        blockers.push("provider-sync-incomplete".into());
    }
    if evidence.receipt_id != receipt.receipt_id {
        blockers.push("receipt-id-mismatch".into());
    }
    if evidence.provider != receipt.provider {
        blockers.push("provider-mismatch".into());
    }
    if evidence.destination != receipt.destination {
        blockers.push("destination-mismatch".into());
    }
    if evidence.observed_bytes != receipt.bytes {
        blockers.push("remote-size-mismatch".into());
    }
    if evidence.destination_blake3 != receipt.blake3 {
        blockers.push("destination-hash-mismatch".into());
    }
    if evidence.confirmed_at_ms < receipt.copied_at_ms {
        blockers.push("sync-evidence-predates-copy".into());
    }
    if evidence.evidence_id.trim().is_empty() {
        blockers.push("sync-evidence-id-missing".into());
    }
    match (evidence.kind, receipt.provider, &evidence.remote_content) {
        (SyncEvidenceKind::ProviderNativeStatus, _, None) => {}
        (SyncEvidenceKind::ProviderNativeStatus, _, Some(_)) => {
            blockers.push("native-status-remote-content-unexpected".into());
        }
        (SyncEvidenceKind::ProviderApi, CloudProvider::Icloud, _) => {
            blockers.push("icloud-provider-api-unsupported".into());
        }
        (SyncEvidenceKind::ProviderApi, _, None) => {
            blockers.push("remote-content-proof-missing".into());
        }
        (SyncEvidenceKind::ProviderApi, provider, Some(proof)) => {
            if proof.object_id.trim().is_empty() {
                blockers.push("remote-object-id-missing".into());
            }
            if proof.revision.trim().is_empty() {
                blockers.push("remote-revision-missing".into());
            }
            if !proof.location_bound {
                blockers.push("remote-location-unbound".into());
            } else if proof
                .location_proof
                .as_deref()
                .is_none_or(|value| value.trim().is_empty())
            {
                blockers.push("remote-location-proof-missing".into());
            } else {
                let expected_prefix = match provider {
                    CloudProvider::Onedrive => "onedrive-path-v1:",
                    CloudProvider::GoogleDrive => "google-drive-parent-chain-v1:",
                    CloudProvider::Icloud => "",
                };
                let valid = proof
                    .location_proof
                    .as_deref()
                    .and_then(|value| value.strip_prefix(expected_prefix))
                    .is_some_and(|digest| {
                        digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
                    });
                if !valid {
                    blockers.push("remote-location-proof-invalid".into());
                }
            }
            let checksum_matches = match (provider, proof.algorithm) {
                (CloudProvider::Onedrive, RemoteChecksumAlgorithm::QuickXor) => {
                    proof.checksum == receipt.quick_xor_base64
                }
                (CloudProvider::GoogleDrive, RemoteChecksumAlgorithm::Sha256) => {
                    proof.checksum.eq_ignore_ascii_case(&receipt.sha256)
                }
                _ => false,
            };
            if !checksum_matches {
                blockers.push("remote-checksum-mismatch".into());
            }
        }
    }
    if !blockers.is_empty() {
        return Err(blockers);
    }
    Ok(LocalEvictionPermit {
        receipt_id: receipt.receipt_id.clone(),
        provider: receipt.provider,
        source: receipt.source.clone(),
        destination: receipt.destination.clone(),
        bytes: receipt.bytes,
        blake3: receipt.blake3.clone(),
        approved_at_ms: evidence.confirmed_at_ms,
        evidence_kind: evidence.kind,
        evidence_id: evidence.evidence_id.clone(),
        evidence_record_id: evidence_record.record_id.clone(),
    })
}

#[cfg(not(coverage))]
fn modified_ms(metadata: &std::fs::Metadata) -> Result<u64, String> {
    metadata
        .modified()
        .map_err(|error| error.to_string())?
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .map_err(|error| error.to_string())
}

#[cfg(not(coverage))]
fn hash_file(path: &Path) -> Result<ContentDigests, String> {
    let mut file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let mut hasher = ContentHasher::default();
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize())
}

#[cfg(not(coverage))]
fn remove_created_file(path: &Path) {
    let _ = std::fs::remove_file(path);
}

#[cfg(not(coverage))]
fn copy_and_verify(
    candidate: &CloudCandidate,
    cloud_root: &CloudRoot,
) -> Result<(u64, ContentDigests), String> {
    let source = Path::new(&candidate.src);
    let destination = Path::new(&candidate.dst);
    let before = std::fs::symlink_metadata(source).map_err(|error| error.to_string())?;
    if before.file_type().is_symlink() || !before.is_file() {
        return Err("source-must-be-regular-file".into());
    }
    let before_modified_ms = modified_ms(&before)?;
    if before.len() != candidate.bytes || before_modified_ms != candidate.modified_ms {
        return Err("source-changed-since-plan".into());
    }
    if destination.exists() {
        return Err("destination-already-exists".into());
    }
    let parent = destination
        .parent()
        .ok_or_else(|| "destination-parent-missing".to_string())?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let canonical_root =
        std::fs::canonicalize(&cloud_root.path).map_err(|error| error.to_string())?;
    let canonical_parent = std::fs::canonicalize(parent).map_err(|error| error.to_string())?;
    let canonical_source = std::fs::canonicalize(source).map_err(|error| error.to_string())?;
    if !canonical_parent.starts_with(&canonical_root) {
        return Err("destination-parent-escapes-cloud-root".into());
    }
    if canonical_source.starts_with(&canonical_root) {
        return Err("source-already-in-cloud-root".into());
    }

    let mut source_file = std::fs::File::open(source).map_err(|error| error.to_string())?;
    let mut destination_file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|error| error.to_string())?;

    let copy_result = (|| -> Result<(u64, ContentDigests), String> {
        let mut source_hasher = ContentHasher::default();
        let mut copied = 0_u64;
        let mut buffer = vec![0_u8; 1024 * 1024];
        loop {
            let read = source_file
                .read(&mut buffer)
                .map_err(|error| error.to_string())?;
            if read == 0 {
                break;
            }
            destination_file
                .write_all(&buffer[..read])
                .map_err(|error| error.to_string())?;
            source_hasher.update(&buffer[..read]);
            copied = copied.saturating_add(read as u64);
        }
        destination_file
            .sync_all()
            .map_err(|error| error.to_string())?;
        drop(destination_file);

        let streamed_hashes = source_hasher.finalize();
        let source_hashes = hash_file(source)?;
        let destination_hashes = hash_file(destination)?;
        let after = std::fs::symlink_metadata(source).map_err(|error| error.to_string())?;
        let unchanged = after.is_file()
            && !after.file_type().is_symlink()
            && after.len() == before.len()
            && modified_ms(&after)? == before_modified_ms;
        let destination_len = std::fs::metadata(destination)
            .map_err(|error| error.to_string())?
            .len();
        if !unchanged
            || copied != candidate.bytes
            || destination_len != candidate.bytes
            || streamed_hashes != source_hashes
            || source_hashes != destination_hashes
        {
            return Err("copy-verification-failed".into());
        }
        Ok((copied, destination_hashes))
    })();

    if copy_result.is_err() {
        remove_created_file(destination);
    }
    copy_result
}

#[cfg(not(coverage))]
fn verify_existing_destination(
    candidate: &CloudCandidate,
    cloud_root: &CloudRoot,
) -> Result<ContentDigests, String> {
    let source = Path::new(&candidate.src);
    let destination = Path::new(&candidate.dst);
    let source_before = std::fs::symlink_metadata(source).map_err(|error| error.to_string())?;
    if source_before.file_type().is_symlink() || !source_before.is_file() {
        return Err("source-must-be-regular-file".into());
    }
    let source_modified_ms = modified_ms(&source_before)?;
    if source_before.len() != candidate.bytes || source_modified_ms != candidate.modified_ms {
        return Err("source-changed-since-plan".into());
    }

    let destination_before = std::fs::symlink_metadata(destination)
        .map_err(|_| "existing-destination-missing".to_string())?;
    if destination_before.file_type().is_symlink() || !destination_before.is_file() {
        return Err("existing-destination-must-be-regular-file".into());
    }
    if destination_before.len() != candidate.bytes {
        return Err("existing-destination-size-mismatch".into());
    }
    let destination_modified = destination_before.modified().ok();

    let canonical_root =
        std::fs::canonicalize(&cloud_root.path).map_err(|error| error.to_string())?;
    let canonical_source = std::fs::canonicalize(source).map_err(|error| error.to_string())?;
    let canonical_destination =
        std::fs::canonicalize(destination).map_err(|error| error.to_string())?;
    if canonical_source.starts_with(&canonical_root) {
        return Err("source-already-in-cloud-root".into());
    }
    if !canonical_destination.starts_with(&canonical_root) {
        return Err("existing-destination-escapes-cloud-root".into());
    }

    // Opening a File Provider placeholder can materialize it. This happens only after an explicit
    // adoption action and is required to prove byte identity before a receipt can be issued.
    let source_hashes = hash_file(source)?;
    let destination_hashes = hash_file(destination)?;

    let source_after = std::fs::symlink_metadata(source).map_err(|error| error.to_string())?;
    let destination_after =
        std::fs::symlink_metadata(destination).map_err(|error| error.to_string())?;
    let source_unchanged = source_after.is_file()
        && !source_after.file_type().is_symlink()
        && source_after.len() == source_before.len()
        && modified_ms(&source_after)? == source_modified_ms;
    let destination_unchanged = destination_after.is_file()
        && !destination_after.file_type().is_symlink()
        && destination_after.len() == destination_before.len()
        && destination_after.modified().ok() == destination_modified;
    if !source_unchanged || !destination_unchanged {
        return Err("existing-copy-changed-during-verification".into());
    }
    if source_hashes != destination_hashes {
        return Err("existing-destination-content-mismatch".into());
    }
    Ok(destination_hashes)
}

#[cfg(not(coverage))]
fn write_immutable_receipt(
    receipt: &CloudCopyReceipt,
    receipt_dir: &Path,
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(receipt_dir).map_err(|error| error.to_string())?;
    let directory_metadata = std::fs::symlink_metadata(receipt_dir)
        .map_err(|_| "receipt-directory-metadata-failed".to_string())?;
    if !directory_metadata.is_dir() || directory_metadata.file_type().is_symlink() {
        return Err("receipt-directory-unsafe".into());
    }
    let path = receipt_dir.join(format!("{}.json", receipt.receipt_id));
    let encoded = serde_json::to_vec_pretty(receipt).map_err(|error| error.to_string())?;
    if encoded.len() as u64 > MAX_RECEIPT_BYTES {
        return Err("receipt-too-large".into());
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|error| error.to_string())?;
    let result = (|| -> Result<(), String> {
        file.write_all(&encoded)
            .map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        let mut permissions = file
            .metadata()
            .map_err(|error| error.to_string())?
            .permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            permissions.set_mode(0o400);
        }
        #[cfg(not(unix))]
        permissions.set_readonly(true);
        std::fs::set_permissions(&path, permissions).map_err(|error| error.to_string())?;
        #[cfg(unix)]
        std::fs::File::open(receipt_dir)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| error.to_string())?;
        Ok(())
    })();
    if let Err(error) = result {
        drop(file);
        remove_created_file(&path);
        return Err(error);
    }
    Ok(path)
}

#[cfg(not(coverage))]
fn build_verified_receipt(
    candidate: &CloudCandidate,
    review_decision: Option<&CloudReviewDecision>,
    hashes: ContentDigests,
    verified_at_ms: u64,
    copy_verification_method: CloudCopyVerificationMethod,
) -> Result<CloudCopyReceipt, String> {
    let lineage = lineage_snapshot(candidate, review_decision, copy_verification_method);
    let lineage_fingerprint = lineage_fingerprint(&lineage)?;
    let mut receipt = CloudCopyReceipt {
        version: RECEIPT_VERSION,
        receipt_id: String::new(),
        candidate_fingerprint: candidate.metadata_fingerprint.clone(),
        provider: candidate.provider,
        source: candidate.src.clone(),
        destination: candidate.dst.clone(),
        bytes: candidate.bytes,
        blake3: hashes.blake3,
        sha256: hashes.sha256,
        quick_xor_base64: hashes.quick_xor_base64,
        source_modified_ms: candidate.modified_ms,
        copied_at_ms: verified_at_ms,
        copy_verified: true,
        provider_sync_confirmed: false,
        lineage_fingerprint: Some(lineage_fingerprint),
        lineage: Some(lineage),
    };
    receipt.receipt_id = receipt_id_for(
        receipt.version,
        &receipt.candidate_fingerprint,
        receipt.provider,
        &receipt.source,
        &receipt.destination,
        receipt.bytes,
        &receipt.blake3,
        &receipt.sha256,
        &receipt.quick_xor_base64,
        receipt.source_modified_ms,
        receipt.copied_at_ms,
        receipt.copy_verified,
        receipt.provider_sync_confirmed,
        receipt.lineage_fingerprint.as_deref(),
    );
    Ok(receipt)
}

/// Copy a pre-approved candidate into its cloud root and persist an immutable verification
/// receipt. The source is never removed, even when receipt persistence fails.
#[cfg(not(coverage))]
pub fn prepare_cloud_copy(
    candidate: &CloudCandidate,
    cloud_root: &CloudRoot,
    receipt_dir: &Path,
    copied_at_ms: u64,
) -> Result<(CloudCopyReceipt, PathBuf), String> {
    prepare_cloud_copy_with_review(candidate, cloud_root, receipt_dir, copied_at_ms, None)
}

/// Copy a candidate after validating an optional operator review decision. Approval can clear only
/// the `review-required` gate; embedded high-confidence dates and all path/provider/planner gates
/// remain mandatory.
#[cfg(not(coverage))]
pub fn prepare_cloud_copy_with_review(
    candidate: &CloudCandidate,
    cloud_root: &CloudRoot,
    receipt_dir: &Path,
    copied_at_ms: u64,
    review_decision: Option<&CloudReviewDecision>,
) -> Result<(CloudCopyReceipt, PathBuf), String> {
    let blockers = candidate_blockers_with_review(candidate, cloud_root, review_decision);
    if !blockers.is_empty() {
        return Err(blockers.join(","));
    }
    let (_, hashes) = copy_and_verify(candidate, cloud_root)?;
    let receipt = build_verified_receipt(
        candidate,
        review_decision,
        hashes,
        copied_at_ms,
        CloudCopyVerificationMethod::CopiedByDiskSage,
    )?;
    match write_immutable_receipt(&receipt, receipt_dir) {
        Ok(path) => Ok((receipt, path)),
        Err(error) => {
            remove_created_file(Path::new(&candidate.dst));
            Err(error)
        }
    }
}

/// Verify and adopt a destination that already exists under the selected cloud root. Neither file
/// is modified or removed. A receipt is issued only when the fresh planner reported exactly
/// `destination-exists` and all three content digests match.
#[cfg(not(coverage))]
pub fn adopt_existing_cloud_copy(
    candidate: &CloudCandidate,
    cloud_root: &CloudRoot,
    receipt_dir: &Path,
    verified_at_ms: u64,
) -> Result<(CloudCopyReceipt, PathBuf), String> {
    adopt_existing_cloud_copy_with_review(candidate, cloud_root, receipt_dir, verified_at_ms, None)
}

#[cfg(not(coverage))]
pub fn adopt_existing_cloud_copy_with_review(
    candidate: &CloudCandidate,
    cloud_root: &CloudRoot,
    receipt_dir: &Path,
    verified_at_ms: u64,
    review_decision: Option<&CloudReviewDecision>,
) -> Result<(CloudCopyReceipt, PathBuf), String> {
    let blockers =
        existing_copy_candidate_blockers_with_review(candidate, cloud_root, review_decision);
    if !blockers.is_empty() {
        return Err(blockers.join(","));
    }
    let hashes = verify_existing_destination(candidate, cloud_root)?;
    let receipt = build_verified_receipt(
        candidate,
        review_decision,
        hashes,
        verified_at_ms,
        CloudCopyVerificationMethod::AdoptedExisting,
    )?;
    let path = write_immutable_receipt(&receipt, receipt_dir)?;
    Ok((receipt, path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::{ArchiveKind, MetadataEvidence};
    use crate::provider_evidence::{
        create_sync_evidence_record, ProviderSyncEvidenceRecord, PROVIDER_EVIDENCE_RECORD_VERSION,
    };

    #[cfg(windows)]
    const ROOT: &str = r"C:\cloud";
    #[cfg(not(windows))]
    const ROOT: &str = "/cloud";
    #[cfg(windows)]
    const SOURCE: &str = r"C:\source\report.pdf";
    #[cfg(not(windows))]
    const SOURCE: &str = "/source/report.pdf";
    #[cfg(windows)]
    const DESTINATION: &str = r"C:\cloud\DiskSage Archive\report.pdf";
    #[cfg(not(windows))]
    const DESTINATION: &str = "/cloud/DiskSage Archive/report.pdf";

    fn approve_evidence(
        receipt: &CloudCopyReceipt,
        evidence: &ProviderSyncEvidence,
    ) -> Result<LocalEvictionPermit, Vec<String>> {
        let record =
            create_sync_evidence_record(evidence).unwrap_or_else(|_| ProviderSyncEvidenceRecord {
                version: PROVIDER_EVIDENCE_RECORD_VERSION,
                record_id: "0".repeat(64),
                evidence: evidence.clone(),
            });
        approve_local_eviction(receipt, &record)
    }

    fn root() -> CloudRoot {
        CloudRoot {
            id: "icloud:test".into(),
            provider: CloudProvider::Icloud,
            account_scope: CloudAccountScope::Organization,
            label: "iCloud Drive".into(),
            path: ROOT.into(),
            readable: true,
            access_issue: None,
        }
    }

    fn candidate() -> CloudCandidate {
        let mut candidate = CloudCandidate {
            metadata_fingerprint: "a".repeat(64),
            review_fingerprint: String::new(),
            src: SOURCE.into(),
            dst: DESTINATION.into(),
            provider: CloudProvider::Icloud,
            destination_account_scope: CloudAccountScope::Organization,
            kind: ArchiveKind::Document,
            bytes: 12,
            age_days: 90,
            created_ms: 1,
            modified_ms: 2,
            production_time_ms: 3,
            production_time_source: "embedded:exiftool:CreateDate".into(),
            production_time_confidence: "high".into(),
            source_root: SOURCE.into(),
            relative_path: "report.pdf".into(),
            source_context: "source".into(),
            requires_review: false,
            review_reasons: Vec::new(),
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
            blocked_reason: None,
        };
        candidate.review_fingerprint = candidate_review_fingerprint(&candidate);
        candidate
    }

    fn refresh_review_fingerprint(candidate: &mut CloudCandidate) {
        candidate.review_fingerprint = candidate_review_fingerprint(candidate);
    }

    fn receipt() -> CloudCopyReceipt {
        let candidate = candidate();
        let lineage = lineage_snapshot(
            &candidate,
            None,
            CloudCopyVerificationMethod::CopiedByDiskSage,
        );
        let lineage_fingerprint = lineage_fingerprint(&lineage).unwrap();
        let mut receipt = CloudCopyReceipt {
            version: RECEIPT_VERSION,
            receipt_id: String::new(),
            candidate_fingerprint: candidate.metadata_fingerprint,
            provider: CloudProvider::Icloud,
            source: SOURCE.into(),
            destination: DESTINATION.into(),
            bytes: 12,
            blake3: "b".repeat(64),
            sha256: "sha256-hash".into(),
            quick_xor_base64: "quick-xor".into(),
            source_modified_ms: 2,
            copied_at_ms: 100,
            copy_verified: true,
            provider_sync_confirmed: false,
            lineage_fingerprint: Some(lineage_fingerprint),
            lineage: Some(lineage),
        };
        receipt.receipt_id = receipt_id_for(
            receipt.version,
            &receipt.candidate_fingerprint,
            receipt.provider,
            &receipt.source,
            &receipt.destination,
            receipt.bytes,
            &receipt.blake3,
            &receipt.sha256,
            &receipt.quick_xor_base64,
            receipt.source_modified_ms,
            receipt.copied_at_ms,
            receipt.copy_verified,
            receipt.provider_sync_confirmed,
            receipt.lineage_fingerprint.as_deref(),
        );
        receipt
    }

    fn receipt_for(provider: CloudProvider) -> CloudCopyReceipt {
        let mut provider_receipt = receipt();
        provider_receipt.provider = provider;
        provider_receipt.receipt_id = receipt_id_for(
            provider_receipt.version,
            &provider_receipt.candidate_fingerprint,
            provider_receipt.provider,
            &provider_receipt.source,
            &provider_receipt.destination,
            provider_receipt.bytes,
            &provider_receipt.blake3,
            &provider_receipt.sha256,
            &provider_receipt.quick_xor_base64,
            provider_receipt.source_modified_ms,
            provider_receipt.copied_at_ms,
            provider_receipt.copy_verified,
            provider_receipt.provider_sync_confirmed,
            provider_receipt.lineage_fingerprint.as_deref(),
        );
        provider_receipt
    }

    fn legacy_receipt() -> CloudCopyReceipt {
        let mut legacy = receipt();
        legacy.version = LEGACY_RECEIPT_VERSION;
        legacy.lineage_fingerprint = None;
        legacy.lineage = None;
        legacy.receipt_id = receipt_id_for(
            legacy.version,
            &legacy.candidate_fingerprint,
            legacy.provider,
            &legacy.source,
            &legacy.destination,
            legacy.bytes,
            &legacy.blake3,
            &legacy.sha256,
            &legacy.quick_xor_base64,
            legacy.source_modified_ms,
            legacy.copied_at_ms,
            legacy.copy_verified,
            legacy.provider_sync_confirmed,
            None,
        );
        legacy
    }

    fn evidence() -> ProviderSyncEvidence {
        ProviderSyncEvidence {
            receipt_id: receipt().receipt_id,
            provider: CloudProvider::Icloud,
            destination: DESTINATION.into(),
            observed_bytes: 12,
            destination_blake3: "b".repeat(64),
            confirmed_at_ms: 101,
            kind: SyncEvidenceKind::ProviderNativeStatus,
            evidence_id: "icloud-uploaded-flag".into(),
            sync_complete: true,
            remote_content: None,
        }
    }

    #[test]
    fn candidate_gate_accepts_only_embedded_high_confidence_safe_paths() {
        let accepted = candidate();
        assert!(candidate_blockers(&accepted, &root()).is_empty());
        assert_eq!(receipt().receipt_id.len(), 64);
        assert!(receipt_integrity_valid(&receipt()));

        let mut rejected = accepted;
        rejected.requires_review = true;
        rejected.blocked_reason = Some("blocked".into());
        rejected.production_time_source = "filesystem:created".into();
        rejected.production_time_confidence = "low".into();
        rejected.metadata_fingerprint = " ".into();
        rejected.provider = CloudProvider::Onedrive;
        rejected.src = rejected.dst.clone();
        rejected.dst = SOURCE.into();
        let mut unsafe_root = root();
        unsafe_root.path = "relative/cloud".into();
        let blockers = candidate_blockers(&rejected, &unsafe_root);
        for expected in [
            "review-required",
            "planner-blocked",
            "embedded-high-confidence-date-required",
            "metadata-fingerprint-missing",
            "provider-mismatch",
            "cloud-root-not-safe-absolute",
            "destination-outside-cloud-root",
        ] {
            assert!(blockers.contains(&expected.to_string()), "{expected}");
        }

        let mut same_path = candidate();
        same_path.dst = same_path.src.clone();
        assert!(candidate_blockers(&same_path, &root())
            .contains(&"source-equals-destination".to_string()));

        let mut changed_scope = root();
        changed_scope.account_scope = CloudAccountScope::Personal;
        assert!(candidate_blockers(&candidate(), &changed_scope)
            .contains(&"destination-account-scope-mismatch".to_string()));

        let mut unsafe_paths = candidate();
        unsafe_paths.src = "relative/../source".into();
        unsafe_paths.dst = "relative/../destination".into();
        let blockers = candidate_blockers(&unsafe_paths, &root());
        assert!(blockers.contains(&"source-path-not-safe-absolute".to_string()));
        assert!(blockers.contains(&"destination-path-not-safe-absolute".to_string()));

        let mut already_cloud = candidate();
        already_cloud.src = DESTINATION.into();
        assert!(candidate_blockers(&already_cloud, &root())
            .contains(&"source-already-in-cloud-root".to_string()));
    }

    #[test]
    fn receipt_lineage_is_integrity_bound_and_legacy_v2_remains_valid() {
        let current = receipt();
        assert!(receipt_blockers(&current).is_empty());
        let lineage = current.lineage.as_ref().unwrap();
        assert_eq!(lineage.candidate_fingerprint, current.candidate_fingerprint);
        assert_eq!(
            lineage.production_time_source,
            "embedded:exiftool:CreateDate"
        );
        assert_eq!(lineage.content_title.as_deref(), Some("Report"));
        assert_eq!(lineage.metadata_evidence.len(), 1);

        let mut tampered = current.clone();
        tampered.lineage.as_mut().unwrap().content_title = Some("Tampered".into());
        let blockers = receipt_blockers(&tampered);
        assert!(blockers.contains(&"receipt-lineage-integrity-mismatch".to_string()));

        let mut inconsistent_review = current.clone();
        inconsistent_review.lineage.as_mut().unwrap().reviewed_at_ms = Some(10);
        assert!(receipt_blockers(&inconsistent_review)
            .contains(&"receipt-lineage-review-decision-mismatch".to_string()));

        let mut inconsistent_time = current.clone();
        inconsistent_time.lineage.as_mut().unwrap().modified_ms += 1;
        assert!(receipt_blockers(&inconsistent_time)
            .contains(&"receipt-lineage-modified-time-mismatch".to_string()));

        let mut missing = current;
        missing.lineage = None;
        assert!(receipt_blockers(&missing).contains(&"receipt-lineage-missing".to_string()));

        let legacy = legacy_receipt();
        assert!(receipt_blockers(&legacy).is_empty());
        let encoded = serde_json::to_vec(&legacy).unwrap();
        let decoded: CloudCopyReceipt = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(decoded, legacy);
        assert!(!String::from_utf8(encoded).unwrap().contains("lineage"));
    }

    #[test]
    fn operator_decision_clears_only_the_matching_review_gate() {
        let mut reviewed = candidate();
        reviewed.metadata_fingerprint = "a".repeat(64);
        reviewed.requires_review = true;
        reviewed.review_reasons = vec!["embedded-metadata-probe-incomplete".into()];
        reviewed.review_fingerprint = crate::cloud::candidate_review_fingerprint(&reviewed);
        let approved =
            crate::cloud_review::create_decision(&reviewed, CloudReviewDisposition::Approved, 10)
                .unwrap();
        assert!(candidate_blockers_with_review(&reviewed, &root(), Some(&approved)).is_empty());
        let reviewed_lineage = lineage_snapshot(
            &reviewed,
            Some(&approved),
            CloudCopyVerificationMethod::CopiedByDiskSage,
        );
        assert_eq!(
            reviewed_lineage.review_decision_id.as_deref(),
            Some(approved.decision_id.as_str())
        );
        assert_eq!(
            reviewed_lineage.review_disposition,
            Some(CloudReviewDisposition::Approved)
        );
        assert_eq!(reviewed_lineage.reviewed_at_ms, Some(10));
        assert_eq!(
            reviewed_lineage.review_fingerprint,
            reviewed.review_fingerprint
        );

        let attributed = crate::cloud_review::create_attributed_decision(
            &reviewed,
            CloudReviewDisposition::Approved,
            11,
            "human:local:reviewer",
            "Metadata title, account scope, and destination reviewed.",
        )
        .unwrap();
        let attributed_lineage = lineage_snapshot(
            &reviewed,
            Some(&attributed),
            CloudCopyVerificationMethod::CopiedByDiskSage,
        );
        assert_eq!(
            attributed_lineage.reviewed_by.as_deref(),
            Some("human:local:reviewer")
        );
        assert_eq!(
            attributed_lineage.review_rationale.as_deref(),
            Some("Metadata title, account scope, and destination reviewed.")
        );
        let original_fingerprint = lineage_fingerprint(&attributed_lineage).unwrap();
        let mut changed_attribution = attributed_lineage;
        changed_attribution.review_rationale = Some("Changed rationale".into());
        assert_ne!(
            original_fingerprint,
            lineage_fingerprint(&changed_attribution).unwrap()
        );

        let held =
            crate::cloud_review::create_decision(&reviewed, CloudReviewDisposition::Held, 11)
                .unwrap();
        assert!(
            candidate_blockers_with_review(&reviewed, &root(), Some(&held))
                .contains(&"review-held".to_string())
        );

        let mut changed = reviewed.clone();
        changed.review_reasons.push("new-warning".into());
        changed.review_fingerprint = crate::cloud::candidate_review_fingerprint(&changed);
        assert!(
            candidate_blockers_with_review(&changed, &root(), Some(&approved))
                .contains(&"review-decision-stale".to_string())
        );

        let mut tampered = reviewed.clone();
        tampered.content_title = Some("Changed after review".into());
        assert!(
            candidate_blockers_with_review(&tampered, &root(), Some(&approved))
                .contains(&"review-fingerprint-mismatch".to_string())
        );

        reviewed.production_time_source = "filename:path-token".into();
        reviewed.production_time_confidence = "low".into();
        reviewed.review_fingerprint = crate::cloud::candidate_review_fingerprint(&reviewed);
        let filename_approval =
            crate::cloud_review::create_decision(&reviewed, CloudReviewDisposition::Approved, 12)
                .unwrap();
        assert!(
            candidate_blockers_with_review(&reviewed, &root(), Some(&filename_approval)).is_empty()
        );

        assert!(candidate_blockers_with_review(&reviewed, &root(), None)
            .contains(&"embedded-high-confidence-date-required".to_string()));

        let filename_hold =
            crate::cloud_review::create_decision(&reviewed, CloudReviewDisposition::Held, 13)
                .unwrap();
        let held_blockers =
            candidate_blockers_with_review(&reviewed, &root(), Some(&filename_hold));
        assert!(held_blockers.contains(&"review-held".to_string()));
        assert!(held_blockers.contains(&"embedded-high-confidence-date-required".to_string()));
    }

    #[test]
    fn provider_sync_evidence_is_required_before_eviction_permit() {
        let valid_receipt = receipt();
        assert!(receipt_blockers(&valid_receipt).is_empty());
        let approved = approve_evidence(&valid_receipt, &evidence()).unwrap();
        assert_eq!(approved.receipt_id, valid_receipt.receipt_id);
        assert_eq!(approved.provider, CloudProvider::Icloud);
        assert_eq!(approved.source, SOURCE);
        assert_eq!(approved.destination, DESTINATION);
        assert_eq!(approved.bytes, 12);
        assert_eq!(approved.blake3, "b".repeat(64));
        assert_eq!(approved.approved_at_ms, 101);
        assert_eq!(
            approved.evidence_kind,
            SyncEvidenceKind::ProviderNativeStatus
        );
        assert_eq!(approved.evidence_id, "icloud-uploaded-flag");
        assert_eq!(approved.evidence_record_id.len(), 64);

        let mut invalid_receipt = receipt();
        invalid_receipt.version = 99;
        invalid_receipt.copy_verified = false;
        invalid_receipt.provider_sync_confirmed = true;
        invalid_receipt.source = "relative/../source".into();
        invalid_receipt.destination = invalid_receipt.source.clone();
        let mut invalid_evidence = evidence();
        invalid_evidence.sync_complete = false;
        invalid_evidence.receipt_id = "other".into();
        invalid_evidence.provider = CloudProvider::GoogleDrive;
        invalid_evidence.destination = "other".into();
        invalid_evidence.observed_bytes = 99;
        invalid_evidence.destination_blake3 = "other-hash".into();
        invalid_evidence.confirmed_at_ms = 1;
        invalid_evidence.kind = SyncEvidenceKind::ProviderApi;
        invalid_evidence.evidence_id = " ".into();
        invalid_evidence.remote_content = None;
        let blockers = approve_evidence(&invalid_receipt, &invalid_evidence).unwrap_err();
        for expected in [
            "receipt-version-unsupported",
            "receipt-integrity-mismatch",
            "copy-not-verified",
            "receipt-already-consumed",
            "receipt-source-path-not-safe-absolute",
            "receipt-destination-path-not-safe-absolute",
            "receipt-source-equals-destination",
            "provider-sync-incomplete",
            "receipt-id-mismatch",
            "provider-mismatch",
            "destination-mismatch",
            "remote-size-mismatch",
            "destination-hash-mismatch",
            "sync-evidence-predates-copy",
            "sync-evidence-id-missing",
            "icloud-provider-api-unsupported",
        ] {
            assert!(blockers.contains(&expected.to_string()), "{expected}");
        }
    }

    #[test]
    fn eviction_permit_rejects_tampered_evidence_record() {
        let valid_receipt = receipt();
        let mut record = create_sync_evidence_record(&evidence()).unwrap();
        let expected_record_id = record.record_id.clone();
        let permit = approve_local_eviction(&valid_receipt, &record).unwrap();
        assert_eq!(permit.evidence_record_id, expected_record_id);

        record.evidence.confirmed_at_ms += 1;
        assert!(approve_local_eviction(&valid_receipt, &record)
            .unwrap_err()
            .contains(&"provider-evidence-record-integrity-mismatch".to_string()));
    }

    #[test]
    fn provider_api_evidence_requires_provider_specific_remote_checksum() {
        for (provider, algorithm, checksum) in [
            (
                CloudProvider::Onedrive,
                RemoteChecksumAlgorithm::QuickXor,
                "quick-xor",
            ),
            (
                CloudProvider::GoogleDrive,
                RemoteChecksumAlgorithm::Sha256,
                "SHA256-HASH",
            ),
        ] {
            let provider_receipt = receipt_for(provider);
            let api_evidence = ProviderSyncEvidence {
                receipt_id: provider_receipt.receipt_id.clone(),
                provider,
                destination: provider_receipt.destination.clone(),
                observed_bytes: provider_receipt.bytes,
                destination_blake3: provider_receipt.blake3.clone(),
                confirmed_at_ms: 101,
                kind: SyncEvidenceKind::ProviderApi,
                evidence_id: "authenticated-provider-response".into(),
                sync_complete: true,
                remote_content: Some(RemoteContentProof {
                    object_id: "remote-id".into(),
                    revision: "revision-1".into(),
                    algorithm,
                    checksum: checksum.into(),
                    location_bound: true,
                    location_proof: Some(format!(
                        "{}{}",
                        match provider {
                            CloudProvider::Onedrive => "onedrive-path-v1:",
                            CloudProvider::GoogleDrive => "google-drive-parent-chain-v1:",
                            CloudProvider::Icloud => unreachable!(),
                        },
                        "a".repeat(64)
                    )),
                }),
            };
            assert!(approve_evidence(&provider_receipt, &api_evidence).is_ok());
        }
    }

    #[test]
    fn provider_api_evidence_rejects_missing_or_wrong_remote_proof() {
        let provider_receipt = receipt_for(CloudProvider::Onedrive);
        let mut api_evidence = ProviderSyncEvidence {
            receipt_id: provider_receipt.receipt_id.clone(),
            provider: CloudProvider::Onedrive,
            destination: provider_receipt.destination.clone(),
            observed_bytes: provider_receipt.bytes,
            destination_blake3: provider_receipt.blake3.clone(),
            confirmed_at_ms: 101,
            kind: SyncEvidenceKind::ProviderApi,
            evidence_id: "authenticated-provider-response".into(),
            sync_complete: true,
            remote_content: None,
        };
        assert!(approve_evidence(&provider_receipt, &api_evidence)
            .unwrap_err()
            .contains(&"remote-content-proof-missing".to_string()));

        api_evidence.remote_content = Some(RemoteContentProof {
            object_id: " ".into(),
            revision: " ".into(),
            algorithm: RemoteChecksumAlgorithm::Sha256,
            checksum: "wrong".into(),
            location_bound: false,
            location_proof: None,
        });
        let blockers = approve_evidence(&provider_receipt, &api_evidence).unwrap_err();
        for expected in [
            "remote-object-id-missing",
            "remote-revision-missing",
            "remote-location-unbound",
            "remote-checksum-mismatch",
        ] {
            assert!(blockers.contains(&expected.to_string()), "{expected}");
        }

        api_evidence.remote_content = Some(RemoteContentProof {
            object_id: "remote-id".into(),
            revision: "revision-1".into(),
            algorithm: RemoteChecksumAlgorithm::QuickXor,
            checksum: "quick-xor".into(),
            location_bound: true,
            location_proof: None,
        });
        assert!(approve_evidence(&provider_receipt, &api_evidence)
            .unwrap_err()
            .contains(&"remote-location-proof-missing".to_string()));

        api_evidence.remote_content = Some(RemoteContentProof {
            object_id: "remote-id".into(),
            revision: "revision-1".into(),
            algorithm: RemoteChecksumAlgorithm::QuickXor,
            checksum: "quick-xor".into(),
            location_bound: true,
            location_proof: Some("onedrive-path-v1:not-a-valid-digest".into()),
        });
        assert!(approve_evidence(&provider_receipt, &api_evidence)
            .unwrap_err()
            .contains(&"remote-location-proof-invalid".to_string()));

        api_evidence.kind = SyncEvidenceKind::ProviderNativeStatus;
        api_evidence.remote_content = None;
        assert!(approve_evidence(&provider_receipt, &api_evidence).is_ok());

        api_evidence.remote_content = Some(RemoteContentProof {
            object_id: "remote-id".into(),
            revision: "revision-1".into(),
            algorithm: RemoteChecksumAlgorithm::QuickXor,
            checksum: "quick-xor".into(),
            location_bound: true,
            location_proof: Some(format!("onedrive-path-v1:{}", "a".repeat(64))),
        });
        assert!(approve_evidence(&provider_receipt, &api_evidence)
            .unwrap_err()
            .contains(&"native-status-remote-content-unexpected".to_string()));
    }

    #[cfg(not(coverage))]
    #[test]
    fn verified_copy_keeps_source_and_writes_read_only_receipt() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("source/report.pdf");
        let cloud = tmp.path().join("cloud");
        let destination = cloud.join("DiskSage Archive/report.pdf");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::create_dir_all(&cloud).unwrap();
        std::fs::write(&source, b"hello-cloud").unwrap();
        let metadata = std::fs::metadata(&source).unwrap();
        let mut test_candidate = candidate();
        test_candidate.src = source.to_string_lossy().into_owned();
        test_candidate.dst = destination.to_string_lossy().into_owned();
        test_candidate.bytes = metadata.len();
        test_candidate.modified_ms = modified_ms(&metadata).unwrap();
        refresh_review_fingerprint(&mut test_candidate);
        let test_root = CloudRoot {
            id: "icloud:test".into(),
            provider: CloudProvider::Icloud,
            account_scope: CloudAccountScope::Organization,
            label: "iCloud Drive".into(),
            path: cloud.to_string_lossy().into_owned(),
            readable: true,
            access_issue: None,
        };
        let receipt_dir = tmp.path().join("receipts");
        let (copy_receipt, receipt_path) =
            prepare_cloud_copy(&test_candidate, &test_root, &receipt_dir, 123).unwrap();
        assert!(source.exists());
        assert_eq!(std::fs::read(&destination).unwrap(), b"hello-cloud");
        assert_eq!(copy_receipt.blake3, hash_file(&source).unwrap().blake3);
        assert!(receipt_path.metadata().unwrap().permissions().readonly());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                receipt_path.metadata().unwrap().permissions().mode() & 0o777,
                0o400
            );
        }
        let persisted = read_immutable_receipt(&receipt_path).unwrap();
        assert_eq!(persisted, copy_receipt);
        let lineage = persisted.lineage.as_ref().unwrap();
        assert_eq!(persisted.version, RECEIPT_VERSION);
        assert_eq!(
            lineage.review_fingerprint,
            test_candidate.review_fingerprint
        );
        assert_eq!(
            lineage.production_time_ms,
            test_candidate.production_time_ms
        );
        assert_eq!(lineage.metadata_evidence, test_candidate.metadata_evidence);
        assert_eq!(lineage.review_decision_id, None);

        let wrong_name = receipt_dir.join("wrong-name.json");
        std::fs::copy(&receipt_path, &wrong_name).unwrap();
        let mut permissions = std::fs::metadata(&wrong_name).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&wrong_name, permissions).unwrap();
        assert_eq!(
            read_immutable_receipt(&wrong_name).unwrap_err(),
            "receipt-filename-id-mismatch"
        );
    }

    #[cfg(not(coverage))]
    #[test]
    fn identical_existing_destination_is_adopted_without_modifying_either_file() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("source/report.pdf");
        let cloud = tmp.path().join("cloud");
        let destination = cloud.join("DiskSage Archive/report.pdf");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
        std::fs::write(&source, b"already-in-cloud").unwrap();
        std::fs::write(&destination, b"already-in-cloud").unwrap();
        let metadata = std::fs::metadata(&source).unwrap();
        let mut test_candidate = candidate();
        test_candidate.src = source.to_string_lossy().into_owned();
        test_candidate.dst = destination.to_string_lossy().into_owned();
        test_candidate.bytes = metadata.len();
        test_candidate.modified_ms = modified_ms(&metadata).unwrap();
        test_candidate.blocked_reason = Some("destination-exists".into());
        refresh_review_fingerprint(&mut test_candidate);
        let test_root = CloudRoot {
            id: "icloud:test".into(),
            provider: CloudProvider::Icloud,
            account_scope: CloudAccountScope::Organization,
            label: "iCloud Drive".into(),
            path: cloud.to_string_lossy().into_owned(),
            readable: true,
            access_issue: None,
        };

        assert!(candidate_blockers(&test_candidate, &test_root)
            .contains(&"planner-blocked".to_string()));
        assert!(
            existing_copy_candidate_blockers_with_review(&test_candidate, &test_root, None)
                .is_empty()
        );
        let (receipt, receipt_path) = adopt_existing_cloud_copy(
            &test_candidate,
            &test_root,
            &tmp.path().join("receipts"),
            456,
        )
        .unwrap();

        assert_eq!(std::fs::read(&source).unwrap(), b"already-in-cloud");
        assert_eq!(std::fs::read(&destination).unwrap(), b"already-in-cloud");
        assert!(receipt_path.metadata().unwrap().permissions().readonly());
        assert_eq!(
            receipt.lineage.as_ref().unwrap().copy_verification_method,
            CloudCopyVerificationMethod::AdoptedExisting
        );
        assert!(String::from_utf8(std::fs::read(receipt_path).unwrap())
            .unwrap()
            .contains("\"copy_verification_method\": \"adopted-existing\""));
    }

    #[cfg(not(coverage))]
    #[test]
    fn existing_destination_adoption_rejects_mismatch_and_requires_fresh_plan_blocker() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("source.bin");
        let cloud = tmp.path().join("cloud");
        let destination = cloud.join("destination.bin");
        std::fs::create_dir_all(&cloud).unwrap();
        std::fs::write(&source, b"source-a").unwrap();
        std::fs::write(&destination, b"cloud--b").unwrap();
        let metadata = std::fs::metadata(&source).unwrap();
        let mut test_candidate = candidate();
        test_candidate.src = source.to_string_lossy().into_owned();
        test_candidate.dst = destination.to_string_lossy().into_owned();
        test_candidate.bytes = metadata.len();
        test_candidate.modified_ms = modified_ms(&metadata).unwrap();
        test_candidate.blocked_reason = Some("destination-exists".into());
        refresh_review_fingerprint(&mut test_candidate);
        let test_root = CloudRoot {
            id: "icloud:test".into(),
            provider: CloudProvider::Icloud,
            account_scope: CloudAccountScope::Organization,
            label: "iCloud Drive".into(),
            path: cloud.to_string_lossy().into_owned(),
            readable: true,
            access_issue: None,
        };
        let receipt_dir = tmp.path().join("receipts");

        assert_eq!(
            adopt_existing_cloud_copy(&test_candidate, &test_root, &receipt_dir, 456).unwrap_err(),
            "existing-destination-content-mismatch"
        );
        assert_eq!(std::fs::read(&source).unwrap(), b"source-a");
        assert_eq!(std::fs::read(&destination).unwrap(), b"cloud--b");
        assert!(!receipt_dir.exists());

        test_candidate.blocked_reason = None;
        refresh_review_fingerprint(&mut test_candidate);
        assert!(
            existing_copy_candidate_blockers_with_review(&test_candidate, &test_root, None)
                .contains(&"existing-destination-plan-required".to_string())
        );
    }

    #[cfg(all(unix, not(coverage)))]
    #[test]
    fn receipt_write_rejects_oversized_lineage_and_symlink_directory_without_leaving_copy() {
        use std::os::unix::fs::symlink;

        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("source.bin");
        let cloud = tmp.path().join("cloud");
        let destination = cloud.join("destination.bin");
        std::fs::create_dir_all(&cloud).unwrap();
        std::fs::write(&source, b"content").unwrap();
        let metadata = std::fs::metadata(&source).unwrap();
        let mut test_candidate = candidate();
        test_candidate.src = source.to_string_lossy().into_owned();
        test_candidate.dst = destination.to_string_lossy().into_owned();
        test_candidate.bytes = metadata.len();
        test_candidate.modified_ms = modified_ms(&metadata).unwrap();
        test_candidate.metadata_evidence[0].value = "x".repeat(MAX_RECEIPT_BYTES as usize);
        refresh_review_fingerprint(&mut test_candidate);
        let test_root = CloudRoot {
            id: "icloud:test".into(),
            provider: CloudProvider::Icloud,
            account_scope: CloudAccountScope::Organization,
            label: "iCloud Drive".into(),
            path: cloud.to_string_lossy().into_owned(),
            readable: true,
            access_issue: None,
        };

        assert_eq!(
            prepare_cloud_copy(
                &test_candidate,
                &test_root,
                &tmp.path().join("receipts"),
                123,
            )
            .unwrap_err(),
            "receipt-too-large"
        );
        assert!(source.exists());
        assert!(!destination.exists());

        test_candidate.metadata_evidence[0].value = "bounded".into();
        refresh_review_fingerprint(&mut test_candidate);
        let real_receipt_dir = tmp.path().join("real-receipts");
        let receipt_link = tmp.path().join("receipt-link");
        std::fs::create_dir(&real_receipt_dir).unwrap();
        symlink(&real_receipt_dir, &receipt_link).unwrap();
        assert_eq!(
            prepare_cloud_copy(&test_candidate, &test_root, &receipt_link, 124).unwrap_err(),
            "receipt-directory-unsafe"
        );
        assert!(source.exists());
        assert!(!destination.exists());
        assert!(std::fs::read_dir(real_receipt_dir)
            .unwrap()
            .next()
            .is_none());
    }

    #[cfg(not(coverage))]
    #[test]
    fn copy_gate_rejects_changed_source_and_existing_destination() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("source.bin");
        let cloud = tmp.path().join("cloud");
        let destination = cloud.join("destination.bin");
        std::fs::create_dir_all(&cloud).unwrap();
        std::fs::write(&source, b"changed").unwrap();
        let mut test_candidate = candidate();
        test_candidate.src = source.to_string_lossy().into_owned();
        test_candidate.dst = destination.to_string_lossy().into_owned();
        test_candidate.bytes = 999;
        test_candidate.modified_ms = modified_ms(&std::fs::metadata(&source).unwrap()).unwrap();
        refresh_review_fingerprint(&mut test_candidate);
        let test_root = CloudRoot {
            id: "icloud:test".into(),
            provider: CloudProvider::Icloud,
            account_scope: CloudAccountScope::Organization,
            label: "iCloud Drive".into(),
            path: cloud.to_string_lossy().into_owned(),
            readable: true,
            access_issue: None,
        };
        let receipt_dir = tmp.path().join("receipts");
        assert_eq!(
            prepare_cloud_copy(&test_candidate, &test_root, &receipt_dir, 123).unwrap_err(),
            "source-changed-since-plan"
        );
        test_candidate.bytes = std::fs::metadata(&source).unwrap().len();
        refresh_review_fingerprint(&mut test_candidate);
        std::fs::write(&destination, b"existing").unwrap();
        assert_eq!(
            prepare_cloud_copy(&test_candidate, &test_root, &receipt_dir, 124).unwrap_err(),
            "destination-already-exists"
        );
        assert_eq!(std::fs::read(destination).unwrap(), b"existing");
        assert_eq!(std::fs::read(source).unwrap(), b"changed");
    }

    #[cfg(all(unix, not(coverage)))]
    #[test]
    fn copy_gate_rejects_cloud_parent_symlink_escape() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("source.bin");
        let cloud = tmp.path().join("cloud");
        let outside = tmp.path().join("outside");
        std::fs::create_dir_all(&cloud).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(&source, b"content").unwrap();
        std::os::unix::fs::symlink(&outside, cloud.join("DiskSage Archive")).unwrap();
        let metadata = std::fs::metadata(&source).unwrap();
        let destination = cloud.join("DiskSage Archive/escaped.bin");
        let mut test_candidate = candidate();
        test_candidate.src = source.to_string_lossy().into_owned();
        test_candidate.dst = destination.to_string_lossy().into_owned();
        test_candidate.bytes = metadata.len();
        test_candidate.modified_ms = modified_ms(&metadata).unwrap();
        refresh_review_fingerprint(&mut test_candidate);
        let test_root = CloudRoot {
            id: "icloud:test".into(),
            provider: CloudProvider::Icloud,
            account_scope: CloudAccountScope::Organization,
            label: "iCloud Drive".into(),
            path: cloud.to_string_lossy().into_owned(),
            readable: true,
            access_issue: None,
        };
        assert_eq!(
            prepare_cloud_copy(
                &test_candidate,
                &test_root,
                &tmp.path().join("receipts"),
                123,
            )
            .unwrap_err(),
            "destination-parent-escapes-cloud-root"
        );
        assert!(!outside.join("escaped.bin").exists());
        assert!(source.exists());
    }

    #[cfg(not(coverage))]
    #[test]
    fn preexisting_receipt_is_preserved_and_new_copy_is_rolled_back() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("source.bin");
        let cloud = tmp.path().join("cloud");
        let destination = cloud.join("destination.bin");
        let receipt_dir = tmp.path().join("receipts");
        std::fs::create_dir_all(&cloud).unwrap();
        std::fs::create_dir_all(&receipt_dir).unwrap();
        std::fs::write(&source, b"content").unwrap();
        let metadata = std::fs::metadata(&source).unwrap();
        let mut test_candidate = candidate();
        test_candidate.src = source.to_string_lossy().into_owned();
        test_candidate.dst = destination.to_string_lossy().into_owned();
        test_candidate.bytes = metadata.len();
        test_candidate.modified_ms = modified_ms(&metadata).unwrap();
        refresh_review_fingerprint(&mut test_candidate);
        let test_root = CloudRoot {
            id: "icloud:test".into(),
            provider: CloudProvider::Icloud,
            account_scope: CloudAccountScope::Organization,
            label: "iCloud Drive".into(),
            path: cloud.to_string_lossy().into_owned(),
            readable: true,
            access_issue: None,
        };
        let content_hash = hash_file(&source).unwrap();
        let lineage = lineage_snapshot(
            &test_candidate,
            None,
            CloudCopyVerificationMethod::CopiedByDiskSage,
        );
        let lineage_fingerprint = lineage_fingerprint(&lineage).unwrap();
        let receipt_id = receipt_id_for(
            RECEIPT_VERSION,
            &test_candidate.metadata_fingerprint,
            test_candidate.provider,
            &test_candidate.src,
            &test_candidate.dst,
            test_candidate.bytes,
            &content_hash.blake3,
            &content_hash.sha256,
            &content_hash.quick_xor_base64,
            test_candidate.modified_ms,
            123,
            true,
            false,
            Some(&lineage_fingerprint),
        );
        let existing_receipt = receipt_dir.join(format!("{receipt_id}.json"));
        std::fs::write(&existing_receipt, b"existing-receipt").unwrap();
        assert!(prepare_cloud_copy(&test_candidate, &test_root, &receipt_dir, 123).is_err());
        assert_eq!(
            std::fs::read(existing_receipt).unwrap(),
            b"existing-receipt"
        );
        assert!(!destination.exists());
        assert!(source.exists());
    }
}
