//! Fail-closed cloud transfer safety gates.
//!
//! A verified copy is deliberately not a move. The source remains untouched until a later
//! provider-native synchronization attestation matches the immutable copy receipt. This module
//! produces an eviction permit but intentionally exposes no source deletion API.

use crate::cloud::{CloudCandidate, CloudProvider, CloudRoot};
use std::path::Path;

#[cfg(not(coverage))]
use crate::content_digest::{ContentDigests, ContentHasher};
#[cfg(not(coverage))]
use std::io::{Read, Write};
#[cfg(not(coverage))]
use std::path::PathBuf;

pub const RECEIPT_VERSION: u32 = 2;
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

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RemoteContentProof {
    pub object_id: String,
    pub revision: String,
    pub algorithm: RemoteChecksumAlgorithm,
    pub checksum: String,
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
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
pub fn candidate_blockers(candidate: &CloudCandidate, cloud_root: &CloudRoot) -> Vec<String> {
    let source = Path::new(&candidate.src);
    let destination = Path::new(&candidate.dst);
    let root = Path::new(&cloud_root.path);
    let mut blockers = Vec::new();

    if candidate.requires_review {
        blockers.push("review-required".into());
    }
    if candidate.blocked_reason.is_some() {
        blockers.push("planner-blocked".into());
    }
    if !embedded_high_confidence(candidate) {
        blockers.push("embedded-high-confidence-date-required".into());
    }
    if candidate.metadata_fingerprint.trim().is_empty() {
        blockers.push("metadata-fingerprint-missing".into());
    }
    if candidate.provider != cloud_root.provider {
        blockers.push("provider-mismatch".into());
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

fn receipt_id_for(
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
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&RECEIPT_VERSION.to_le_bytes());
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
    hasher.finalize().to_hex().to_string()
}

fn receipt_integrity_valid(receipt: &CloudCopyReceipt) -> bool {
    receipt.receipt_id
        == receipt_id_for(
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
        )
}

/// Validate a persisted copy receipt before any provider-specific filesystem or API probe.
///
/// This function is read-only and deliberately excludes provider evidence. It prevents callers
/// from trusting receipt-controlled paths before the receipt's structure and integrity pass.
pub fn receipt_blockers(receipt: &CloudCopyReceipt) -> Vec<String> {
    let mut blockers = Vec::new();
    if receipt.version != RECEIPT_VERSION {
        blockers.push("receipt-version-unsupported".into());
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
fn same_receipt_file_identity(
    expected: &std::fs::Metadata,
    observed: &std::fs::Metadata,
) -> bool {
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
    evidence: &ProviderSyncEvidence,
) -> Result<LocalEvictionPermit, Vec<String>> {
    let mut blockers = receipt_blockers(receipt);
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
        (SyncEvidenceKind::ProviderNativeStatus, CloudProvider::Icloud, None) => {}
        (SyncEvidenceKind::ProviderNativeStatus, _, _) => {
            blockers.push("native-status-provider-unsupported".into());
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
fn write_immutable_receipt(
    receipt: &CloudCopyReceipt,
    receipt_dir: &Path,
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(receipt_dir).map_err(|error| error.to_string())?;
    let path = receipt_dir.join(format!("{}.json", receipt.receipt_id));
    let encoded = serde_json::to_vec_pretty(receipt).map_err(|error| error.to_string())?;
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

/// Copy a pre-approved candidate into its cloud root and persist an immutable verification
/// receipt. The source is never removed, even when receipt persistence fails.
#[cfg(not(coverage))]
pub fn prepare_cloud_copy(
    candidate: &CloudCandidate,
    cloud_root: &CloudRoot,
    receipt_dir: &Path,
    copied_at_ms: u64,
) -> Result<(CloudCopyReceipt, PathBuf), String> {
    let blockers = candidate_blockers(candidate, cloud_root);
    if !blockers.is_empty() {
        return Err(blockers.join(","));
    }
    let (_, hashes) = copy_and_verify(candidate, cloud_root)?;
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
        copied_at_ms,
        copy_verified: true,
        provider_sync_confirmed: false,
    };
    receipt.receipt_id = receipt_id_for(
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
    );
    match write_immutable_receipt(&receipt, receipt_dir) {
        Ok(path) => Ok((receipt, path)),
        Err(error) => {
            remove_created_file(Path::new(&candidate.dst));
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::{ArchiveKind, MetadataEvidence};

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

    fn root() -> CloudRoot {
        CloudRoot {
            id: "icloud:test".into(),
            provider: CloudProvider::Icloud,
            label: "iCloud Drive".into(),
            path: ROOT.into(),
        }
    }

    fn candidate() -> CloudCandidate {
        CloudCandidate {
            metadata_fingerprint: "metadata-fingerprint".into(),
            src: SOURCE.into(),
            dst: DESTINATION.into(),
            provider: CloudProvider::Icloud,
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
        }
    }

    fn receipt() -> CloudCopyReceipt {
        let mut receipt = CloudCopyReceipt {
            version: RECEIPT_VERSION,
            receipt_id: String::new(),
            candidate_fingerprint: "metadata-fingerprint".into(),
            provider: CloudProvider::Icloud,
            source: SOURCE.into(),
            destination: DESTINATION.into(),
            bytes: 12,
            blake3: "content-hash".into(),
            sha256: "sha256-hash".into(),
            quick_xor_base64: "quick-xor".into(),
            source_modified_ms: 2,
            copied_at_ms: 100,
            copy_verified: true,
            provider_sync_confirmed: false,
        };
        receipt.receipt_id = receipt_id_for(
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
        );
        receipt
    }

    fn receipt_for(provider: CloudProvider) -> CloudCopyReceipt {
        let mut provider_receipt = receipt();
        provider_receipt.provider = provider;
        provider_receipt.receipt_id = receipt_id_for(
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
        );
        provider_receipt
    }

    fn evidence() -> ProviderSyncEvidence {
        ProviderSyncEvidence {
            receipt_id: receipt().receipt_id,
            provider: CloudProvider::Icloud,
            destination: DESTINATION.into(),
            observed_bytes: 12,
            destination_blake3: "content-hash".into(),
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
        rejected.production_time_source = "filename-date".into();
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
    fn provider_sync_evidence_is_required_before_eviction_permit() {
        let valid_receipt = receipt();
        assert!(receipt_blockers(&valid_receipt).is_empty());
        let approved = approve_local_eviction(&valid_receipt, &evidence()).unwrap();
        assert_eq!(approved.receipt_id, valid_receipt.receipt_id);
        assert_eq!(approved.provider, CloudProvider::Icloud);
        assert_eq!(approved.source, SOURCE);
        assert_eq!(approved.destination, DESTINATION);
        assert_eq!(approved.bytes, 12);
        assert_eq!(approved.blake3, "content-hash");
        assert_eq!(approved.approved_at_ms, 101);
        assert_eq!(
            approved.evidence_kind,
            SyncEvidenceKind::ProviderNativeStatus
        );
        assert_eq!(approved.evidence_id, "icloud-uploaded-flag");

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
        let blockers = approve_local_eviction(&invalid_receipt, &invalid_evidence).unwrap_err();
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
                }),
            };
            assert!(approve_local_eviction(&provider_receipt, &api_evidence).is_ok());
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
        assert!(approve_local_eviction(&provider_receipt, &api_evidence)
            .unwrap_err()
            .contains(&"remote-content-proof-missing".to_string()));

        api_evidence.remote_content = Some(RemoteContentProof {
            object_id: " ".into(),
            revision: " ".into(),
            algorithm: RemoteChecksumAlgorithm::Sha256,
            checksum: "wrong".into(),
        });
        let blockers = approve_local_eviction(&provider_receipt, &api_evidence).unwrap_err();
        for expected in [
            "remote-object-id-missing",
            "remote-revision-missing",
            "remote-checksum-mismatch",
        ] {
            assert!(blockers.contains(&expected.to_string()), "{expected}");
        }

        api_evidence.kind = SyncEvidenceKind::ProviderNativeStatus;
        api_evidence.remote_content = None;
        assert!(approve_local_eviction(&provider_receipt, &api_evidence)
            .unwrap_err()
            .contains(&"native-status-provider-unsupported".to_string()));
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
        let test_root = CloudRoot {
            id: "icloud:test".into(),
            provider: CloudProvider::Icloud,
            label: "iCloud Drive".into(),
            path: cloud.to_string_lossy().into_owned(),
        };
        let receipt_dir = tmp.path().join("receipts");
        let (copy_receipt, receipt_path) =
            prepare_cloud_copy(&test_candidate, &test_root, &receipt_dir, 123).unwrap();
        assert!(source.exists());
        assert_eq!(std::fs::read(&destination).unwrap(), b"hello-cloud");
        assert_eq!(copy_receipt.blake3, hash_file(&source).unwrap().blake3);
        assert!(receipt_path.metadata().unwrap().permissions().readonly());
        let persisted = read_immutable_receipt(&receipt_path).unwrap();
        assert_eq!(persisted, copy_receipt);

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
        let test_root = CloudRoot {
            id: "icloud:test".into(),
            provider: CloudProvider::Icloud,
            label: "iCloud Drive".into(),
            path: cloud.to_string_lossy().into_owned(),
        };
        let receipt_dir = tmp.path().join("receipts");
        assert_eq!(
            prepare_cloud_copy(&test_candidate, &test_root, &receipt_dir, 123).unwrap_err(),
            "source-changed-since-plan"
        );
        test_candidate.bytes = std::fs::metadata(&source).unwrap().len();
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
        let test_root = CloudRoot {
            id: "icloud:test".into(),
            provider: CloudProvider::Icloud,
            label: "iCloud Drive".into(),
            path: cloud.to_string_lossy().into_owned(),
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
        let test_root = CloudRoot {
            id: "icloud:test".into(),
            provider: CloudProvider::Icloud,
            label: "iCloud Drive".into(),
            path: cloud.to_string_lossy().into_owned(),
        };
        let content_hash = hash_file(&source).unwrap();
        let receipt_id = receipt_id_for(
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
