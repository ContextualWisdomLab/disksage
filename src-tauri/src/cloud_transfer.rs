//! Fail-closed cloud transfer safety gates.
//!
//! A verified copy is deliberately not a move. The source remains untouched until a later
//! provider-native synchronization attestation matches the immutable copy receipt. This module
//! produces an eviction permit but intentionally exposes no source deletion API.

use crate::cloud::{
    candidate_review_fingerprint, ArchiveKind, CloudAccountScope, CloudCandidate, CloudProvider,
    CloudRoot, MetadataEvidence, ORGANIZATION_TENANT_AUTHORITY_REVIEW_REASON,
};
use crate::cloud_review::{
    organization_tenant_authority_attested, validate_decision, validate_review_attribution,
    CloudReviewDecision, CloudReviewDisposition, DECISION_VERSION,
};
use crate::dataset_metadata::DatasetProfile;
use crate::provider_evidence::{validate_sync_evidence_record, ProviderSyncEvidenceRecord};
use std::path::Path;

#[cfg(not(coverage))]
use crate::content_digest::{ContentDigests, ContentHasher};
#[cfg(all(not(coverage), not(target_os = "macos")))]
use same_file::Handle;
#[cfg(all(not(coverage), target_os = "macos"))]
use std::ffi::OsStr;
#[cfg(not(coverage))]
use std::io::{Read, Write};
#[cfg(not(coverage))]
use std::path::PathBuf;
#[cfg(not(coverage))]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(all(not(coverage), target_os = "macos"))]
use std::process::{Command, Stdio};
#[cfg(all(not(coverage), target_os = "macos"))]
use std::time::{Duration, Instant};

/// Legacy receipt schema version retained for backward-compatible reads.
pub const LEGACY_RECEIPT_VERSION: u32 = 2;
/// Receipt schema version used before exact action approvals were embedded.
pub const PRE_APPROVAL_RECEIPT_VERSION: u32 = 3;
/// Current immutable cloud-copy receipt schema version.
pub const RECEIPT_VERSION: u32 = 4;
/// Schema version for one exact human cloud-copy approval.
pub const CLOUD_COPY_APPROVAL_VERSION: u32 = 1;
/// Schema version for private local copy-failure journals.
pub const CLOUD_COPY_FAILURE_VERSION: u32 = 1;
/// Maximum age accepted for an exact cloud-copy approval.
pub const MAX_CLOUD_COPY_APPROVAL_AGE_MS: u64 = 15 * 60 * 1000;

/// Return a bounded blocker when the source cannot be safely revalidated for a later eviction.
///
/// This is deliberately separate from receipt integrity: a valid receipt may outlive its local
/// source, and that state must keep the dynamic ADR/Goal projection blocked rather than implying
/// that the source was safely removed.
#[cfg(not(coverage))]
pub fn source_eviction_blocker(source: &Path) -> Option<&'static str> {
    match std::fs::symlink_metadata(source) {
        Ok(metadata) if metadata.file_type().is_symlink() => Some("source-not-regular-file"),
        Ok(metadata) if metadata.is_file() && crate::cloud::metadata_is_dataless(&metadata) => {
            Some("source-content-not-local")
        }
        Ok(metadata) if metadata.is_file() => None,
        Ok(_) => Some("source-not-regular-file"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Some("source-not-present"),
        Err(_) => Some("source-state-unavailable"),
    }
}
#[cfg(not(coverage))]
const MAX_RECEIPT_BYTES: u64 = 64 * 1024;
/// Keep the private diagnostic journal bounded without deleting evidence implicitly.
#[cfg(not(coverage))]
const MAX_CLOUD_COPY_FAILURE_RECORDS: usize = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SyncEvidenceKind {
    ProviderApi,
    ProviderNativeStatus,
}

/// Provider state observed alongside content-bound synchronization evidence.
///
/// A local-current item with `is_uploaded=false` is deliberately represented as
/// `pending-upload`; it is not an incomplete-but-unknown result and never authorizes eviction.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderSyncState {
    Complete,
    PendingUpload,
    NotUbiquitous,
    NotLocalCurrent,
    Uploading,
    ExcludedFromSync,
    SyncPaused,
    RemoteUnavailable,
    ContentMismatch,
    #[default]
    Unknown,
}

impl ProviderSyncState {
    pub fn is_complete(&self) -> bool {
        *self == Self::Complete
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::PendingUpload => "pending-upload",
            Self::NotUbiquitous => "not-ubiquitous",
            Self::NotLocalCurrent => "not-local-current",
            Self::Uploading => "uploading",
            Self::ExcludedFromSync => "excluded-from-sync",
            Self::SyncPaused => "sync-paused",
            Self::RemoteUnavailable => "remote-unavailable",
            Self::ContentMismatch => "content-mismatch",
            Self::Unknown => "unknown",
        }
    }

    pub fn is_unknown(&self) -> bool {
        *self == Self::Unknown
    }
}

/// Runtime state of one metadata-bound cloud offload. This state machine never deletes a source;
/// `EvictionReady` only permits a separately approved OS-Trash operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CloudOffloadGoalState {
    CopyVerified,
    PendingProviderSync,
    ProviderSyncConfirmed,
    EvictionReady,
    SourceEvicted,
}

impl CloudOffloadGoalState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CopyVerified => "copy-verified",
            Self::PendingProviderSync => "pending-provider-sync",
            Self::ProviderSyncConfirmed => "provider-sync-confirmed",
            Self::EvictionReady => "eviction-ready",
            Self::SourceEvicted => "source-evicted",
        }
    }

    pub fn after_attestation(evidence: &ProviderSyncEvidence, permit_available: bool) -> Self {
        if !evidence.sync_complete || !evidence.sync_state.is_complete() {
            return Self::PendingProviderSync;
        }
        if permit_available {
            Self::EvictionReady
        } else {
            Self::ProviderSyncConfirmed
        }
    }
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
    /// The source was uploaded through an authenticated provider API because the local File
    /// Provider could not admit a new copy. The same copy-only approval still binds the action.
    CopiedByProviderApi,
    AdoptedExisting,
}

impl CloudCopyVerificationMethod {
    fn is_copied_by_disksage(&self) -> bool {
        *self == Self::CopiedByDiskSage
    }
}

/// Identifies the exact cloud-copy action authorized by a human reviewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CloudCopyApprovalAction {
    /// Authorize creating a new provider copy while retaining the local source.
    CopyOnly,
    /// Authorize adopting an already-existing destination after digest verification.
    AdoptExistingCopy,
}

impl CloudCopyApprovalAction {
    /// Return the stable kebab-case value stored in receipts and confirmation phrases.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CopyOnly => "copy-only",
            Self::AdoptExistingCopy => "adopt-existing-copy",
        }
    }

    fn accepts_verification_method(self, method: CloudCopyVerificationMethod) -> bool {
        match self {
            Self::CopyOnly => matches!(
                method,
                CloudCopyVerificationMethod::CopiedByDiskSage
                    | CloudCopyVerificationMethod::CopiedByProviderApi
            ),
            Self::AdoptExistingCopy => method == CloudCopyVerificationMethod::AdoptedExisting,
        }
    }
}

/// Fields retained only so lineage fingerprints from older receipts can be revalidated exactly.
/// They are not populated on new receipts; the immutable receipt remains the authority.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct LegacyOntologyRelation {
    subject: String,
    predicate: String,
    object: String,
    source: String,
}

/// A fresh, human-attributed authorization for one exact candidate, destination, and action.
///
/// The candidate review fingerprint binds the source, destination, provider/account scope,
/// production-time evidence, and displayed metadata. A generic confirmation such as `승인` can
/// never satisfy `exact_confirmation_phrase`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CloudCopyApproval {
    /// Version of the approval record schema.
    pub version: u32,
    /// Integrity digest binding every field in this approval.
    pub approval_id: String,
    /// Exact copy or adoption action the reviewer authorized.
    pub action: CloudCopyApprovalAction,
    /// Metadata fingerprint of the candidate shown to the reviewer.
    pub candidate_fingerprint: String,
    /// Review fingerprint binding source, destination, scope, and displayed evidence.
    pub review_fingerprint: String,
    /// Cloud provider that will receive or already contains the destination object.
    pub provider: CloudProvider,
    /// Account boundary in which the destination is located.
    pub destination_account_scope: CloudAccountScope,
    /// Stable identifier of the reviewed cloud root.
    pub cloud_root_id: String,
    /// Millisecond Unix timestamp at which the reviewer approved the action.
    pub approved_at_ms: u64,
    /// Human-attributed reviewer identifier, such as `human:operator-id`.
    pub approved_by: String,
    /// Reviewer-authored explanation for approving this exact action.
    pub rationale: String,
    /// Exact candidate-specific phrase entered by the reviewer.
    pub exact_confirmation_phrase: String,
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
    /// Backward-compatible v3 lineage fields from the pre-Naruon receipt schema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) ontology_class: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) ontology_relations: Option<Vec<LegacyOntologyRelation>>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) capacity: Option<crate::provider_capacity::CloudCapacityAssessment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub copy_approval: Option<CloudCopyApproval>,
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

/// Private local evidence for a failed copy attempt. It never grants sync or eviction authority.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CloudCopyFailureRecord {
    pub version: u32,
    pub failure_id: String,
    pub candidate_fingerprint: String,
    pub provider: CloudProvider,
    pub source: String,
    pub destination: String,
    pub action: CloudCopyApprovalAction,
    pub error_code: String,
    pub occurred_at_ms: u64,
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
    /// Older evidence records omit this field and deserialize as `unknown`.
    #[serde(default, skip_serializing_if = "ProviderSyncState::is_unknown")]
    pub sync_state: ProviderSyncState,
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

fn valid_fingerprint(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn hash_copy_approval_value(hasher: &mut blake3::Hasher, value: &[u8]) {
    hasher.update(&(value.len() as u64).to_le_bytes());
    hasher.update(value);
}

fn copy_approval_id_for(approval: &CloudCopyApproval) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-cloud-copy-approval-v1\0");
    hasher.update(&approval.version.to_le_bytes());
    for value in [
        approval.action.as_str().as_bytes(),
        approval.candidate_fingerprint.as_bytes(),
        approval.review_fingerprint.as_bytes(),
        approval.provider.as_str().as_bytes(),
        approval.destination_account_scope.as_str().as_bytes(),
        approval.cloud_root_id.as_bytes(),
        approval.approved_by.as_bytes(),
        approval.rationale.as_bytes(),
        approval.exact_confirmation_phrase.as_bytes(),
    ] {
        hash_copy_approval_value(&mut hasher, value);
    }
    hash_copy_approval_value(&mut hasher, &approval.approved_at_ms.to_le_bytes());
    hasher.finalize().to_hex().to_string()
}

/// Build the exact phrase a human must enter for one candidate and action.
///
/// The phrase includes the action and current review fingerprint, preventing a generic approval
/// from being replayed for a different source, destination, account scope, or operation.
pub fn cloud_copy_approval_phrase(
    candidate: &CloudCandidate,
    action: CloudCopyApprovalAction,
) -> String {
    format!(
        "DiskSage cloud {} {} 승인",
        action.as_str(),
        candidate.review_fingerprint
    )
}

fn validate_cloud_copy_approval_integrity(approval: &CloudCopyApproval) -> Result<(), String> {
    if approval.version != CLOUD_COPY_APPROVAL_VERSION {
        return Err("cloud-copy-approval-version-unsupported".into());
    }
    if !valid_fingerprint(&approval.approval_id)
        || !valid_fingerprint(&approval.candidate_fingerprint)
        || !valid_fingerprint(&approval.review_fingerprint)
    {
        return Err("cloud-copy-approval-fingerprint-invalid".into());
    }
    validate_review_attribution(&approval.approved_by, &approval.rationale)
        .map_err(|_| "cloud-copy-approval-attribution-invalid".to_string())?;
    if approval.approved_at_ms == 0 {
        return Err("cloud-copy-approval-time-invalid".into());
    }
    if approval.cloud_root_id.trim().is_empty() {
        return Err("cloud-copy-approval-root-id-missing".into());
    }
    if approval.approval_id != copy_approval_id_for(approval) {
        return Err("cloud-copy-approval-integrity-mismatch".into());
    }
    Ok(())
}

/// Create an integrity-bound approval after validating the candidate, destination, actor, and phrase.
///
/// This constructor fails closed when the candidate fingerprint is stale, the cloud root does not
/// match the candidate, the reviewer attribution is incomplete, or the exact phrase differs.
pub fn create_cloud_copy_approval(
    candidate: &CloudCandidate,
    cloud_root: &CloudRoot,
    action: CloudCopyApprovalAction,
    approved_at_ms: u64,
    approved_by: &str,
    rationale: &str,
    exact_confirmation_phrase: &str,
) -> Result<CloudCopyApproval, String> {
    if candidate.review_fingerprint != candidate_review_fingerprint(candidate)
        || !valid_fingerprint(&candidate.metadata_fingerprint)
        || !valid_fingerprint(&candidate.review_fingerprint)
    {
        return Err("cloud-copy-approval-candidate-stale".into());
    }
    if candidate.provider != cloud_root.provider
        || candidate.destination_account_scope != cloud_root.account_scope
    {
        return Err("cloud-copy-approval-destination-mismatch".into());
    }
    let expected_phrase = cloud_copy_approval_phrase(candidate, action);
    if exact_confirmation_phrase != expected_phrase {
        return Err("cloud-copy-exact-confirmation-phrase-mismatch".into());
    }
    validate_review_attribution(approved_by, rationale)
        .map_err(|_| "cloud-copy-approval-attribution-invalid".to_string())?;
    let mut approval = CloudCopyApproval {
        version: CLOUD_COPY_APPROVAL_VERSION,
        approval_id: String::new(),
        action,
        candidate_fingerprint: candidate.metadata_fingerprint.clone(),
        review_fingerprint: candidate.review_fingerprint.clone(),
        provider: candidate.provider,
        destination_account_scope: candidate.destination_account_scope,
        cloud_root_id: cloud_root.id.clone(),
        approved_at_ms,
        approved_by: approved_by.to_string(),
        rationale: rationale.to_string(),
        exact_confirmation_phrase: exact_confirmation_phrase.to_string(),
    };
    approval.approval_id = copy_approval_id_for(&approval);
    validate_cloud_copy_approval_integrity(&approval)?;
    Ok(approval)
}

fn validate_cloud_copy_approval_for_action(
    approval: &CloudCopyApproval,
    candidate: &CloudCandidate,
    cloud_root: &CloudRoot,
    action: CloudCopyApprovalAction,
    action_at_ms: u64,
) -> Result<(), String> {
    validate_cloud_copy_approval_integrity(approval)?;
    if approval.action != action
        || approval.candidate_fingerprint != candidate.metadata_fingerprint
        || approval.review_fingerprint != candidate.review_fingerprint
        || approval.provider != candidate.provider
        || approval.destination_account_scope != candidate.destination_account_scope
        || approval.cloud_root_id != cloud_root.id
        || approval.exact_confirmation_phrase != cloud_copy_approval_phrase(candidate, action)
    {
        return Err("cloud-copy-approval-context-mismatch".into());
    }
    if approval.approved_at_ms > action_at_ms
        || action_at_ms.saturating_sub(approval.approved_at_ms) > MAX_CLOUD_COPY_APPROVAL_AGE_MS
    {
        return Err("cloud-copy-approval-stale".into());
    }
    Ok(())
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
    let organization_tenant_authority_required = candidate.destination_account_scope
        == CloudAccountScope::Organization
        || candidate
            .review_reasons
            .iter()
            .any(|reason| reason == ORGANIZATION_TENANT_AUTHORITY_REVIEW_REASON);

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
            Some(decision) if decision.version != DECISION_VERSION => {
                blockers.push("review-decision-attribution-required".into());
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
            Some(decision)
                if organization_tenant_authority_required
                    && !organization_tenant_authority_attested(decision) =>
            {
                blockers.push("organization-tenant-authority-attestation-required".into());
            }
            Some(_) => exact_review_approved = true,
        }
    }
    if organization_tenant_authority_required && !candidate.requires_review {
        blockers.push("organization-tenant-authority-attestation-required".into());
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
    if version >= PRE_APPROVAL_RECEIPT_VERSION {
        hasher.update(b"\0lineage\0");
        hasher.update(lineage_fingerprint.unwrap_or_default().as_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

fn lineage_snapshot(
    candidate: &CloudCandidate,
    review_decision: Option<&CloudReviewDecision>,
    copy_verification_method: CloudCopyVerificationMethod,
    copy_approval: Option<&CloudCopyApproval>,
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
        ontology_class: None,
        ontology_relations: None,
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
        capacity: None,
        copy_approval: copy_approval.cloned(),
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

/// Validate the exact action approval embedded in a receipt without probing either path.
/// Version 3 receipts predate this approval and remain readable; version 4 requires it.
pub fn validate_receipt_copy_approval(receipt: &CloudCopyReceipt) -> Result<(), String> {
    let lineage = receipt
        .lineage
        .as_ref()
        .ok_or_else(|| "receipt-lineage-copy-approval-lineage-missing".to_string())?;
    let approval = match lineage.copy_approval.as_ref() {
        Some(approval) => approval,
        None if receipt.version == PRE_APPROVAL_RECEIPT_VERSION => return Ok(()),
        None => return Err("receipt-lineage-copy-approval-missing".into()),
    };
    let expected_phrase = format!(
        "DiskSage cloud {} {} 승인",
        approval.action.as_str(),
        lineage.review_fingerprint
    );
    if validate_cloud_copy_approval_integrity(approval).is_err()
        || approval.candidate_fingerprint != receipt.candidate_fingerprint
        || approval.review_fingerprint != lineage.review_fingerprint
        || approval.provider != receipt.provider
        || approval.destination_account_scope != lineage.destination_account_scope
        || !approval
            .action
            .accepts_verification_method(lineage.copy_verification_method)
        || approval.exact_confirmation_phrase != expected_phrase
        || approval.approved_at_ms > receipt.copied_at_ms
        || receipt.copied_at_ms.saturating_sub(approval.approved_at_ms)
            > MAX_CLOUD_COPY_APPROVAL_AGE_MS
    {
        return Err("receipt-lineage-copy-approval-mismatch".into());
    }
    Ok(())
}

/// Validate a persisted copy receipt before any provider-specific filesystem or API probe.
///
/// This function is read-only and deliberately excludes provider evidence. It prevents callers
/// from trusting receipt-controlled paths before the receipt's structure and integrity pass.
pub fn receipt_blockers(receipt: &CloudCopyReceipt) -> Vec<String> {
    let mut blockers = Vec::new();
    if !matches!(
        receipt.version,
        LEGACY_RECEIPT_VERSION | PRE_APPROVAL_RECEIPT_VERSION | RECEIPT_VERSION
    ) {
        blockers.push("receipt-version-unsupported".into());
    }
    match receipt.version {
        LEGACY_RECEIPT_VERSION => {
            if receipt.lineage.is_some() || receipt.lineage_fingerprint.is_some() {
                blockers.push("legacy-receipt-lineage-unexpected".into());
            }
        }
        PRE_APPROVAL_RECEIPT_VERSION | RECEIPT_VERSION => {
            match (&receipt.lineage, &receipt.lineage_fingerprint) {
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
                    if let Err(blocker) = validate_receipt_copy_approval(receipt) {
                        blockers.push(blocker);
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
            }
        }
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

#[cfg(all(not(coverage), not(target_os = "macos")))]
fn remove_created_file_if_identity_matches(path: &Path, expected: Option<&Handle>) {
    let can_remove = expected.is_some_and(|identity| {
        let Ok(metadata) = std::fs::symlink_metadata(path) else {
            return false;
        };
        !metadata.file_type().is_symlink()
            && metadata.is_file()
            && Handle::from_path(path).is_ok_and(|current| current.eq(identity))
    });
    if can_remove {
        remove_created_file(path);
    }
}

#[cfg(not(coverage))]
fn failure_id_for(record: &CloudCopyFailureRecord) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-cloud-copy-failure-v1\0");
    for value in [
        record.candidate_fingerprint.as_str(),
        record.provider.as_str(),
        record.source.as_str(),
        record.destination.as_str(),
        record.action.as_str(),
        record.error_code.as_str(),
    ] {
        hasher.update(value.as_bytes());
        hasher.update(&[0]);
    }
    hasher.update(&record.occurred_at_ms.to_le_bytes());
    hasher.finalize().to_hex().to_string()
}

#[cfg(not(coverage))]
fn write_copy_failure_record(
    record: &CloudCopyFailureRecord,
    receipt_dir: &Path,
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(receipt_dir).map_err(|error| error.to_string())?;
    let metadata = std::fs::symlink_metadata(receipt_dir)
        .map_err(|_| "failure-record-directory-unavailable".to_string())?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("failure-record-directory-unsafe".into());
    }
    let mut existing_records = 0_usize;
    for entry in std::fs::read_dir(receipt_dir)
        .map_err(|_| "failure-record-directory-unavailable".to_string())?
    {
        let entry = entry.map_err(|_| "failure-record-directory-unavailable".to_string())?;
        let entry_metadata = std::fs::symlink_metadata(entry.path())
            .map_err(|_| "failure-record-directory-unavailable".to_string())?;
        if entry_metadata.is_file()
            && !entry_metadata.file_type().is_symlink()
            && entry.file_name().to_string_lossy().ends_with("-failure.json")
        {
            existing_records = existing_records.saturating_add(1);
        }
    }
    if existing_records >= MAX_CLOUD_COPY_FAILURE_RECORDS {
        return Err("failure-record-retention-limit".into());
    }
    // A repeated failure in the same millisecond has the same content-derived base id. Retry with
    // a bounded suffix so diagnostic evidence is append-only instead of silently dropped by
    // `create_new`.
    for suffix in 0..=MAX_CLOUD_COPY_FAILURE_RECORDS {
        let mut candidate = record.clone();
        if suffix > 0 {
            candidate.failure_id = format!("{}-{suffix}", record.failure_id);
        }
        let encoded = serde_json::to_vec_pretty(&candidate)
            .map_err(|_| "failure-record-json-invalid".to_string())?;
        if encoded.len() as u64 > MAX_RECEIPT_BYTES {
            return Err("failure-record-too-large".into());
        }
        let path = receipt_dir.join(format!("{}-failure.json", candidate.failure_id));
        #[cfg(unix)]
        let file_result = {
            use std::os::unix::fs::OpenOptionsExt;
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o400)
                .open(&path)
        };
        #[cfg(not(unix))]
        let file_result = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path);
        let mut file = match file_result {
            Ok(file) => file,
            Err(error)
                if error.kind() == std::io::ErrorKind::AlreadyExists
                    && suffix < MAX_CLOUD_COPY_FAILURE_RECORDS =>
            {
                continue;
            }
            Err(_) => return Err("failure-record-create-failed".into()),
        };
        let result = (|| -> Result<(), String> {
            file.write_all(&encoded)
                .and_then(|_| file.sync_all())
                .map_err(|_| "failure-record-write-failed".to_string())?;
            #[cfg(not(unix))]
            {
                let mut permissions = file
                    .metadata()
                    .map_err(|_| "failure-record-permissions-failed".to_string())?
                    .permissions();
                permissions.set_readonly(true);
                std::fs::set_permissions(&path, permissions)
                    .map_err(|_| "failure-record-permissions-failed".to_string())?;
            }
            #[cfg(unix)]
            std::fs::File::open(receipt_dir)
                .and_then(|directory| directory.sync_all())
                .map_err(|_| "failure-record-directory-sync-failed".to_string())?;
            Ok(())
        })();
        if let Err(error) = result {
            drop(file);
            remove_created_file(&path);
            return Err(error);
        }
        return Ok(path);
    }
    Err("failure-record-create-failed".into())
}

#[cfg(not(coverage))]
pub(crate) fn record_copy_failure(
    candidate: &CloudCandidate,
    action: CloudCopyApprovalAction,
    error_code: &str,
    occurred_at_ms: u64,
    receipt_dir: &Path,
) -> Result<(), String> {
    let mut record = CloudCopyFailureRecord {
        version: CLOUD_COPY_FAILURE_VERSION,
        failure_id: String::new(),
        candidate_fingerprint: candidate.metadata_fingerprint.clone(),
        provider: candidate.provider,
        source: candidate.src.clone(),
        destination: candidate.dst.clone(),
        action,
        error_code: error_code.chars().take(160).collect(),
        occurred_at_ms,
    };
    record.failure_id = failure_id_for(&record);
    write_copy_failure_record(&record, receipt_dir).map(|_| ())
}

#[cfg(all(not(coverage), target_os = "macos"))]
const COPY_TIMEOUT_BASE_SECS: u64 = 120;
#[cfg(all(not(coverage), target_os = "macos"))]
const COPY_TIMEOUT_MAX_SECS: u64 = 30 * 60;
#[cfg(all(not(coverage), target_os = "macos"))]
const COPY_EXPECTED_BYTES_PER_SEC: u64 = 4 * 1024 * 1024;

#[cfg(all(not(coverage), target_os = "macos"))]
fn copy_timeout_for_bytes(bytes: u64) -> Duration {
    let transfer_secs =
        bytes.saturating_add(COPY_EXPECTED_BYTES_PER_SEC - 1) / COPY_EXPECTED_BYTES_PER_SEC;
    Duration::from_secs(
        COPY_TIMEOUT_BASE_SECS
            .saturating_add(transfer_secs)
            .min(COPY_TIMEOUT_MAX_SECS),
    )
}

/// Run one fixed macOS filesystem helper outside the UI process so a File Provider
/// materialization/write cannot leave the Tauri command waiting forever.
#[cfg(all(not(coverage), target_os = "macos"))]
fn bounded_macos_command(
    program: &Path,
    args: &[&OsStr],
    timeout: Duration,
    cancel: Option<&AtomicBool>,
) -> Result<(), String> {
    use std::os::unix::process::CommandExt;

    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    unsafe {
        command.pre_exec(|| {
            if libc::setpgid(0, 0) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    if cancel.is_some_and(|flag| flag.load(Ordering::SeqCst)) {
        return Err("cloud-copy-cancelled".into());
    }
    let mut child = command
        .spawn()
        .map_err(|_| "cloud-copy-helper-failed".to_string())?;
    let child_pid = child.id();
    let kill_group = || unsafe {
        let _ = libc::kill(-(child_pid as libc::pid_t), libc::SIGKILL);
    };
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(_)) => return Err("cloud-copy-helper-failed".into()),
            Ok(None) => {
                if cancel.is_some_and(|flag| flag.load(Ordering::SeqCst)) {
                    kill_group();
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("cloud-copy-cancelled".into());
                }
                if Instant::now() < deadline {
                    std::thread::sleep(Duration::from_millis(50));
                } else {
                    kill_group();
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("cloud-copy-timeout".into());
                }
            }
            Err(_) => {
                kill_group();
                let _ = child.kill();
                let _ = child.wait();
                return Err("cloud-copy-helper-failed".into());
            }
        }
    }
}

#[cfg(all(not(coverage), target_os = "macos"))]
fn bounded_macos_mkdir(path: &Path, timeout: Duration, cancel: Option<&AtomicBool>) -> Result<(), String> {
    bounded_macos_command(
        Path::new("/bin/mkdir"),
        &[OsStr::new("-p"), path.as_os_str()],
        timeout,
        cancel,
    )
}

/// Copy outside the UI process; the parent verifies bytes and hashes after the child exits.
#[cfg(all(not(coverage), target_os = "macos"))]
fn bounded_macos_copy(source: &Path, destination: &Path, timeout: Duration, cancel: Option<&AtomicBool>) -> Result<(), String> {
    bounded_macos_command(
        Path::new("/bin/cp"),
        // Never replace a File Provider object that appeared after the read-only preflight.
        &[OsStr::new("-n"), source.as_os_str(), destination.as_os_str()],
        timeout,
        cancel,
    )
}

#[cfg(all(not(coverage), target_os = "macos"))]
fn bounded_macos_move_create_only(
    source: &Path,
    destination: &Path,
    timeout: Duration,
    cancel: Option<&AtomicBool>,
) -> Result<(), String> {
    bounded_macos_command(
        Path::new("/bin/mv"),
        &[
            OsStr::new("-n"),
            source.as_os_str(),
            destination.as_os_str(),
        ],
        timeout,
        cancel,
    )?;
    if std::fs::symlink_metadata(source).is_ok() {
        return Err("cloud-copy-finalize-race".into());
    }
    Ok(())
}

#[cfg(all(not(coverage), target_os = "macos"))]
fn create_macos_copy_staging(parent: &Path) -> Result<(tempfile::TempDir, PathBuf), String> {
    let directory = tempfile::Builder::new()
        .prefix(".disksage-copy-")
        .tempdir_in(parent)
        .map_err(|_| "cloud-copy-staging-create-failed".to_string())?;
    let path = directory.path().join("payload");
    Ok((directory, path))
}

#[cfg(not(coverage))]
fn copy_and_verify(
    candidate: &CloudCandidate,
    cloud_root: &CloudRoot,
    cancel: Option<&AtomicBool>,
) -> Result<(u64, ContentDigests), String> {
    let source = Path::new(&candidate.src);
    let destination = Path::new(&candidate.dst);
    let before = std::fs::symlink_metadata(source).map_err(|error| error.to_string())?;
    if before.file_type().is_symlink() || !before.is_file() {
        return Err("source-must-be-regular-file".into());
    }
    if crate::cloud::metadata_is_dataless(&before) {
        return Err("source-content-not-local".into());
    }
    let before_modified_ms = modified_ms(&before)?;
    if before.len() != candidate.bytes || before_modified_ms != candidate.modified_ms {
        return Err("source-changed-since-plan".into());
    }
    match std::fs::symlink_metadata(destination) {
        Ok(_) => return Err("destination-already-exists".into()),
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            return Err("destination-state-unavailable".into())
        }
        Err(_) => {}
    }
    let parent = destination
        .parent()
        .ok_or_else(|| "destination-parent-missing".to_string())?;
    #[cfg(target_os = "macos")]
    bounded_macos_mkdir(parent, copy_timeout_for_bytes(candidate.bytes), cancel)?;
    #[cfg(not(target_os = "macos"))]
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

    #[cfg(target_os = "macos")]
    let (_staging_directory, staging) = create_macos_copy_staging(parent)?;

    #[cfg(target_os = "macos")]
    let copy_result = (|| -> Result<(u64, ContentDigests), String> {
        bounded_macos_copy(source, &staging, copy_timeout_for_bytes(candidate.bytes), cancel)?;
        let source_hashes = hash_file(source)?;
        let staging_hashes = hash_file(&staging)?;
        let after = std::fs::symlink_metadata(source).map_err(|error| error.to_string())?;
        let unchanged = after.is_file()
            && !after.file_type().is_symlink()
            && after.len() == before.len()
            && modified_ms(&after)? == before_modified_ms;
        let staging_metadata =
            std::fs::symlink_metadata(&staging).map_err(|error| error.to_string())?;
        let staging_len = staging_metadata.len();
        if !unchanged
            || !staging_metadata.is_file()
            || staging_metadata.file_type().is_symlink()
            || staging_len != candidate.bytes
            || source_hashes != staging_hashes
        {
            return Err("copy-verification-failed".into());
        }
        if std::fs::symlink_metadata(destination).is_ok() {
            return Err("destination-created-during-copy".into());
        }
        bounded_macos_move_create_only(
            &staging,
            destination,
            copy_timeout_for_bytes(candidate.bytes),
            cancel,
        )?;
        let finalized = std::fs::symlink_metadata(destination)
            .map_err(|_| "cloud-copy-finalize-failed".to_string())?;
        if !finalized.is_file() || finalized.file_type().is_symlink() {
            return Err("cloud-copy-finalize-failed".into());
        }
        Ok((staging_len, staging_hashes))
    })();

    #[cfg(all(not(coverage), not(target_os = "macos")))]
    let mut destination_identity: Option<Handle> = None;

    #[cfg(not(target_os = "macos"))]
    let copy_result = (|| -> Result<(u64, ContentDigests), String> {
        let mut source_file = std::fs::File::open(source).map_err(|error| error.to_string())?;
        let mut destination_file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)
            .map_err(|error| error.to_string())?;
        #[cfg(all(not(coverage), not(target_os = "macos")))]
        {
            // Capture ownership from the create-new handle itself. A path lookup here could
            // observe a provider/foreign replacement between creation and cleanup.
            destination_identity = Some(
                Handle::from_file(
                    destination_file
                        .try_clone()
                        .map_err(|_| "destination-identity-unavailable".to_string())?,
                )
                .map_err(|_| "destination-identity-unavailable".to_string())?,
            );
        }
        let mut source_hasher = ContentHasher::default();
        let mut copied = 0_u64;
        let mut buffer = vec![0_u8; 1024 * 1024];
        loop {
            if cancel.is_some_and(|flag| flag.load(Ordering::SeqCst)) {
                return Err("cloud-copy-cancelled".into());
            }
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
        if cancel.is_some_and(|flag| flag.load(Ordering::SeqCst)) {
            return Err("cloud-copy-cancelled".into());
        }
        destination_file
            .sync_all()
            .map_err(|error| error.to_string())?;
        drop(destination_file);

        if cancel.is_some_and(|flag| flag.load(Ordering::SeqCst)) {
            return Err("cloud-copy-cancelled".into());
        }

        let streamed_hashes = source_hasher.finalize();
        let source_hashes = hash_file(source)?;
        let destination_hashes = hash_file(destination)?;
        if cancel.is_some_and(|flag| flag.load(Ordering::SeqCst)) {
            return Err("cloud-copy-cancelled".into());
        }
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

    // The TempDir owns the only pathname created for a macOS copy. On Unix, a native copy's
    // cleanup is identity-bound so a concurrent replacement cannot be deleted accidentally.
    #[cfg(all(not(coverage), not(target_os = "macos")))]
    if copy_result.is_err() {
        remove_created_file_if_identity_matches(destination, destination_identity.as_ref());
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
    if crate::cloud::metadata_is_dataless(&source_before) {
        return Err("source-content-not-local".into());
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

    // Never hash a dataless File Provider placeholder: opening it can materialize bytes and consume
    // local headroom. Provider-native status must succeed before any content read.
    crate::provider_sync::require_existing_destination_local_current(
        candidate.provider,
        destination,
        candidate.bytes,
    )?;
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
pub fn write_provider_api_receipt(
    receipt: &CloudCopyReceipt,
    receipt_dir: &Path,
) -> Result<PathBuf, String> {
    write_immutable_receipt(receipt, receipt_dir)
}

#[cfg(not(coverage))]
pub(crate) fn build_verified_receipt(
    candidate: &CloudCandidate,
    review_decision: Option<&CloudReviewDecision>,
    copy_approval: &CloudCopyApproval,
    hashes: ContentDigests,
    verified_at_ms: u64,
    copy_verification_method: CloudCopyVerificationMethod,
) -> Result<CloudCopyReceipt, String> {
    let lineage = lineage_snapshot(
        candidate,
        review_decision,
        copy_verification_method,
        Some(copy_approval),
    );
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

/// Hash and bind a source before an authenticated provider upload. This deliberately does not
/// touch the destination: a disconnected File Provider may not expose a usable local directory.
#[cfg(not(coverage))]
pub fn prepare_provider_api_source_receipt(
    candidate: &CloudCandidate,
    cloud_root: &CloudRoot,
    review_decision: Option<&CloudReviewDecision>,
    copy_approval: &CloudCopyApproval,
    copied_at_ms: u64,
) -> Result<(CloudCopyReceipt, ContentDigests), String> {
    validate_cloud_copy_approval_for_action(
        copy_approval,
        candidate,
        cloud_root,
        CloudCopyApprovalAction::CopyOnly,
        copied_at_ms,
    )?;
    let blockers = candidate_blockers_with_review(candidate, cloud_root, review_decision);
    if !blockers.is_empty() {
        return Err(blockers.join(","));
    }
    let source = Path::new(&candidate.src);
    let before = std::fs::symlink_metadata(source).map_err(|error| error.to_string())?;
    if before.file_type().is_symlink() || !before.is_file() {
        return Err("source-must-be-regular-file".into());
    }
    if crate::cloud::metadata_is_dataless(&before) {
        return Err("source-content-not-local".into());
    }
    let before_modified_ms = modified_ms(&before)?;
    if before.len() != candidate.bytes || before_modified_ms != candidate.modified_ms {
        return Err("source-changed-since-plan".into());
    }
    let hashes = hash_file(source)?;
    let after = std::fs::symlink_metadata(source).map_err(|error| error.to_string())?;
    if after.file_type().is_symlink()
        || !after.is_file()
        || after.len() != before.len()
        || modified_ms(&after)? != before_modified_ms
    {
        return Err("source-changed-during-provider-upload-preflight".into());
    }
    let receipt = build_verified_receipt(
        candidate,
        review_decision,
        copy_approval,
        hashes.clone(),
        copied_at_ms,
        CloudCopyVerificationMethod::CopiedByProviderApi,
    )?;
    Ok((receipt, hashes))
}

#[cfg(not(coverage))]
pub fn verify_provider_api_source_unchanged(
    candidate: &CloudCandidate,
    hashes: &ContentDigests,
) -> Result<(), String> {
    let source = Path::new(&candidate.src);
    let metadata = std::fs::symlink_metadata(source).map_err(|_| "source-unavailable".to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("source-changed-during-provider-upload".into());
    }
    if metadata.len() != candidate.bytes || modified_ms(&metadata)? != candidate.modified_ms {
        return Err("source-changed-during-provider-upload".into());
    }
    if hash_file(source)? != *hashes {
        return Err("source-changed-during-provider-upload".into());
    }
    Ok(())
}

/// Copy a candidate only after validating both the optional metadata review decision and a fresh,
/// exact, human-attributed copy approval. The production entrypoint reads the live clock at the
/// mutation boundary so an earlier preflight cannot silently extend the approval lifetime.
#[cfg(not(coverage))]
pub fn prepare_cloud_copy_with_approval(
    candidate: &CloudCandidate,
    cloud_root: &CloudRoot,
    receipt_dir: &Path,
    review_decision: Option<&CloudReviewDecision>,
    copy_approval: &CloudCopyApproval,
) -> Result<(CloudCopyReceipt, PathBuf), String> {
    prepare_cloud_copy_with_approval_at(
        candidate,
        cloud_root,
        receipt_dir,
        crate::cloud::system_now_ms(),
        review_decision,
        copy_approval,
    )
}

/// Production command variant that can stop the bounded helper or chunked copy at a safe
/// boundary. The token is process-local and never persisted as authority.
#[cfg(not(coverage))]
pub fn prepare_cloud_copy_with_approval_cancelable(
    candidate: &CloudCandidate,
    cloud_root: &CloudRoot,
    receipt_dir: &Path,
    review_decision: Option<&CloudReviewDecision>,
    copy_approval: &CloudCopyApproval,
    cancel: &AtomicBool,
) -> Result<(CloudCopyReceipt, PathBuf), String> {
    prepare_cloud_copy_with_approval_at_cancel(
        candidate,
        cloud_root,
        receipt_dir,
        crate::cloud::system_now_ms(),
        review_decision,
        copy_approval,
        Some(cancel),
    )
}

#[cfg(not(coverage))]
fn prepare_cloud_copy_with_approval_at(
    candidate: &CloudCandidate,
    cloud_root: &CloudRoot,
    receipt_dir: &Path,
    copied_at_ms: u64,
    review_decision: Option<&CloudReviewDecision>,
    copy_approval: &CloudCopyApproval,
) -> Result<(CloudCopyReceipt, PathBuf), String> {
    prepare_cloud_copy_with_approval_at_cancel(
        candidate,
        cloud_root,
        receipt_dir,
        copied_at_ms,
        review_decision,
        copy_approval,
        None,
    )
}

#[cfg(not(coverage))]
fn prepare_cloud_copy_with_approval_at_cancel(
    candidate: &CloudCandidate,
    cloud_root: &CloudRoot,
    receipt_dir: &Path,
    copied_at_ms: u64,
    review_decision: Option<&CloudReviewDecision>,
    copy_approval: &CloudCopyApproval,
    cancel: Option<&AtomicBool>,
) -> Result<(CloudCopyReceipt, PathBuf), String> {
    validate_cloud_copy_approval_for_action(
        copy_approval,
        candidate,
        cloud_root,
        CloudCopyApprovalAction::CopyOnly,
        copied_at_ms,
    )?;
    let blockers = candidate_blockers_with_review(candidate, cloud_root, review_decision);
    if !blockers.is_empty() {
        return Err(blockers.join(","));
    }
    let (_, hashes) = copy_and_verify(candidate, cloud_root, cancel)?;
    let receipt = build_verified_receipt(
        candidate,
        review_decision,
        copy_approval,
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

/// Verify and adopt an existing destination only after the same exact human action approval.
/// The approval age is evaluated from a fresh live-clock read immediately before verification.
#[cfg(not(coverage))]
pub fn adopt_existing_cloud_copy_with_approval(
    candidate: &CloudCandidate,
    cloud_root: &CloudRoot,
    receipt_dir: &Path,
    review_decision: Option<&CloudReviewDecision>,
    copy_approval: &CloudCopyApproval,
) -> Result<(CloudCopyReceipt, PathBuf), String> {
    adopt_existing_cloud_copy_with_approval_at(
        candidate,
        cloud_root,
        receipt_dir,
        crate::cloud::system_now_ms(),
        review_decision,
        copy_approval,
    )
}

#[cfg(not(coverage))]
fn adopt_existing_cloud_copy_with_approval_at(
    candidate: &CloudCandidate,
    cloud_root: &CloudRoot,
    receipt_dir: &Path,
    verified_at_ms: u64,
    review_decision: Option<&CloudReviewDecision>,
    copy_approval: &CloudCopyApproval,
) -> Result<(CloudCopyReceipt, PathBuf), String> {
    validate_cloud_copy_approval_for_action(
        copy_approval,
        candidate,
        cloud_root,
        CloudCopyApprovalAction::AdoptExistingCopy,
        verified_at_ms,
    )?;
    let blockers =
        existing_copy_candidate_blockers_with_review(candidate, cloud_root, review_decision);
    if !blockers.is_empty() {
        return Err(blockers.join(","));
    }
    let hashes = verify_existing_destination(candidate, cloud_root)?;
    let receipt = build_verified_receipt(
        candidate,
        review_decision,
        copy_approval,
        hashes,
        verified_at_ms,
        CloudCopyVerificationMethod::AdoptedExisting,
    )?;
    let path = write_immutable_receipt(&receipt, receipt_dir)?;
    Ok((receipt, path))
}

#[cfg(test)]
fn test_copy_approval(
    candidate: &CloudCandidate,
    cloud_root: &CloudRoot,
    action: CloudCopyApprovalAction,
    approved_at_ms: u64,
) -> Result<CloudCopyApproval, String> {
    create_cloud_copy_approval(
        candidate,
        cloud_root,
        action,
        approved_at_ms,
        "human:test",
        "test-authorized exact candidate action",
        &cloud_copy_approval_phrase(candidate, action),
    )
}

/// Test-only compatibility helper that creates a valid exact approval before preparing a copy.
#[cfg(all(test, not(coverage)))]
pub fn prepare_cloud_copy(
    candidate: &CloudCandidate,
    cloud_root: &CloudRoot,
    receipt_dir: &Path,
    copied_at_ms: u64,
) -> Result<(CloudCopyReceipt, PathBuf), String> {
    prepare_cloud_copy_with_review(candidate, cloud_root, receipt_dir, copied_at_ms, None)
}

/// Test-only compatibility helper that combines metadata review and exact copy approval fixtures.
#[cfg(all(test, not(coverage)))]
pub fn prepare_cloud_copy_with_review(
    candidate: &CloudCandidate,
    cloud_root: &CloudRoot,
    receipt_dir: &Path,
    copied_at_ms: u64,
    review_decision: Option<&CloudReviewDecision>,
) -> Result<(CloudCopyReceipt, PathBuf), String> {
    let approval = test_copy_approval(
        candidate,
        cloud_root,
        CloudCopyApprovalAction::CopyOnly,
        copied_at_ms,
    )?;
    prepare_cloud_copy_with_approval_at(
        candidate,
        cloud_root,
        receipt_dir,
        copied_at_ms,
        review_decision,
        &approval,
    )
}

/// Test-only compatibility helper that approves and verifies adoption of an existing copy.
#[cfg(all(test, not(coverage)))]
pub fn adopt_existing_cloud_copy(
    candidate: &CloudCandidate,
    cloud_root: &CloudRoot,
    receipt_dir: &Path,
    verified_at_ms: u64,
) -> Result<(CloudCopyReceipt, PathBuf), String> {
    let approval = test_copy_approval(
        candidate,
        cloud_root,
        CloudCopyApprovalAction::AdoptExistingCopy,
        verified_at_ms,
    )?;
    adopt_existing_cloud_copy_with_approval_at(
        candidate,
        cloud_root,
        receipt_dir,
        verified_at_ms,
        None,
        &approval,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::{ArchiveKind, MetadataEvidence};
    use crate::provider_evidence::{
        create_sync_evidence_record, ProviderSyncEvidenceRecord, PROVIDER_EVIDENCE_RECORD_VERSION,
    };
    use crate::provider_capacity::{
        self, CapacityEvidenceKind, CloudCapacitySnapshot, CloudCapacityState,
        CAPACITY_SCHEMA_VERSION,
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
            account_scope: CloudAccountScope::Personal,
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
            destination_account_scope: CloudAccountScope::Personal,
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

    #[cfg(not(coverage))]
    #[test]
    fn cancelled_copy_stops_before_writing_destination() {
        let temporary = tempfile::tempdir().unwrap();
        let source = temporary.path().join("source.bin");
        std::fs::write(&source, b"copy-me").unwrap();
        let cloud_path = temporary.path().join("cloud");
        std::fs::create_dir(&cloud_path).unwrap();
        let destination = cloud_path.join("archive.bin");
        let metadata = std::fs::symlink_metadata(&source).unwrap();
        let mut planned = candidate();
        planned.src = source.to_string_lossy().into_owned();
        planned.dst = destination.to_string_lossy().into_owned();
        planned.bytes = metadata.len();
        planned.modified_ms = modified_ms(&metadata).unwrap();
        let mut selected_root = root();
        selected_root.path = cloud_path.to_string_lossy().into_owned();
        let cancel = AtomicBool::new(true);

        let error = copy_and_verify(&planned, &selected_root, Some(&cancel)).unwrap_err();
        assert_eq!(error, "cloud-copy-cancelled");
        assert!(!destination.exists());
    }

    #[cfg(not(coverage))]
    #[test]
    fn copy_failure_record_is_private_durable_and_round_trips() {
        let temporary = tempfile::tempdir().unwrap();
        let receipt_dir = temporary.path().join("receipts");
        record_copy_failure(
            &candidate(),
            CloudCopyApprovalAction::CopyOnly,
            "cloud-copy-timeout",
            123,
            &receipt_dir,
        )
        .unwrap();
        let entries: Vec<_> = std::fs::read_dir(&receipt_dir).unwrap().collect();
        assert_eq!(entries.len(), 1);
        let path = entries[0].as_ref().unwrap().path();
        assert!(path.file_name().unwrap().to_string_lossy().ends_with("-failure.json"));
        let decoded: CloudCopyFailureRecord =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(decoded.error_code, "cloud-copy-timeout");
        assert_eq!(decoded.occurred_at_ms, 123);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(path).unwrap().permissions().mode() & 0o777, 0o400);
        }
    }

    #[cfg(not(coverage))]
    #[test]
    fn copy_failure_record_surfaces_unwritable_journal() {
        let temporary = tempfile::tempdir().unwrap();
        let receipt_path = temporary.path().join("not-a-directory");
        std::fs::write(&receipt_path, b"occupied").unwrap();

        let error = record_copy_failure(
            &candidate(),
            CloudCopyApprovalAction::CopyOnly,
            "cloud-copy-timeout",
            123,
            &receipt_path,
        )
        .unwrap_err();
        assert!(!error.is_empty());
    }

    #[cfg(not(coverage))]
    #[test]
    fn same_millisecond_copy_failures_get_distinct_records() {
        let temporary = tempfile::tempdir().unwrap();
        let receipt_dir = temporary.path().join("receipts");
        for _ in 0..2 {
            record_copy_failure(
                &candidate(),
                CloudCopyApprovalAction::CopyOnly,
                "cloud-copy-timeout",
                123,
                &receipt_dir,
            )
            .unwrap();
        }
        let mut entries: Vec<_> = std::fs::read_dir(&receipt_dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        entries.sort();
        assert_eq!(entries.len(), 2);
        let records: Vec<CloudCopyFailureRecord> = entries
            .iter()
            .map(|path| serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap())
            .collect();
        assert_ne!(records[0].failure_id, records[1].failure_id);
        assert!(records
            .iter()
            .all(|record| record.occurred_at_ms == 123));
    }

    fn refresh_review_fingerprint(candidate: &mut CloudCandidate) {
        candidate.review_fingerprint = candidate_review_fingerprint(candidate);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_copy_timeout_scales_but_has_a_hard_ceiling() {
        assert_eq!(copy_timeout_for_bytes(0), Duration::from_secs(120));
        assert!(copy_timeout_for_bytes(4 * 1024 * 1024) > copy_timeout_for_bytes(0));
        assert_eq!(
            copy_timeout_for_bytes(u64::MAX),
            Duration::from_secs(COPY_TIMEOUT_MAX_SECS)
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_copy_staging_is_owned_and_removed_with_the_temp_directory() {
        let parent = tempfile::tempdir().unwrap();
        let (directory, staging) = create_macos_copy_staging(parent.path()).unwrap();
        assert!(staging.starts_with(directory.path()));
        std::fs::write(&staging, b"partial").unwrap();
        drop(directory);
        assert!(!staging.exists());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_move_create_only_preserves_staging_on_destination_race() {
        let parent = tempfile::tempdir().unwrap();
        let staging = parent.path().join("staging-payload");
        let destination = parent.path().join("provider-payload");
        std::fs::write(&staging, b"staging").unwrap();
        std::fs::write(&destination, b"provider").unwrap();

        let result = bounded_macos_move_create_only(
            &staging,
            &destination,
            Duration::from_secs(5),
            None,
        );

        assert_eq!(result, Err("cloud-copy-finalize-race".into()));
        assert_eq!(std::fs::read(&staging).unwrap(), b"staging");
        assert_eq!(std::fs::read(&destination).unwrap(), b"provider");
    }

    fn receipt() -> CloudCopyReceipt {
        let candidate = candidate();
        let approval =
            test_copy_approval(&candidate, &root(), CloudCopyApprovalAction::CopyOnly, 100)
                .unwrap();
        let lineage = lineage_snapshot(
            &candidate,
            None,
            CloudCopyVerificationMethod::CopiedByDiskSage,
            Some(&approval),
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
        let lineage = provider_receipt.lineage.as_mut().unwrap();
        let approval = lineage.copy_approval.as_mut().unwrap();
        approval.provider = provider;
        approval.approval_id = copy_approval_id_for(approval);
        provider_receipt.lineage_fingerprint = Some(lineage_fingerprint(lineage).unwrap());
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

    fn pre_approval_receipt() -> CloudCopyReceipt {
        let mut previous = receipt();
        previous.version = PRE_APPROVAL_RECEIPT_VERSION;
        previous.lineage.as_mut().unwrap().copy_approval = None;
        previous.lineage_fingerprint =
            Some(lineage_fingerprint(previous.lineage.as_ref().unwrap()).unwrap());
        previous.receipt_id = receipt_id_for(
            previous.version,
            &previous.candidate_fingerprint,
            previous.provider,
            &previous.source,
            &previous.destination,
            previous.bytes,
            &previous.blake3,
            &previous.sha256,
            &previous.quick_xor_base64,
            previous.source_modified_ms,
            previous.copied_at_ms,
            previous.copy_verified,
            previous.provider_sync_confirmed,
            previous.lineage_fingerprint.as_deref(),
        );
        previous
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
            sync_state: ProviderSyncState::Complete,
            remote_content: None,
        }
    }

    #[test]
    fn unknown_legacy_sync_state_cannot_promote_goal_to_eviction_ready() {
        let evidence = evidence();
        assert_eq!(evidence.sync_state, ProviderSyncState::Complete);
        let mut legacy = evidence;
        legacy.sync_state = ProviderSyncState::Unknown;
        assert_eq!(
            CloudOffloadGoalState::after_attestation(&legacy, true),
            CloudOffloadGoalState::PendingProviderSync
        );
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
        changed_scope.account_scope = CloudAccountScope::Organization;
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
    #[cfg(not(coverage))]
    fn production_copy_entrypoints_recheck_approval_age_against_live_time() {
        let candidate = candidate();
        let root = root();
        let copy_approval =
            test_copy_approval(&candidate, &root, CloudCopyApprovalAction::CopyOnly, 1).unwrap();
        assert_eq!(
            prepare_cloud_copy_with_approval(
                &candidate,
                &root,
                std::path::Path::new("/unused"),
                None,
                &copy_approval,
            )
            .unwrap_err(),
            "cloud-copy-approval-stale"
        );

        let adoption_approval = test_copy_approval(
            &candidate,
            &root,
            CloudCopyApprovalAction::AdoptExistingCopy,
            1,
        )
        .unwrap();
        assert_eq!(
            adopt_existing_cloud_copy_with_approval(
                &candidate,
                &root,
                std::path::Path::new("/unused"),
                None,
                &adoption_approval,
            )
            .unwrap_err(),
            "cloud-copy-approval-stale"
        );
    }

    #[test]
    fn copy_approval_requires_exact_phrase_human_attribution_context_and_freshness() {
        let candidate = candidate();
        let root = root();
        let action = CloudCopyApprovalAction::CopyOnly;
        assert_eq!(
            create_cloud_copy_approval(
                &candidate,
                &root,
                action,
                100,
                "human:test",
                "Exact source and cloud destination reviewed.",
                "승인",
            )
            .unwrap_err(),
            "cloud-copy-exact-confirmation-phrase-mismatch"
        );
        let phrase = cloud_copy_approval_phrase(&candidate, action);
        let approval = create_cloud_copy_approval(
            &candidate,
            &root,
            action,
            100,
            "human:test",
            "Exact source and cloud destination reviewed.",
            &phrase,
        )
        .unwrap();
        assert!(validate_cloud_copy_approval_for_action(
            &approval,
            &candidate,
            &root,
            action,
            100 + MAX_CLOUD_COPY_APPROVAL_AGE_MS,
        )
        .is_ok());
        assert_eq!(
            validate_cloud_copy_approval_for_action(
                &approval,
                &candidate,
                &root,
                action,
                101 + MAX_CLOUD_COPY_APPROVAL_AGE_MS,
            )
            .unwrap_err(),
            "cloud-copy-approval-stale"
        );
        let mut wrong_root = root.clone();
        wrong_root.id = "icloud:other".into();
        assert_eq!(
            validate_cloud_copy_approval_for_action(
                &approval,
                &candidate,
                &wrong_root,
                action,
                100,
            )
            .unwrap_err(),
            "cloud-copy-approval-context-mismatch"
        );
        assert_eq!(
            validate_cloud_copy_approval_for_action(
                &approval,
                &candidate,
                &root,
                CloudCopyApprovalAction::AdoptExistingCopy,
                100,
            )
            .unwrap_err(),
            "cloud-copy-approval-context-mismatch"
        );
    }

    #[test]
    fn receipt_lineage_is_integrity_bound_and_older_receipts_remain_valid() {
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

        let mut approval_missing = current.clone();
        approval_missing.lineage.as_mut().unwrap().copy_approval = None;
        approval_missing.lineage_fingerprint =
            Some(lineage_fingerprint(approval_missing.lineage.as_ref().unwrap()).unwrap());
        approval_missing.receipt_id = receipt_id_for(
            approval_missing.version,
            &approval_missing.candidate_fingerprint,
            approval_missing.provider,
            &approval_missing.source,
            &approval_missing.destination,
            approval_missing.bytes,
            &approval_missing.blake3,
            &approval_missing.sha256,
            &approval_missing.quick_xor_base64,
            approval_missing.source_modified_ms,
            approval_missing.copied_at_ms,
            approval_missing.copy_verified,
            approval_missing.provider_sync_confirmed,
            approval_missing.lineage_fingerprint.as_deref(),
        );
        assert!(receipt_blockers(&approval_missing)
            .contains(&"receipt-lineage-copy-approval-missing".to_string()));

        let mut missing = current;
        missing.lineage = None;
        assert!(receipt_blockers(&missing).contains(&"receipt-lineage-missing".to_string()));

        let previous = pre_approval_receipt();
        assert!(receipt_blockers(&previous).is_empty());

        let legacy = legacy_receipt();
        assert!(receipt_blockers(&legacy).is_empty());
        let encoded = serde_json::to_vec(&legacy).unwrap();
        let decoded: CloudCopyReceipt = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(decoded, legacy);
        assert!(!String::from_utf8(encoded).unwrap().contains("lineage"));
    }

    #[test]
    fn historical_lineage_extensions_remain_integrity_bound() {
        let mut historical = pre_approval_receipt();
        let lineage = historical.lineage.as_mut().unwrap();
        lineage.ontology_class = Some("https://disksage.app/ontology#Document".into());
        lineage.ontology_relations = Some(vec![LegacyOntologyRelation {
            subject: SOURCE.into(),
            predicate: "https://disksage.app/ontology#archivedTo".into(),
            object: DESTINATION.into(),
            source: "archive-destination-planner".into(),
        }]);
        lineage.capacity = Some(provider_capacity::assess_capacity(
            CloudCapacitySnapshot {
                schema_version: CAPACITY_SCHEMA_VERSION,
                provider: CloudProvider::Icloud,
                account_scope: None,
                evidence_kind: CapacityEvidenceKind::ProviderNativeStatus,
                observed_at_ms: 4,
                total_bytes: None,
                used_bytes: None,
                remaining_bytes: Some(1024),
                trashed_bytes: None,
                max_upload_size_bytes: None,
                state: CloudCapacityState::Available,
                evidence_fingerprint: Some("f".repeat(64)),
                unavailable_reason: None,
            },
            12,
            12,
            0,
        ));
        historical.lineage_fingerprint = Some(lineage_fingerprint(lineage).unwrap());
        historical.receipt_id = receipt_id_for(
            historical.version,
            &historical.candidate_fingerprint,
            historical.provider,
            &historical.source,
            &historical.destination,
            historical.bytes,
            &historical.blake3,
            &historical.sha256,
            &historical.quick_xor_base64,
            historical.source_modified_ms,
            historical.copied_at_ms,
            historical.copy_verified,
            historical.provider_sync_confirmed,
            historical.lineage_fingerprint.as_deref(),
        );
        assert!(receipt_blockers(&historical).is_empty());

        let mut tampered = historical;
        tampered.lineage.as_mut().unwrap().ontology_class = Some("tampered".into());
        assert!(receipt_blockers(&tampered)
            .contains(&"receipt-lineage-integrity-mismatch".to_string()));
    }

    #[test]
    fn operator_decision_clears_only_the_matching_review_gate() {
        let mut reviewed = candidate();
        reviewed.metadata_fingerprint = "a".repeat(64);
        reviewed.requires_review = true;
        reviewed.review_reasons = vec!["embedded-metadata-probe-incomplete".into()];
        reviewed.review_fingerprint = crate::cloud::candidate_review_fingerprint(&reviewed);
        let legacy_approved =
            crate::cloud_review::create_decision(&reviewed, CloudReviewDisposition::Approved, 10)
                .unwrap();
        assert!(
            candidate_blockers_with_review(&reviewed, &root(), Some(&legacy_approved))
                .contains(&"review-decision-attribution-required".to_string())
        );

        let approved = crate::cloud_review::create_attributed_decision(
            &reviewed,
            CloudReviewDisposition::Approved,
            11,
            "human:local:reviewer",
            "[organization-tenant-authority-confirmed] Metadata title, account scope, and destination reviewed.",
        )
        .unwrap();
        assert!(candidate_blockers_with_review(&reviewed, &root(), Some(&approved)).is_empty());
        let reviewed_lineage = lineage_snapshot(
            &reviewed,
            Some(&approved),
            CloudCopyVerificationMethod::CopiedByDiskSage,
            None,
        );
        assert_eq!(
            reviewed_lineage.review_decision_id.as_deref(),
            Some(approved.decision_id.as_str())
        );
        assert_eq!(
            reviewed_lineage.review_disposition,
            Some(CloudReviewDisposition::Approved)
        );
        assert_eq!(reviewed_lineage.reviewed_at_ms, Some(11));
        assert_eq!(
            reviewed_lineage.review_fingerprint,
            reviewed.review_fingerprint
        );
        assert_eq!(
            reviewed_lineage.reviewed_by.as_deref(),
            Some("human:local:reviewer")
        );
        assert_eq!(
            reviewed_lineage.review_rationale.as_deref(),
            Some("[organization-tenant-authority-confirmed] Metadata title, account scope, and destination reviewed.")
        );

        let mut organization_sensitive = reviewed.clone();
        organization_sensitive
            .review_reasons
            .push(ORGANIZATION_TENANT_AUTHORITY_REVIEW_REASON.into());
        refresh_review_fingerprint(&mut organization_sensitive);
        let unconfirmed_tenant = crate::cloud_review::create_attributed_decision(
            &organization_sensitive,
            CloudReviewDisposition::Approved,
            12,
            "human:local:reviewer",
            "Metadata and destination reviewed.",
        )
        .unwrap();
        assert!(candidate_blockers_with_review(
            &organization_sensitive,
            &root(),
            Some(&unconfirmed_tenant)
        )
        .contains(&"organization-tenant-authority-attestation-required".to_string()));
        let confirmed_tenant = crate::cloud_review::create_attributed_decision(
            &organization_sensitive,
            CloudReviewDisposition::Approved,
            13,
            "human:local:reviewer",
            "[organization-tenant-authority-confirmed] Authorized tenant and destination reviewed.",
        )
        .unwrap();
        assert!(candidate_blockers_with_review(
            &organization_sensitive,
            &root(),
            Some(&confirmed_tenant)
        )
        .is_empty());

        let original_fingerprint = lineage_fingerprint(&reviewed_lineage).unwrap();
        let mut changed_attribution = reviewed_lineage;
        changed_attribution.review_rationale = Some("Changed rationale".into());
        assert_ne!(
            original_fingerprint,
            lineage_fingerprint(&changed_attribution).unwrap()
        );

        let held = crate::cloud_review::create_attributed_decision(
            &reviewed,
            CloudReviewDisposition::Held,
            12,
            "human:local:reviewer",
            "Hold until the destination account scope is confirmed.",
        )
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
        let filename_approval = crate::cloud_review::create_attributed_decision(
            &reviewed,
            CloudReviewDisposition::Approved,
            13,
            "human:local:reviewer",
            "[organization-tenant-authority-confirmed] Filename date is auxiliary; destination and surrounding context were reviewed.",
        )
        .unwrap();
        assert!(
            candidate_blockers_with_review(&reviewed, &root(), Some(&filename_approval)).is_empty()
        );

        assert!(candidate_blockers_with_review(&reviewed, &root(), None)
            .contains(&"embedded-high-confidence-date-required".to_string()));

        let filename_hold = crate::cloud_review::create_attributed_decision(
            &reviewed,
            CloudReviewDisposition::Held,
            14,
            "human:local:reviewer",
            "Hold because embedded production metadata is unavailable.",
        )
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
                sync_state: ProviderSyncState::Complete,
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
            sync_state: ProviderSyncState::Complete,
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
            account_scope: CloudAccountScope::Personal,
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
        let approval = lineage.copy_approval.as_ref().unwrap();
        assert_eq!(approval.action, CloudCopyApprovalAction::CopyOnly);
        assert_eq!(
            approval.review_fingerprint,
            test_candidate.review_fingerprint
        );
        assert_eq!(approval.approved_by, "human:test");

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
            account_scope: CloudAccountScope::Personal,
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
            account_scope: CloudAccountScope::Personal,
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
            account_scope: CloudAccountScope::Personal,
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
            account_scope: CloudAccountScope::Personal,
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
            account_scope: CloudAccountScope::Personal,
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
            account_scope: CloudAccountScope::Personal,
            label: "iCloud Drive".into(),
            path: cloud.to_string_lossy().into_owned(),
            readable: true,
            access_issue: None,
        };
        let content_hash = hash_file(&source).unwrap();
        let approval = test_copy_approval(
            &test_candidate,
            &test_root,
            CloudCopyApprovalAction::CopyOnly,
            123,
        )
        .unwrap();
        let lineage = lineage_snapshot(
            &test_candidate,
            None,
            CloudCopyVerificationMethod::CopiedByDiskSage,
            Some(&approval),
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
