//! Receipt-bound, crash-resumable movement of verified cloud sources to the OS Trash.
//!
//! The cloud copy and fresh provider-native evidence are validated by `cloud_transfer` before a
//! permit reaches this module. We independently bind that permit to the immutable receipt, verify
//! the local source bytes and identity, stage with a same-directory rename, verify again, and then
//! delegate to the application's only trash operation.

use crate::cloud::CloudProvider;
use crate::cloud_local_eviction::{observe_path_active_use, ActiveUseEvidence};
use crate::cloud_review;
use crate::cloud_transfer::{
    receipt_blockers, CloudCopyReceipt, LocalEvictionPermit, SyncEvidenceKind,
};
use crate::content_digest::{ContentDigests, ContentHasher};
use crate::safety;
use serde::de::DeserializeOwned;
use std::io::{Read, Write};
use std::path::Path;

const EVICTION_RECORD_VERSION: u32 = 2;
const SOURCE_EVICTION_APPROVAL_VERSION: u32 = 1;
const MAX_RECORD_BYTES: u64 = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CloudSourceEvictionApproval {
    pub version: u32,
    pub approval_id: String,
    pub receipt_id: String,
    pub evidence_record_id: String,
    pub approved_at_ms: u64,
    pub approved_by: String,
    pub rationale: String,
    pub active_use_observed_at_ms: u64,
    pub active_use: ActiveUseEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct SourceIdentity {
    bytes: u64,
    modified_ms: u64,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CloudEvictionIntent {
    version: u32,
    intent_id: String,
    receipt_id: String,
    provider: CloudProvider,
    source: String,
    staging_dir: String,
    staged_source: String,
    destination: String,
    bytes: u64,
    blake3: String,
    sha256: String,
    quick_xor_base64: String,
    source_modified_ms: u64,
    approved_at_ms: u64,
    evidence_kind: SyncEvidenceKind,
    evidence_id: String,
    evidence_record_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    human_approval: Option<CloudSourceEvictionApproval>,
    created_at_ms: u64,
    source_identity: SourceIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CloudEvictionCompletion {
    version: u32,
    completion_id: String,
    intent_id: String,
    receipt_id: String,
    evidence_record_id: String,
    completed_at_ms: u64,
    reconciled_after_interruption: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CloudEvictionResult {
    pub action: &'static str,
    pub receipt_id: String,
    pub intent_id: String,
    pub completion_id: String,
    pub evidence_record_id: String,
    pub approval_id: Option<String>,
    pub source: String,
    pub staged_source: String,
    pub intent_path: String,
    pub completion_path: String,
    pub source_trashed: bool,
    pub reconciled_after_interruption: bool,
    pub already_completed: bool,
}

fn absolute_without_parent(path: &Path) -> bool {
    path.is_absolute()
        && !path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
}

fn valid_hex64(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn approval_active_use_is_safe(evidence: &ActiveUseEvidence) -> bool {
    evidence.method == "lsof-fp+ps-command"
        && evidence.evidence_complete
        && !evidence.active
        && evidence.observed_pids.is_empty()
        && !evidence.results_truncated
        && evidence.error.is_none()
}

fn source_eviction_approval_id_for(approval: &CloudSourceEvictionApproval) -> String {
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
    hasher.finalize().to_hex().to_string()
}

fn validate_source_eviction_approval(
    receipt: &CloudCopyReceipt,
    permit: &LocalEvictionPermit,
    approval: &CloudSourceEvictionApproval,
) -> Result<(), String> {
    validate_permit(receipt, permit)?;
    if approval.version != SOURCE_EVICTION_APPROVAL_VERSION
        || approval.receipt_id != receipt.receipt_id
        || approval.evidence_record_id != permit.evidence_record_id
        || approval.approved_at_ms < permit.approved_at_ms
        || approval.active_use_observed_at_ms < permit.approved_at_ms
        || approval.active_use_observed_at_ms > approval.approved_at_ms
        || !approval_active_use_is_safe(&approval.active_use)
        || approval.approval_id != source_eviction_approval_id_for(approval)
    {
        return Err("source-eviction-human-approval-invalid".into());
    }
    cloud_review::validate_review_attribution(&approval.approved_by, &approval.rationale)
        .map_err(|_| "source-eviction-human-approval-attribution-invalid".to_string())?;
    Ok(())
}

pub fn create_source_eviction_approval(
    receipt: &CloudCopyReceipt,
    permit: &LocalEvictionPermit,
    confirmation_receipt_id: &str,
    approved_at_ms: u64,
    approved_by: &str,
    rationale: &str,
    active_use_observed_at_ms: u64,
    active_use: ActiveUseEvidence,
) -> Result<CloudSourceEvictionApproval, String> {
    if confirmation_receipt_id != receipt.receipt_id {
        return Err("eviction-confirmation-receipt-id-mismatch".into());
    }
    validate_permit(receipt, permit)?;
    let approved_by = approved_by.trim();
    let rationale = rationale.trim();
    cloud_review::validate_review_attribution(approved_by, rationale)
        .map_err(|_| "source-eviction-human-approval-attribution-invalid".to_string())?;
    if !approval_active_use_is_safe(&active_use)
        || active_use_observed_at_ms < permit.approved_at_ms
        || active_use_observed_at_ms > approved_at_ms
    {
        return Err("source-eviction-active-use-evidence-invalid".into());
    }
    let mut approval = CloudSourceEvictionApproval {
        version: SOURCE_EVICTION_APPROVAL_VERSION,
        approval_id: String::new(),
        receipt_id: receipt.receipt_id.clone(),
        evidence_record_id: permit.evidence_record_id.clone(),
        approved_at_ms,
        approved_by: approved_by.into(),
        rationale: rationale.into(),
        active_use_observed_at_ms,
        active_use,
    };
    approval.approval_id = source_eviction_approval_id_for(&approval);
    Ok(approval)
}

fn path_entry_exists(path: &Path) -> Result<bool, String> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.to_string()),
    }
}

fn modified_ms(metadata: &std::fs::Metadata) -> Result<u64, String> {
    metadata
        .modified()
        .map_err(|error| error.to_string())?
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .map_err(|error| error.to_string())
}

fn source_identity(metadata: &std::fs::Metadata) -> Result<SourceIdentity, String> {
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("eviction-source-must-be-regular-file".into());
    }
    #[cfg(unix)]
    use std::os::unix::fs::MetadataExt;
    Ok(SourceIdentity {
        bytes: metadata.len(),
        modified_ms: modified_ms(metadata)?,
        #[cfg(unix)]
        device: metadata.dev(),
        #[cfg(unix)]
        inode: metadata.ino(),
    })
}

fn hash_stable_file(path: &Path) -> Result<(SourceIdentity, ContentDigests), String> {
    let before = std::fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    let identity = source_identity(&before)?;
    let mut file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    if source_identity(&file.metadata().map_err(|error| error.to_string())?)? != identity {
        return Err("eviction-source-changed-before-read".into());
    }
    let mut hasher = ContentHasher::default();
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let after = std::fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if source_identity(&after)? != identity {
        return Err("eviction-source-changed-during-read".into());
    }
    Ok((identity, hasher.finalize()))
}

fn verify_source(
    path: &Path,
    receipt: &CloudCopyReceipt,
    expected_identity: Option<&SourceIdentity>,
) -> Result<SourceIdentity, String> {
    let (identity, digests) = hash_stable_file(path)?;
    if identity.bytes != receipt.bytes {
        return Err("eviction-source-size-mismatch".into());
    }
    if identity.modified_ms != receipt.source_modified_ms {
        return Err("eviction-source-modified-time-mismatch".into());
    }
    if digests.blake3 != receipt.blake3
        || digests.sha256 != receipt.sha256
        || digests.quick_xor_base64 != receipt.quick_xor_base64
    {
        return Err("eviction-source-content-mismatch".into());
    }
    if expected_identity.is_some_and(|expected| *expected != identity) {
        return Err("eviction-source-identity-mismatch".into());
    }
    Ok(identity)
}

fn validate_permit(receipt: &CloudCopyReceipt, permit: &LocalEvictionPermit) -> Result<(), String> {
    let blockers = receipt_blockers(receipt);
    if !blockers.is_empty() {
        return Err(blockers.join(","));
    }
    if permit.receipt_id != receipt.receipt_id
        || permit.provider != receipt.provider
        || permit.source != receipt.source
        || permit.destination != receipt.destination
        || permit.bytes != receipt.bytes
        || permit.blake3 != receipt.blake3
    {
        return Err("eviction-permit-receipt-mismatch".into());
    }
    if permit.approved_at_ms < receipt.copied_at_ms
        || permit.evidence_id.trim().is_empty()
        || !valid_hex64(&permit.evidence_record_id)
    {
        return Err("eviction-permit-invalid".into());
    }
    Ok(())
}

fn intent_id_for(intent: &CloudEvictionIntent) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&intent.version.to_le_bytes());
    for field in [
        intent.receipt_id.as_str(),
        intent.provider.as_str(),
        intent.source.as_str(),
        intent.staging_dir.as_str(),
        intent.staged_source.as_str(),
        intent.destination.as_str(),
        intent.blake3.as_str(),
        intent.sha256.as_str(),
        intent.quick_xor_base64.as_str(),
        intent.evidence_id.as_str(),
        intent.evidence_record_id.as_str(),
    ] {
        hasher.update(field.as_bytes());
        hasher.update(&[0]);
    }
    hasher.update(&intent.bytes.to_le_bytes());
    hasher.update(&intent.source_modified_ms.to_le_bytes());
    hasher.update(&intent.approved_at_ms.to_le_bytes());
    hasher.update(match intent.evidence_kind {
        SyncEvidenceKind::ProviderApi => b"provider-api",
        SyncEvidenceKind::ProviderNativeStatus => b"provider-native-status",
    });
    hasher.update(&intent.created_at_ms.to_le_bytes());
    if let Some(approval) = &intent.human_approval {
        hasher.update(b"human-approval-v1\0");
        hasher.update(approval.approval_id.as_bytes());
        hasher.update(&[0]);
    }
    hasher.update(&intent.source_identity.bytes.to_le_bytes());
    hasher.update(&intent.source_identity.modified_ms.to_le_bytes());
    #[cfg(unix)]
    {
        hasher.update(&intent.source_identity.device.to_le_bytes());
        hasher.update(&intent.source_identity.inode.to_le_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

fn completion_id_for(completion: &CloudEvictionCompletion) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&completion.version.to_le_bytes());
    hasher.update(completion.intent_id.as_bytes());
    hasher.update(&[0]);
    hasher.update(completion.receipt_id.as_bytes());
    hasher.update(&[0]);
    hasher.update(completion.evidence_record_id.as_bytes());
    hasher.update(&completion.completed_at_ms.to_le_bytes());
    hasher.update(&[completion.reconciled_after_interruption as u8]);
    hasher.finalize().to_hex().to_string()
}

fn same_record_file_identity(expected: &std::fs::Metadata, observed: &std::fs::Metadata) -> bool {
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

fn read_immutable_json<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    let metadata = std::fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || !metadata.permissions().readonly()
    {
        return Err("eviction-record-must-be-read-only-regular-file".into());
    }
    if metadata.len() > MAX_RECORD_BYTES {
        return Err("eviction-record-too-large".into());
    }
    let mut file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    if !same_record_file_identity(
        &metadata,
        &file.metadata().map_err(|error| error.to_string())?,
    ) {
        return Err("eviction-record-changed-during-read".into());
    }
    let mut encoded = Vec::with_capacity(metadata.len() as usize);
    Read::by_ref(&mut file)
        .take(MAX_RECORD_BYTES + 1)
        .read_to_end(&mut encoded)
        .map_err(|error| error.to_string())?;
    if encoded.len() as u64 > MAX_RECORD_BYTES {
        return Err("eviction-record-too-large".into());
    }
    let after = std::fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if !same_record_file_identity(&metadata, &after) {
        return Err("eviction-record-changed-during-read".into());
    }
    serde_json::from_slice(&encoded).map_err(|_| "eviction-record-json-invalid".into())
}

fn write_immutable_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let encoded = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    if encoded.len() as u64 > MAX_RECORD_BYTES {
        return Err("eviction-record-too-large".into());
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
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
        std::fs::set_permissions(path, permissions).map_err(|error| error.to_string())?;
        #[cfg(unix)]
        if let Some(parent) = path.parent() {
            std::fs::File::open(parent)
                .and_then(|directory| directory.sync_all())
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    })();
    if let Err(error) = result {
        drop(file);
        let _ = std::fs::remove_file(path);
        return Err(error);
    }
    Ok(())
}

fn validate_intent(
    intent: &CloudEvictionIntent,
    receipt: &CloudCopyReceipt,
    permit: &LocalEvictionPermit,
    human_approval: Option<&CloudSourceEvictionApproval>,
    expected_staging_dir: &Path,
    expected_staged_source: &Path,
) -> Result<(), String> {
    if intent.version != EVICTION_RECORD_VERSION || intent.intent_id != intent_id_for(intent) {
        return Err("eviction-intent-integrity-mismatch".into());
    }
    if intent.receipt_id != receipt.receipt_id
        || intent.provider != receipt.provider
        || intent.source != receipt.source
        || intent.destination != receipt.destination
        || intent.bytes != receipt.bytes
        || intent.blake3 != receipt.blake3
        || intent.sha256 != receipt.sha256
        || intent.quick_xor_base64 != receipt.quick_xor_base64
        || intent.source_modified_ms != receipt.source_modified_ms
        || Path::new(&intent.staging_dir) != expected_staging_dir
        || Path::new(&intent.staged_source) != expected_staged_source
    {
        return Err("eviction-intent-receipt-mismatch".into());
    }
    if intent.approved_at_ms != permit.approved_at_ms
        || intent.evidence_kind != permit.evidence_kind
        || intent.evidence_id != permit.evidence_id
        || intent.evidence_record_id != permit.evidence_record_id
    {
        return Err("eviction-intent-permit-mismatch".into());
    }
    if intent.human_approval.as_ref() != human_approval {
        return Err("eviction-intent-human-approval-mismatch".into());
    }
    if let Some(approval) = human_approval {
        validate_source_eviction_approval(receipt, permit, approval)?;
    }
    Ok(())
}

fn validate_completion(
    completion: &CloudEvictionCompletion,
    intent: &CloudEvictionIntent,
) -> Result<(), String> {
    if completion.version != EVICTION_RECORD_VERSION
        || completion.completion_id != completion_id_for(completion)
    {
        return Err("eviction-completion-integrity-mismatch".into());
    }
    if completion.intent_id != intent.intent_id
        || completion.receipt_id != intent.receipt_id
        || completion.evidence_record_id != intent.evidence_record_id
    {
        return Err("eviction-completion-intent-mismatch".into());
    }
    Ok(())
}

fn ensure_record_directory(path: &Path) -> Result<(), String> {
    if !absolute_without_parent(path) {
        return Err("eviction-dir-must-be-safe-absolute".into());
    }
    std::fs::create_dir_all(path).map_err(|error| error.to_string())?;
    let metadata = std::fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("eviction-dir-must-be-real-directory".into());
    }
    Ok(())
}

fn ensure_journal_parent(path: &Path) -> Result<(), String> {
    if !absolute_without_parent(path) {
        return Err("journal-path-must-be-safe-absolute".into());
    }
    let parent = path
        .parent()
        .ok_or_else(|| "journal-path-parent-missing".to_string())?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let metadata = std::fs::symlink_metadata(parent).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("journal-parent-must-be-real-directory".into());
    }
    Ok(())
}

fn result_from_completion(
    intent: &CloudEvictionIntent,
    completion: &CloudEvictionCompletion,
    intent_path: &Path,
    completion_path: &Path,
    source_trashed: bool,
    already_completed: bool,
) -> CloudEvictionResult {
    CloudEvictionResult {
        action: "trash-verified-cloud-source",
        receipt_id: intent.receipt_id.clone(),
        intent_id: intent.intent_id.clone(),
        completion_id: completion.completion_id.clone(),
        evidence_record_id: intent.evidence_record_id.clone(),
        approval_id: intent
            .human_approval
            .as_ref()
            .map(|approval| approval.approval_id.clone()),
        source: intent.source.clone(),
        staged_source: intent.staged_source.clone(),
        intent_path: intent_path.to_string_lossy().into_owned(),
        completion_path: completion_path.to_string_lossy().into_owned(),
        source_trashed,
        reconciled_after_interruption: completion.reconciled_after_interruption,
        already_completed,
    }
}

fn unexpected_staging_entries(staging_dir: &Path, staged_source: &Path) -> Result<bool, String> {
    if !path_entry_exists(staging_dir)? {
        return Ok(false);
    }
    let metadata = std::fs::symlink_metadata(staging_dir).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("eviction-staging-dir-must-be-real-directory".into());
    }
    for entry in std::fs::read_dir(staging_dir).map_err(|error| error.to_string())? {
        if entry.map_err(|error| error.to_string())?.path() != staged_source {
            return Ok(true);
        }
    }
    Ok(false)
}

fn evict_source_with_context<F>(
    receipt: &CloudCopyReceipt,
    permit: &LocalEvictionPermit,
    human_approval: Option<&CloudSourceEvictionApproval>,
    confirmation_receipt_id: &str,
    eviction_dir: &Path,
    journal_path: &Path,
    now_ms: u64,
    trash_move: F,
) -> Result<CloudEvictionResult, String>
where
    F: Fn(&Path, u64, &Path, u64) -> Result<(), String>,
{
    validate_permit(receipt, permit)?;
    if let Some(approval) = human_approval {
        validate_source_eviction_approval(receipt, permit, approval)?;
    }
    if confirmation_receipt_id != receipt.receipt_id {
        return Err("eviction-confirmation-receipt-id-mismatch".into());
    }
    let source = Path::new(&receipt.source);
    if !absolute_without_parent(source) || safety::is_protected(source) {
        return Err("eviction-source-path-not-safe".into());
    }
    let source_name = source
        .file_name()
        .ok_or_else(|| "eviction-source-filename-missing".to_string())?;
    let source_parent = source
        .parent()
        .ok_or_else(|| "eviction-source-parent-missing".to_string())?;
    let staging_dir = source_parent.join(format!(".disksage-evict-{}", receipt.receipt_id));
    let staged_source = staging_dir.join(source_name);
    if eviction_dir == staging_dir || eviction_dir.starts_with(&staging_dir) {
        return Err("eviction-control-path-overlaps-staging".into());
    }
    if journal_path == source
        || journal_path == Path::new(&receipt.destination)
        || journal_path == staged_source
        || journal_path == staging_dir
        || journal_path.starts_with(&staging_dir)
    {
        return Err("eviction-journal-path-overlaps-data".into());
    }
    ensure_record_directory(eviction_dir)?;
    ensure_journal_parent(journal_path)?;
    let intent_path = eviction_dir.join(format!("{}.intent.json", receipt.receipt_id));
    let completion_path = eviction_dir.join(format!("{}.complete.json", receipt.receipt_id));

    if intent_path == source
        || completion_path == source
        || journal_path == intent_path
        || journal_path == completion_path
    {
        return Err("eviction-control-path-overlaps-data".into());
    }

    let intent = if path_entry_exists(&intent_path)? {
        let intent: CloudEvictionIntent = read_immutable_json(&intent_path)?;
        validate_intent(
            &intent,
            receipt,
            permit,
            human_approval,
            &staging_dir,
            &staged_source,
        )?;
        intent
    } else {
        if path_entry_exists(&staging_dir)? {
            return Err("eviction-staging-dir-exists-without-intent".into());
        }
        let identity = verify_source(source, receipt, None)?;
        let mut intent = CloudEvictionIntent {
            version: EVICTION_RECORD_VERSION,
            intent_id: String::new(),
            receipt_id: receipt.receipt_id.clone(),
            provider: receipt.provider,
            source: receipt.source.clone(),
            staging_dir: staging_dir.to_string_lossy().into_owned(),
            staged_source: staged_source.to_string_lossy().into_owned(),
            destination: receipt.destination.clone(),
            bytes: receipt.bytes,
            blake3: receipt.blake3.clone(),
            sha256: receipt.sha256.clone(),
            quick_xor_base64: receipt.quick_xor_base64.clone(),
            source_modified_ms: receipt.source_modified_ms,
            approved_at_ms: permit.approved_at_ms,
            evidence_kind: permit.evidence_kind,
            evidence_id: permit.evidence_id.clone(),
            evidence_record_id: permit.evidence_record_id.clone(),
            human_approval: human_approval.cloned(),
            created_at_ms: now_ms,
            source_identity: identity,
        };
        intent.intent_id = intent_id_for(&intent);
        write_immutable_json(&intent_path, &intent)?;
        intent
    };

    if path_entry_exists(&completion_path)? {
        let completion: CloudEvictionCompletion = read_immutable_json(&completion_path)?;
        validate_completion(&completion, &intent)?;
        return Ok(result_from_completion(
            &intent,
            &completion,
            &intent_path,
            &completion_path,
            false,
            true,
        ));
    }
    if unexpected_staging_entries(&staging_dir, &staged_source)? {
        return Err("eviction-staging-dir-has-unexpected-entries".into());
    }

    let source_exists = path_entry_exists(source)?;
    let staged_exists = path_entry_exists(&staged_source)?;
    if source_exists && staged_exists {
        return Err("eviction-source-and-staging-both-exist".into());
    }
    let reconciled_after_interruption = !source_exists && !staged_exists;
    if source_exists {
        verify_source(source, receipt, Some(&intent.source_identity))?;
        if human_approval.is_some()
            && !approval_active_use_is_safe(&observe_path_active_use(source))
        {
            return Err("source-eviction-live-active-use-blocked".into());
        }
        if !path_entry_exists(&staging_dir)? {
            std::fs::create_dir(&staging_dir).map_err(|error| error.to_string())?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&staging_dir, std::fs::Permissions::from_mode(0o700))
                    .map_err(|error| error.to_string())?;
            }
        }
        std::fs::rename(source, &staged_source).map_err(|error| error.to_string())?;
        if let Err(error) = verify_source(&staged_source, receipt, Some(&intent.source_identity)) {
            let restore = if !path_entry_exists(source)? {
                std::fs::rename(&staged_source, source).map_err(|restore| restore.to_string())
            } else {
                Err("eviction-source-reappeared-before-restore".into())
            };
            return match restore {
                Ok(()) => Err(error),
                Err(restore_error) => Err(format!(
                    "{error}; eviction-staging-restore-failed:{restore_error}"
                )),
            };
        }
    }
    if !reconciled_after_interruption {
        verify_source(&staged_source, receipt, Some(&intent.source_identity))?;
        if human_approval.is_some()
            && !approval_active_use_is_safe(&observe_path_active_use(&staged_source))
        {
            if !path_entry_exists(source)? {
                std::fs::rename(&staged_source, source).map_err(|error| error.to_string())?;
                std::fs::remove_dir(&staging_dir).map_err(|error| error.to_string())?;
            }
            return Err("source-eviction-live-active-use-blocked".into());
        }
        trash_move(&staging_dir, receipt.bytes, journal_path, now_ms)?;
        if path_entry_exists(source)? || path_entry_exists(&staging_dir)? {
            return Err("eviction-trash-did-not-remove-staging".into());
        }
    }

    let mut completion = CloudEvictionCompletion {
        version: EVICTION_RECORD_VERSION,
        completion_id: String::new(),
        intent_id: intent.intent_id.clone(),
        receipt_id: receipt.receipt_id.clone(),
        evidence_record_id: intent.evidence_record_id.clone(),
        completed_at_ms: now_ms,
        reconciled_after_interruption,
    };
    completion.completion_id = completion_id_for(&completion);
    write_immutable_json(&completion_path, &completion)?;
    Ok(result_from_completion(
        &intent,
        &completion,
        &intent_path,
        &completion_path,
        !reconciled_after_interruption,
        false,
    ))
}

#[cfg(test)]
fn evict_source_with<F>(
    receipt: &CloudCopyReceipt,
    permit: &LocalEvictionPermit,
    confirmation_receipt_id: &str,
    eviction_dir: &Path,
    journal_path: &Path,
    now_ms: u64,
    trash_move: F,
) -> Result<CloudEvictionResult, String>
where
    F: Fn(&Path, u64, &Path, u64) -> Result<(), String>,
{
    evict_source_with_context(
        receipt,
        permit,
        None,
        confirmation_receipt_id,
        eviction_dir,
        journal_path,
        now_ms,
        trash_move,
    )
}

pub fn write_immutable_source_eviction_approval(
    approval_dir: &Path,
    approval: &CloudSourceEvictionApproval,
) -> Result<std::path::PathBuf, String> {
    if approval.version != SOURCE_EVICTION_APPROVAL_VERSION
        || !valid_hex64(&approval.approval_id)
        || !valid_hex64(&approval.receipt_id)
        || !valid_hex64(&approval.evidence_record_id)
        || approval.approval_id != source_eviction_approval_id_for(approval)
        || approval.active_use_observed_at_ms > approval.approved_at_ms
        || !approval_active_use_is_safe(&approval.active_use)
    {
        return Err("source-eviction-human-approval-invalid".into());
    }
    cloud_review::validate_review_attribution(&approval.approved_by, &approval.rationale)
        .map_err(|_| "source-eviction-human-approval-attribution-invalid".to_string())?;
    ensure_record_directory(approval_dir)?;
    let path = approval_dir.join(format!("{}.approval.json", approval.approval_id));
    write_immutable_json(&path, approval)?;
    Ok(path)
}

/// Move a receipt-bound source to Trash only after an attributed, receipt-confirmed human
/// approval and two bounded active-use observations. The approval captures the first observation;
/// this entrypoint recollects it immediately before staging so a newly opened file fails closed.
pub fn evict_source_with_human_approval(
    receipt: &CloudCopyReceipt,
    permit: &LocalEvictionPermit,
    approval: &CloudSourceEvictionApproval,
    confirmation_receipt_id: &str,
    eviction_dir: &Path,
    journal_path: &Path,
    now_ms: u64,
) -> Result<CloudEvictionResult, String> {
    validate_source_eviction_approval(receipt, permit, approval)?;
    let live_active_use = observe_path_active_use(Path::new(&receipt.source));
    if !approval_active_use_is_safe(&live_active_use) {
        return Err("source-eviction-live-active-use-blocked".into());
    }
    evict_source_with_context(
        receipt,
        permit,
        Some(approval),
        confirmation_receipt_id,
        eviction_dir,
        journal_path,
        now_ms,
        |path, bytes, journal, timestamp| {
            safety::trash_delete(path, bytes, journal, timestamp).map_err(|error| error.to_string())
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::{
        candidate_review_fingerprint, ArchiveKind, CloudCandidate, CloudRoot, MetadataEvidence,
    };
    use crate::cloud_transfer::{approve_local_eviction, prepare_cloud_copy, ProviderSyncEvidence};
    use crate::provider_evidence::create_sync_evidence_record;

    fn valid_receipt(temp: &tempfile::TempDir) -> (CloudCopyReceipt, LocalEvictionPermit) {
        let source_dir = temp.path().join("source");
        let cloud_dir = temp.path().join("cloud");
        let receipt_dir = temp.path().join("receipts");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::create_dir_all(&cloud_dir).unwrap();
        let source = source_dir.join("report.bin");
        let destination = cloud_dir.join("report.bin");
        std::fs::write(&source, b"verified source bytes").unwrap();
        let metadata = std::fs::metadata(&source).unwrap();
        let modified = modified_ms(&metadata).unwrap();
        let mut candidate = CloudCandidate {
            metadata_fingerprint: "a".repeat(64),
            review_fingerprint: String::new(),
            src: source.to_string_lossy().into_owned(),
            dst: destination.to_string_lossy().into_owned(),
            provider: CloudProvider::Onedrive,
            destination_account_scope: crate::cloud::CloudAccountScope::Organization,
            kind: ArchiveKind::Document,
            bytes: metadata.len(),
            age_days: 1,
            created_ms: modified,
            modified_ms: modified,
            production_time_ms: modified,
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
            account_scope: crate::cloud::CloudAccountScope::Organization,
            label: "test".into(),
            path: cloud_dir.to_string_lossy().into_owned(),
            readable: true,
            access_issue: None,
        };
        let (receipt, _) = prepare_cloud_copy(&candidate, &root, &receipt_dir, 100).unwrap();
        let evidence = ProviderSyncEvidence {
            receipt_id: receipt.receipt_id.clone(),
            provider: receipt.provider,
            destination: receipt.destination.clone(),
            observed_bytes: receipt.bytes,
            destination_blake3: receipt.blake3.clone(),
            confirmed_at_ms: 101,
            kind: SyncEvidenceKind::ProviderNativeStatus,
            evidence_id: "native-test-evidence".into(),
            sync_complete: true,
            remote_content: None,
        };
        let evidence_record = create_sync_evidence_record(&evidence).unwrap();
        let permit = approve_local_eviction(&receipt, &evidence_record).unwrap();
        (receipt, permit)
    }

    fn mock_trash(path: &Path, _: u64, _: &Path, _: u64) -> Result<(), String> {
        std::fs::remove_dir_all(path).map_err(|error| error.to_string())
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
    fn attributed_human_approval_is_immutable_and_bound_into_intent() {
        let temp = tempfile::tempdir().unwrap();
        let (receipt, permit) = valid_receipt(&temp);
        let approval = create_source_eviction_approval(
            &receipt,
            &permit,
            &receipt.receipt_id,
            160,
            "human:local:test",
            "verified cloud copy; move only this source to Trash",
            150,
            idle_active_use(),
        )
        .unwrap();
        let approval_path =
            write_immutable_source_eviction_approval(&temp.path().join("approvals"), &approval)
                .unwrap();
        assert!(approval_path.metadata().unwrap().permissions().readonly());

        let result = evict_source_with_context(
            &receipt,
            &permit,
            Some(&approval),
            &receipt.receipt_id,
            &temp.path().join("evictions"),
            &temp.path().join("journal/operations.jsonl"),
            200,
            mock_trash,
        )
        .unwrap();
        assert_eq!(
            result.approval_id.as_deref(),
            Some(approval.approval_id.as_str())
        );
        let intent: CloudEvictionIntent =
            read_immutable_json(Path::new(&result.intent_path)).unwrap();
        assert_eq!(intent.human_approval, Some(approval.clone()));

        let mut future_evidence = approval.clone();
        future_evidence.active_use_observed_at_ms = future_evidence.approved_at_ms + 1;
        future_evidence.approval_id = source_eviction_approval_id_for(&future_evidence);
        assert_eq!(
            write_immutable_source_eviction_approval(
                &temp.path().join("invalid-approvals"),
                &future_evidence,
            )
            .unwrap_err(),
            "source-eviction-human-approval-invalid"
        );

        let mut tampered = approval;
        tampered.rationale.push_str(" changed");
        assert_eq!(
            validate_source_eviction_approval(&receipt, &permit, &tampered).unwrap_err(),
            "source-eviction-human-approval-invalid"
        );
    }

    #[test]
    fn human_approval_rejects_active_or_unattributed_execution() {
        let temp = tempfile::tempdir().unwrap();
        let (receipt, permit) = valid_receipt(&temp);
        let mut active = idle_active_use();
        active.active = true;
        active.observed_pids = vec![42];
        assert_eq!(
            create_source_eviction_approval(
                &receipt,
                &permit,
                &receipt.receipt_id,
                160,
                "human:local:test",
                "specific source approval",
                150,
                active,
            )
            .unwrap_err(),
            "source-eviction-active-use-evidence-invalid"
        );
        assert_eq!(
            create_source_eviction_approval(
                &receipt,
                &permit,
                &receipt.receipt_id,
                160,
                "agent:test",
                "specific source approval",
                150,
                idle_active_use(),
            )
            .unwrap_err(),
            "source-eviction-human-approval-attribution-invalid"
        );
    }

    #[test]
    fn verified_source_is_staged_rechecked_and_completed_once() {
        let temp = tempfile::tempdir().unwrap();
        let (receipt, permit) = valid_receipt(&temp);
        let records = temp.path().join("evictions");
        let journal = temp.path().join("journal/operations.jsonl");
        let result = evict_source_with(
            &receipt,
            &permit,
            &receipt.receipt_id,
            &records,
            &journal,
            200,
            mock_trash,
        )
        .unwrap();
        assert!(result.source_trashed);
        assert!(!result.already_completed);
        assert_eq!(result.evidence_record_id, permit.evidence_record_id);
        assert!(!Path::new(&receipt.source).exists());
        assert!(Path::new(&receipt.destination).exists());
        assert!(Path::new(&result.intent_path)
            .metadata()
            .unwrap()
            .permissions()
            .readonly());
        assert!(Path::new(&result.completion_path)
            .metadata()
            .unwrap()
            .permissions()
            .readonly());

        let intent: CloudEvictionIntent =
            read_immutable_json(Path::new(&result.intent_path)).unwrap();
        let completion: CloudEvictionCompletion =
            read_immutable_json(Path::new(&result.completion_path)).unwrap();
        assert_eq!(intent.evidence_record_id, permit.evidence_record_id);
        assert_eq!(completion.evidence_record_id, permit.evidence_record_id);

        let mut substituted_permit = permit.clone();
        substituted_permit.evidence_record_id = "f".repeat(64);
        assert_eq!(
            evict_source_with(
                &receipt,
                &substituted_permit,
                &receipt.receipt_id,
                &records,
                &journal,
                201,
                mock_trash,
            )
            .unwrap_err(),
            "eviction-intent-permit-mismatch"
        );

        std::fs::write(&receipt.source, b"later unrelated file").unwrap();
        let repeated = evict_source_with(
            &receipt,
            &permit,
            &receipt.receipt_id,
            &records,
            &journal,
            201,
            mock_trash,
        )
        .unwrap();
        assert!(repeated.already_completed);
        assert!(Path::new(&receipt.source).exists());
    }

    #[test]
    fn wrong_confirmation_or_changed_source_never_stages() {
        let temp = tempfile::tempdir().unwrap();
        let (receipt, permit) = valid_receipt(&temp);
        let records = temp.path().join("evictions");
        let journal = temp.path().join("journal/operations.jsonl");
        assert_eq!(
            evict_source_with(
                &receipt,
                &permit,
                &"0".repeat(64),
                &records,
                &journal,
                200,
                mock_trash,
            )
            .unwrap_err(),
            "eviction-confirmation-receipt-id-mismatch"
        );
        std::fs::write(&receipt.source, b"changed source bytes").unwrap();
        assert!(evict_source_with(
            &receipt,
            &permit,
            &receipt.receipt_id,
            &records,
            &journal,
            200,
            mock_trash,
        )
        .unwrap_err()
        .starts_with("eviction-source-"));
        assert!(Path::new(&receipt.source).exists());
        assert!(!records
            .join(format!("{}.intent.json", receipt.receipt_id))
            .exists());
    }

    #[test]
    fn control_paths_cannot_overlap_source_or_staging() {
        let temp = tempfile::tempdir().unwrap();
        let (receipt, permit) = valid_receipt(&temp);
        let records = temp.path().join("evictions");
        let source = Path::new(&receipt.source);
        assert_eq!(
            evict_source_with(
                &receipt,
                &permit,
                &receipt.receipt_id,
                &records,
                source,
                200,
                mock_trash,
            )
            .unwrap_err(),
            "eviction-journal-path-overlaps-data"
        );
        let staging_dir = source
            .parent()
            .unwrap()
            .join(format!(".disksage-evict-{}", receipt.receipt_id));
        assert_eq!(
            evict_source_with(
                &receipt,
                &permit,
                &receipt.receipt_id,
                &staging_dir,
                &temp.path().join("journal.jsonl"),
                200,
                mock_trash,
            )
            .unwrap_err(),
            "eviction-control-path-overlaps-staging"
        );
        assert!(source.exists());
    }

    #[test]
    fn interrupted_staging_resumes_and_missing_source_reconciles() {
        let temp = tempfile::tempdir().unwrap();
        let (receipt, permit) = valid_receipt(&temp);
        let records = temp.path().join("evictions");
        let journal = temp.path().join("journal/operations.jsonl");
        let first = evict_source_with(
            &receipt,
            &permit,
            &receipt.receipt_id,
            &records,
            &journal,
            200,
            |_, _, _, _| Err("simulated-crash-before-trash".into()),
        );
        assert_eq!(first.unwrap_err(), "simulated-crash-before-trash");
        assert!(!Path::new(&receipt.source).exists());
        let resumed = evict_source_with(
            &receipt,
            &permit,
            &receipt.receipt_id,
            &records,
            &journal,
            201,
            mock_trash,
        )
        .unwrap();
        assert!(resumed.source_trashed);

        let second_temp = tempfile::tempdir().unwrap();
        let (second_receipt, second_permit) = valid_receipt(&second_temp);
        let second_records = second_temp.path().join("evictions");
        let second_journal = second_temp.path().join("journal/operations.jsonl");
        let staged = evict_source_with(
            &second_receipt,
            &second_permit,
            &second_receipt.receipt_id,
            &second_records,
            &second_journal,
            300,
            |path, _, _, _| {
                std::fs::remove_dir_all(path).map_err(|error| error.to_string())?;
                Err("simulated-crash-after-trash".into())
            },
        );
        assert_eq!(staged.unwrap_err(), "simulated-crash-after-trash");
        let reconciled = evict_source_with(
            &second_receipt,
            &second_permit,
            &second_receipt.receipt_id,
            &second_records,
            &second_journal,
            301,
            mock_trash,
        )
        .unwrap();
        assert!(reconciled.reconciled_after_interruption);
        assert!(!reconciled.source_trashed);
    }
}
