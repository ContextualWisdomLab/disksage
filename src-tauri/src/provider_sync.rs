use crate::cloud::CloudProvider;
use crate::cloud_transfer::{
    CloudCopyReceipt, ProviderSyncEvidence, ProviderSyncState, RemoteChecksumAlgorithm,
    RemoteContentProof, SyncEvidenceKind,
};

#[cfg(test)]
use crate::cloud_transfer::LEGACY_RECEIPT_VERSION;

const ICLOUD_UPLOADED_KEY: &str = "NSURLUbiquitousItemIsUploadedKey";
const ICLOUD_UPLOADING_KEY: &str = "NSURLUbiquitousItemIsUploadingKey";
pub const PROVIDER_SYNC_OVERDUE_AFTER_MS: u64 = 24 * 60 * 60 * 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderSyncTimeliness {
    Complete,
    Pending,
    Overdue,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderSyncTimelinessAssessment {
    pub state: ProviderSyncTimeliness,
    pub pending_age_ms: u64,
    pub overdue_after_ms: u64,
    pub reason_codes: Vec<String>,
}

/// Classify how long provider-native confirmation has remained incomplete.
///
/// This is diagnostic only: `Overdue` never turns negative evidence into an eviction permit.
/// Receipt/evidence identity and time ordering are validated before the elapsed duration is used.
pub fn assess_provider_sync_timeliness(
    receipt: &CloudCopyReceipt,
    evidence: &ProviderSyncEvidence,
) -> Result<ProviderSyncTimelinessAssessment, String> {
    if evidence.receipt_id != receipt.receipt_id
        || evidence.provider != receipt.provider
        || evidence.destination != receipt.destination
        || evidence.observed_bytes != receipt.bytes
        || evidence.destination_blake3 != receipt.blake3
    {
        return Err("provider-sync-timeliness-evidence-mismatch".into());
    }
    if evidence.confirmed_at_ms < receipt.copied_at_ms {
        return Err("provider-sync-timeliness-time-order-invalid".into());
    }
    if evidence.sync_complete && evidence.sync_state.is_complete() {
        return Ok(ProviderSyncTimelinessAssessment {
            state: ProviderSyncTimeliness::Complete,
            pending_age_ms: 0,
            overdue_after_ms: PROVIDER_SYNC_OVERDUE_AFTER_MS,
            reason_codes: Vec::new(),
        });
    }

    let pending_age_ms = evidence.confirmed_at_ms - receipt.copied_at_ms;
    let (state, reason) = if pending_age_ms >= PROVIDER_SYNC_OVERDUE_AFTER_MS {
        (
            ProviderSyncTimeliness::Overdue,
            "provider-sync-confirmation-overdue",
        )
    } else {
        (
            ProviderSyncTimeliness::Pending,
            "provider-sync-confirmation-pending",
        )
    };
    Ok(ProviderSyncTimelinessAssessment {
        state,
        pending_age_ms,
        overdue_after_ms: PROVIDER_SYNC_OVERDUE_AFTER_MS,
        reason_codes: vec![reason.into()],
    })
}

/// Minimal, provider-native facts collected for one destination.
///
/// Keeping this value independent from Foundation makes the decision logic deterministic and
/// testable. The macOS adapter below is only responsible for collecting these facts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IcloudStatusSnapshot {
    pub is_ubiquitous: bool,
    pub is_uploaded: bool,
    pub is_uploading: bool,
    pub is_current: bool,
    pub observed_bytes: u64,
    pub destination_blake3: String,
}

fn icloud_sync_state(snapshot: &IcloudStatusSnapshot) -> ProviderSyncState {
    if !snapshot.is_ubiquitous {
        ProviderSyncState::NotUbiquitous
    } else if !snapshot.is_current {
        ProviderSyncState::NotLocalCurrent
    } else if snapshot.is_uploading {
        ProviderSyncState::Uploading
    } else if snapshot.is_uploaded {
        ProviderSyncState::Complete
    } else {
        ProviderSyncState::PendingUpload
    }
}

fn icloud_evidence_id(
    receipt: &CloudCopyReceipt,
    snapshot: &IcloudStatusSnapshot,
    confirmed_at_ms: u64,
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(receipt.receipt_id.as_bytes());
    hasher.update(&[0]);
    hasher.update(ICLOUD_UPLOADED_KEY.as_bytes());
    hasher.update(&[0]);
    hasher.update(ICLOUD_UPLOADING_KEY.as_bytes());
    hasher.update(&[
        snapshot.is_ubiquitous as u8,
        snapshot.is_uploaded as u8,
        snapshot.is_uploading as u8,
        snapshot.is_current as u8,
    ]);
    hasher.update(&snapshot.observed_bytes.to_le_bytes());
    hasher.update(snapshot.destination_blake3.as_bytes());
    hasher.update(&confirmed_at_ms.to_le_bytes());
    format!("foundation:{}", hasher.finalize().to_hex())
}

/// Convert an iCloud Foundation status snapshot into auditable sync evidence.
///
/// A negative status is still returned as evidence with `sync_complete = false`; the eviction
/// gate can then explain that the provider has not confirmed the upload. A non-iCloud receipt is
/// rejected instead of being relabelled as iCloud evidence.
pub fn evidence_from_icloud_snapshot(
    receipt: &CloudCopyReceipt,
    snapshot: &IcloudStatusSnapshot,
    confirmed_at_ms: u64,
) -> Result<ProviderSyncEvidence, String> {
    if receipt.provider != CloudProvider::Icloud {
        return Err("icloud-receipt-required".into());
    }
    if receipt.destination.trim().is_empty() {
        return Err("destination-missing".into());
    }
    let sync_complete = snapshot.is_ubiquitous
        && snapshot.is_uploaded
        && !snapshot.is_uploading
        && snapshot.is_current;
    Ok(ProviderSyncEvidence {
        receipt_id: receipt.receipt_id.clone(),
        provider: CloudProvider::Icloud,
        destination: receipt.destination.clone(),
        observed_bytes: snapshot.observed_bytes,
        destination_blake3: snapshot.destination_blake3.clone(),
        confirmed_at_ms,
        kind: SyncEvidenceKind::ProviderNativeStatus,
        evidence_id: icloud_evidence_id(receipt, snapshot, confirmed_at_ms),
        sync_complete,
        sync_state: icloud_sync_state(snapshot),
        remote_content: None,
    })
}

const FILE_PROVIDER_CTL_EVALUATE: &str = "fileproviderctl:evaluate";
const FILE_PROVIDER_ITEM_IDENTIFIER_MAX_BYTES: usize = 4 * 1024;
pub const FILE_PROVIDER_CAPABILITY_ALLOWS_EVICTING: u64 = 1 << 29;

/// Content and policy facts returned by one bounded `fileproviderctl evaluate` observation.
///
/// The raw provider item identifier is deliberately not retained. Its fingerprint is sufficient
/// to detect identity drift without putting an opaque provider identifier in reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileProviderItemStatus {
    pub is_downloaded: bool,
    pub is_downloading: bool,
    pub is_most_recent_version_downloaded: bool,
    pub is_uploaded: bool,
    pub is_uploading: bool,
    pub has_unresolved_conflicts: bool,
    pub is_excluded_from_sync: bool,
    pub is_sync_paused: bool,
    pub is_trashed: bool,
    pub capabilities: u64,
    pub allows_eviction: bool,
    pub observed_bytes: u64,
    pub item_identifier_fingerprint: String,
}

impl FileProviderItemStatus {
    pub fn is_local_current(&self) -> bool {
        self.is_downloaded && !self.is_downloading && self.is_most_recent_version_downloaded
    }

    pub fn is_sync_complete(&self) -> bool {
        self.is_local_current()
            && self.is_uploaded
            && !self.is_uploading
            && !self.has_unresolved_conflicts
            && !self.is_excluded_from_sync
            && !self.is_sync_paused
            && !self.is_trashed
    }
}

/// Provider-neutral facts exposed by macOS File Provider for third-party cloud roots.
///
/// Acquisition of the facts is platform-specific, while this value and its decision policy stay
/// deterministic and unit-testable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileProviderStatusSnapshot {
    pub item: FileProviderItemStatus,
    pub observed_bytes: u64,
    pub destination_blake3: String,
}

impl FileProviderStatusSnapshot {
    fn is_local_current(&self) -> bool {
        self.item.is_local_current()
    }

    fn is_sync_complete(&self) -> bool {
        self.item.is_sync_complete()
    }
}

/// Return the stable blocker used when a provider-native destination exists locally but has not
/// reached a complete remote-sync state. This is diagnostic only; it never authorizes eviction.
fn incomplete_sync_blocker(sync_complete: bool) -> Option<&'static str> {
    (!sync_complete).then_some("provider-sync-incomplete")
}

fn icloud_sync_blocker(snapshot: &IcloudStatusSnapshot) -> Option<&'static str> {
    incomplete_sync_blocker(
        snapshot.is_ubiquitous
            && snapshot.is_current
            && !snapshot.is_uploading
            && snapshot.is_uploaded,
    )
}

fn file_provider_sync_blocker(snapshot: &FileProviderItemStatus) -> Option<&'static str> {
    incomplete_sync_blocker(snapshot.is_sync_complete())
}

fn file_provider_sync_state(snapshot: &FileProviderStatusSnapshot) -> ProviderSyncState {
    if snapshot.item.is_excluded_from_sync {
        ProviderSyncState::ExcludedFromSync
    } else if snapshot.item.is_sync_paused {
        ProviderSyncState::SyncPaused
    } else if snapshot.item.is_trashed {
        ProviderSyncState::RemoteUnavailable
    } else if !snapshot.is_local_current() {
        ProviderSyncState::NotLocalCurrent
    } else if snapshot.item.has_unresolved_conflicts {
        ProviderSyncState::ContentMismatch
    } else if snapshot.item.is_uploading {
        ProviderSyncState::Uploading
    } else if snapshot.item.is_uploaded {
        ProviderSyncState::Complete
    } else {
        ProviderSyncState::PendingUpload
    }
}

fn file_provider_evidence_id(
    receipt: &CloudCopyReceipt,
    snapshot: &FileProviderStatusSnapshot,
    confirmed_at_ms: u64,
) -> String {
    let mut hasher = blake3::Hasher::new();
    for value in [
        receipt.receipt_id.as_str(),
        receipt.provider.as_str(),
        FILE_PROVIDER_CTL_EVALUATE,
        snapshot.destination_blake3.as_str(),
    ] {
        hasher.update(value.as_bytes());
        hasher.update(&[0]);
    }
    hasher.update(&[
        snapshot.item.is_downloaded as u8,
        snapshot.item.is_downloading as u8,
        snapshot.item.is_most_recent_version_downloaded as u8,
        snapshot.item.is_uploaded as u8,
        snapshot.item.is_uploading as u8,
        snapshot.item.has_unresolved_conflicts as u8,
        snapshot.item.is_excluded_from_sync as u8,
        snapshot.item.is_sync_paused as u8,
        snapshot.item.is_trashed as u8,
        snapshot.item.allows_eviction as u8,
    ]);
    hasher.update(&snapshot.item.capabilities.to_le_bytes());
    hasher.update(snapshot.item.item_identifier_fingerprint.as_bytes());
    hasher.update(&snapshot.observed_bytes.to_le_bytes());
    hasher.update(&confirmed_at_ms.to_le_bytes());
    format!("file-provider:{}", hasher.finalize().to_hex())
}

/// Convert third-party File Provider status into hash-bound native evidence.
pub fn evidence_from_file_provider_snapshot(
    receipt: &CloudCopyReceipt,
    snapshot: &FileProviderStatusSnapshot,
    confirmed_at_ms: u64,
) -> Result<ProviderSyncEvidence, String> {
    if !matches!(
        receipt.provider,
        CloudProvider::Onedrive | CloudProvider::GoogleDrive
    ) {
        return Err("third-party-file-provider-receipt-required".into());
    }
    if receipt.destination.trim().is_empty() {
        return Err("destination-missing".into());
    }
    Ok(ProviderSyncEvidence {
        receipt_id: receipt.receipt_id.clone(),
        provider: receipt.provider,
        destination: receipt.destination.clone(),
        observed_bytes: snapshot.observed_bytes,
        destination_blake3: snapshot.destination_blake3.clone(),
        confirmed_at_ms,
        kind: SyncEvidenceKind::ProviderNativeStatus,
        evidence_id: file_provider_evidence_id(receipt, snapshot, confirmed_at_ms),
        sync_complete: snapshot.is_sync_complete(),
        sync_state: file_provider_sync_state(snapshot),
        remote_content: None,
    })
}

fn file_provider_status_value<'a>(output: &'a str, key: &str) -> Result<&'a str, String> {
    let prefix = format!("{key} = ");
    let mut values = output
        .lines()
        .map(str::trim)
        .filter_map(|line| line.strip_prefix(&prefix));
    let value = values
        .next()
        .map(|value| value.trim().trim_end_matches(';'))
        .ok_or_else(|| format!("file-provider-status-field-missing:{key}"))?;
    if values.next().is_some() {
        return Err(format!("file-provider-status-field-duplicate:{key}"));
    }
    Ok(value)
}

fn file_provider_status_bool(output: &str, key: &str) -> Result<bool, String> {
    let value = file_provider_status_value(output, key)?;
    match value {
        "1" => Ok(true),
        "0" => Ok(false),
        _ => Err(format!("file-provider-status-field-invalid:{key}")),
    }
}

fn file_provider_status_u64(output: &str, key: &str) -> Result<u64, String> {
    file_provider_status_value(output, key)?
        .parse()
        .map_err(|_| format!("file-provider-status-field-invalid:{key}"))
}

fn file_provider_identifier_fingerprint(output: &str) -> Result<String, String> {
    let identifier = file_provider_status_value(output, "itemIdentifier")?.trim();
    if identifier.is_empty() || identifier.len() > FILE_PROVIDER_ITEM_IDENTIFIER_MAX_BYTES {
        return Err("file-provider-status-field-invalid:itemIdentifier".into());
    }
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-file-provider-item-identifier-v1\0");
    hasher.update(identifier.as_bytes());
    Ok(hasher.finalize().to_hex().to_string())
}

/// Parse the status needed for sync and local-cache decisions without retaining the raw item ID.
pub fn parse_file_providerctl_item_status(
    output: &str,
    observed_bytes: u64,
) -> Result<FileProviderItemStatus, String> {
    let provider_reported_bytes = file_provider_status_u64(output, "documentSize")?;
    if provider_reported_bytes != observed_bytes {
        return Err("file-provider-status-document-size-mismatch".into());
    }
    let capabilities = file_provider_status_u64(output, "capabilities")?;
    Ok(FileProviderItemStatus {
        is_downloaded: file_provider_status_bool(output, "isDownloaded")?,
        is_downloading: file_provider_status_bool(output, "isDownloading")?,
        is_most_recent_version_downloaded: file_provider_status_bool(
            output,
            "isMostRecentVersionDownloaded",
        )?,
        is_uploaded: file_provider_status_bool(output, "isUploaded")?,
        is_uploading: file_provider_status_bool(output, "isUploading")?,
        has_unresolved_conflicts: file_provider_status_bool(output, "hasUnresolvedConflicts")?,
        is_excluded_from_sync: file_provider_status_bool(output, "isExcludedFromSync")?,
        is_sync_paused: file_provider_status_bool(output, "isSyncPaused")?,
        is_trashed: file_provider_status_bool(output, "isTrashed")?,
        capabilities,
        allows_eviction: capabilities & FILE_PROVIDER_CAPABILITY_ALLOWS_EVICTING != 0,
        observed_bytes: provider_reported_bytes,
        item_identifier_fingerprint: file_provider_identifier_fingerprint(output)?,
    })
}

pub fn parse_file_providerctl_snapshot(
    output: &str,
    observed_bytes: u64,
    destination_blake3: &str,
) -> Result<FileProviderStatusSnapshot, String> {
    let item = parse_file_providerctl_item_status(output, observed_bytes)?;
    Ok(FileProviderStatusSnapshot {
        observed_bytes: item.observed_bytes,
        item,
        destination_blake3: destination_blake3.into(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderApiSnapshot {
    pub provider: CloudProvider,
    pub remote_object_id: String,
    pub remote_revision: String,
    pub remote_checksum: String,
    pub observed_bytes: u64,
    pub destination_blake3: String,
    pub available: bool,
    pub trashed: bool,
}

fn provider_api_evidence_id(
    receipt: &CloudCopyReceipt,
    snapshot: &ProviderApiSnapshot,
    algorithm: RemoteChecksumAlgorithm,
    location_proof: Option<&str>,
    confirmed_at_ms: u64,
) -> String {
    let mut hasher = blake3::Hasher::new();
    for value in [
        receipt.receipt_id.as_str(),
        snapshot.provider.as_str(),
        snapshot.remote_object_id.as_str(),
        snapshot.remote_revision.as_str(),
        snapshot.remote_checksum.as_str(),
        snapshot.destination_blake3.as_str(),
    ] {
        hasher.update(value.as_bytes());
        hasher.update(&[0]);
    }
    hasher.update(&snapshot.observed_bytes.to_le_bytes());
    hasher.update(&[snapshot.available as u8, snapshot.trashed as u8]);
    hasher.update(&[location_proof.is_some() as u8]);
    hasher.update(location_proof.unwrap_or_default().as_bytes());
    hasher.update(&[0]);
    hasher.update(&[match algorithm {
        RemoteChecksumAlgorithm::Sha256 => 1,
        RemoteChecksumAlgorithm::QuickXor => 2,
    }]);
    hasher.update(&confirmed_at_ms.to_le_bytes());
    format!("provider-api:{}", hasher.finalize().to_hex())
}

/// Convert authenticated remote metadata into provider API evidence.
///
/// Google Drive binary objects are bound by SHA-256. OneDrive objects are bound by QuickXorHash,
/// the checksum Microsoft guarantees for both personal and work/school drives. This function does
/// not perform OAuth or network I/O; adapters must populate the snapshot from the authenticated
/// provider response and re-hash the local destination immediately around that request.
pub fn evidence_from_provider_api_snapshot(
    receipt: &CloudCopyReceipt,
    snapshot: &ProviderApiSnapshot,
    confirmed_at_ms: u64,
) -> Result<ProviderSyncEvidence, String> {
    evidence_from_provider_api_snapshot_with_location(receipt, snapshot, None, confirmed_at_ms)
}

/// Convert provider metadata into content evidence and record whether the authenticated lookup was
/// addressed by the exact receipt-relative path. Object-ID-only evidence remains useful for audit,
/// but cannot authorize source eviction because equal content can exist elsewhere in the drive.
pub fn evidence_from_provider_api_snapshot_with_location(
    receipt: &CloudCopyReceipt,
    snapshot: &ProviderApiSnapshot,
    location_proof: Option<&str>,
    confirmed_at_ms: u64,
) -> Result<ProviderSyncEvidence, String> {
    if snapshot.provider != receipt.provider {
        return Err("provider-mismatch".into());
    }
    let (algorithm, expected_checksum, checksum_matches) = match snapshot.provider {
        CloudProvider::Icloud => return Err("icloud-native-status-required".into()),
        CloudProvider::Onedrive => (
            RemoteChecksumAlgorithm::QuickXor,
            receipt.quick_xor_base64.as_str(),
            snapshot.remote_checksum == receipt.quick_xor_base64.as_str(),
        ),
        CloudProvider::GoogleDrive => (
            RemoteChecksumAlgorithm::Sha256,
            receipt.sha256.as_str(),
            snapshot
                .remote_checksum
                .eq_ignore_ascii_case(&receipt.sha256),
        ),
    };
    let sync_complete = snapshot.available
        && !snapshot.trashed
        && !snapshot.remote_object_id.trim().is_empty()
        && !snapshot.remote_revision.trim().is_empty()
        && !expected_checksum.is_empty()
        && checksum_matches
        && snapshot.observed_bytes == receipt.bytes
        && snapshot.destination_blake3 == receipt.blake3;
    Ok(ProviderSyncEvidence {
        receipt_id: receipt.receipt_id.clone(),
        provider: receipt.provider,
        destination: receipt.destination.clone(),
        observed_bytes: snapshot.observed_bytes,
        destination_blake3: snapshot.destination_blake3.clone(),
        confirmed_at_ms,
        kind: SyncEvidenceKind::ProviderApi,
        evidence_id: provider_api_evidence_id(
            receipt,
            snapshot,
            algorithm,
            location_proof,
            confirmed_at_ms,
        ),
        sync_complete,
        sync_state: if sync_complete {
            ProviderSyncState::Complete
        } else if !snapshot.available || snapshot.trashed {
            ProviderSyncState::RemoteUnavailable
        } else {
            ProviderSyncState::ContentMismatch
        },
        remote_content: Some(RemoteContentProof {
            object_id: snapshot.remote_object_id.clone(),
            revision: snapshot.remote_revision.clone(),
            algorithm,
            checksum: snapshot.remote_checksum.clone(),
            location_bound: location_proof.is_some(),
            location_proof: location_proof.map(str::to_owned),
        }),
    })
}

#[derive(serde::Deserialize)]
struct OneDriveHashes {
    #[serde(rename = "quickXorHash")]
    quick_xor_hash: Option<String>,
}

#[derive(serde::Deserialize)]
struct OneDriveFileFacet {
    hashes: Option<OneDriveHashes>,
}

#[derive(serde::Deserialize)]
struct OneDriveItemResponse {
    id: Option<String>,
    size: Option<u64>,
    #[serde(rename = "eTag")]
    e_tag: Option<String>,
    file: Option<OneDriveFileFacet>,
    deleted: Option<serde_json::Value>,
}

/// Parse the bounded fields requested from a Microsoft Graph driveItem response.
pub fn parse_onedrive_item_snapshot(
    json: &str,
    destination_blake3: &str,
) -> Result<ProviderApiSnapshot, String> {
    let item: OneDriveItemResponse =
        serde_json::from_str(json).map_err(|_| "onedrive-response-invalid".to_string())?;
    let hashes = item
        .file
        .and_then(|file| file.hashes)
        .ok_or_else(|| "onedrive-file-hashes-missing".to_string())?;
    Ok(ProviderApiSnapshot {
        provider: CloudProvider::Onedrive,
        remote_object_id: item.id.unwrap_or_default(),
        remote_revision: item.e_tag.unwrap_or_default(),
        remote_checksum: hashes.quick_xor_hash.unwrap_or_default(),
        observed_bytes: item.size.unwrap_or_default(),
        destination_blake3: destination_blake3.into(),
        available: true,
        trashed: item.deleted.is_some(),
    })
}

#[derive(serde::Deserialize)]
struct GoogleDriveFileResponse {
    id: Option<String>,
    version: Option<String>,
    size: Option<String>,
    #[serde(rename = "sha256Checksum")]
    sha256_checksum: Option<String>,
    trashed: Option<bool>,
}

/// Parse the bounded fields requested from a Google Drive v3 files.get response.
pub fn parse_google_drive_file_snapshot(
    json: &str,
    destination_blake3: &str,
) -> Result<ProviderApiSnapshot, String> {
    let file: GoogleDriveFileResponse =
        serde_json::from_str(json).map_err(|_| "google-drive-response-invalid".to_string())?;
    let observed_bytes = file
        .size
        .as_deref()
        .ok_or_else(|| "google-drive-size-missing".to_string())?
        .parse::<u64>()
        .map_err(|_| "google-drive-size-invalid".to_string())?;
    Ok(ProviderApiSnapshot {
        provider: CloudProvider::GoogleDrive,
        remote_object_id: file.id.unwrap_or_default(),
        remote_revision: file.version.unwrap_or_default(),
        remote_checksum: file.sha256_checksum.unwrap_or_default(),
        observed_bytes,
        destination_blake3: destination_blake3.into(),
        available: true,
        trashed: file.trashed.unwrap_or(false),
    })
}

#[cfg(all(target_os = "macos", not(coverage)))]
fn foundation_bool_resource(
    url: &objc2_foundation::NSURL,
    key: &objc2_foundation::NSURLResourceKey,
) -> Result<bool, String> {
    use objc2::runtime::AnyObject;
    use objc2_foundation::NSNumber;

    let mut value: Option<objc2::rc::Retained<AnyObject>> = None;
    // SAFETY: Foundation defines both queried resource keys as NSNumber-valued NSURL keys. The
    // returned Objective-C object is retained by objc2 and downcast-checked before use.
    unsafe { url.getResourceValue_forKey_error(&mut value, key) }
        .map_err(|error| error.localizedDescription().to_string())?;
    let value = value.ok_or_else(|| "icloud-resource-value-missing".to_string())?;
    value
        .downcast::<NSNumber>()
        .map(|number| number.as_bool())
        .map_err(|_| "icloud-resource-value-not-boolean".to_string())
}

#[cfg(all(target_os = "macos", not(coverage)))]
fn foundation_string_resource(
    url: &objc2_foundation::NSURL,
    key: &objc2_foundation::NSURLResourceKey,
) -> Result<objc2::rc::Retained<objc2_foundation::NSString>, String> {
    use objc2::runtime::AnyObject;
    use objc2_foundation::NSString;

    let mut value: Option<objc2::rc::Retained<AnyObject>> = None;
    // SAFETY: Foundation defines the downloading-status resource key as NSString-valued. The
    // returned Objective-C object is retained and downcast-checked before use.
    unsafe { url.getResourceValue_forKey_error(&mut value, key) }
        .map_err(|error| error.localizedDescription().to_string())?;
    value
        .ok_or_else(|| "icloud-resource-value-missing".to_string())?
        .downcast::<NSString>()
        .map_err(|_| "icloud-resource-value-not-string".to_string())
}

#[cfg(all(target_os = "macos", not(coverage)))]
fn foundation_icloud_status(path: &str) -> Result<(bool, bool, bool, bool), String> {
    use objc2::rc::autoreleasepool;
    use objc2_foundation::{
        NSString, NSURLIsUbiquitousItemKey, NSURLUbiquitousItemDownloadingStatusCurrent,
        NSURLUbiquitousItemDownloadingStatusKey, NSURLUbiquitousItemIsUploadedKey,
        NSURLUbiquitousItemIsUploadingKey, NSURL,
    };

    autoreleasepool(|_| {
        let url = NSURL::fileURLWithPath(&NSString::from_str(path));
        // SAFETY: These are Foundation-exported, process-lifetime NSURL resource-key and value
        // constants with the types declared by objc2-foundation.
        unsafe {
            let is_ubiquitous = foundation_bool_resource(&url, NSURLIsUbiquitousItemKey)?;
            if !is_ubiquitous {
                return Ok((false, false, false, false));
            }
            let downloading_status =
                foundation_string_resource(&url, NSURLUbiquitousItemDownloadingStatusKey)?;
            Ok((
                is_ubiquitous,
                foundation_bool_resource(&url, NSURLUbiquitousItemIsUploadedKey)?,
                foundation_bool_resource(&url, NSURLUbiquitousItemIsUploadingKey)?,
                downloading_status.isEqualToString(NSURLUbiquitousItemDownloadingStatusCurrent),
            ))
        }
    })
}

#[cfg(all(target_os = "macos", not(coverage)))]
fn hash_file(path: &std::path::Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let mut hasher = blake3::Hasher::new();
    std::io::copy(&mut file, &mut hasher).map_err(|error| error.to_string())?;
    Ok(hasher.finalize().to_hex().to_string())
}

#[cfg(all(target_os = "macos", not(coverage)))]
pub(crate) fn file_providerctl_status(path: &str) -> Result<String, String> {
    use std::io::Read;
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    const TIMEOUT: Duration = Duration::from_secs(5);
    const OUTPUT_LIMIT: u64 = 256 * 1_024;

    let mut command = Command::new("/usr/bin/fileproviderctl");
    command
        .arg("evaluate")
        .arg(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    // File Provider helpers can retain inherited stdout after the leader exits. Keep the
    // helper in a private process group so bounded cleanup can always join the reader.
    unsafe {
        command.pre_exec(|| {
            if libc::setpgid(0, 0) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = command
        .spawn()
        .map_err(|_| "file-provider-status-command-unavailable".to_string())?;
    let child_pid = child.id();
    let kill_group = || unsafe {
        let _ = libc::kill(-(child_pid as libc::pid_t), libc::SIGKILL);
    };
    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            kill_group();
            let _ = child.kill();
            let _ = child.wait();
            return Err("file-provider-status-output-missing".into());
        }
    };
    let output_reader = std::thread::spawn(move || {
        let mut output = Vec::new();
        stdout
            .take(OUTPUT_LIMIT + 1)
            .read_to_end(&mut output)
            .map(|_| output)
    });
    let deadline = Instant::now() + TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(25));
            }
            Ok(None) => {
                kill_group();
                let _ = child.kill();
                let _ = child.wait();
                let _ = output_reader.join();
                return Err("file-provider-status-command-timeout".into());
            }
            Err(_) => {
                kill_group();
                let _ = child.kill();
                let _ = child.wait();
                let _ = output_reader.join();
                return Err("file-provider-status-command-wait-failed".into());
            }
        }
    };
    // The leader may exit while a descendant still owns the pipe.
    kill_group();
    let output = output_reader
        .join()
        .map_err(|_| "file-provider-status-output-reader-panicked".to_string())?
        .map_err(|_| "file-provider-status-output-read-failed".to_string())?;
    if output.len() as u64 > OUTPUT_LIMIT {
        return Err("file-provider-status-output-too-large".into());
    }
    if !status.success() {
        return Err("file-provider-status-command-failed".into());
    }
    String::from_utf8(output).map_err(|_| "file-provider-status-output-not-utf8".into())
}

/// Inspect an already-existing destination during planning without retaining provider paths or
/// identifiers. A failed probe deliberately falls back to the ordinary collision blocker.
#[cfg(all(target_os = "macos", not(coverage)))]
pub fn existing_destination_sync_blocker(
    provider: CloudProvider,
    destination: &std::path::Path,
    expected_bytes: u64,
) -> Option<&'static str> {
    let metadata = std::fs::symlink_metadata(destination).ok()?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() != expected_bytes
    {
        return None;
    }
    let path = destination.to_str()?;
    match provider {
        CloudProvider::Icloud => {
            let (is_ubiquitous, is_uploaded, is_uploading, is_current) =
                foundation_icloud_status(path).ok()?;
            icloud_sync_blocker(&IcloudStatusSnapshot {
                is_ubiquitous,
                is_uploaded,
                is_uploading,
                is_current,
                observed_bytes: expected_bytes,
                destination_blake3: String::new(),
            })
        }
        CloudProvider::Onedrive | CloudProvider::GoogleDrive => {
            let output = file_providerctl_status(path).ok()?;
            let status = parse_file_providerctl_item_status(&output, expected_bytes).ok()?;
            file_provider_sync_blocker(&status)
        }
    }
}

/// Prove that an existing File Provider destination is already materialized before any hash read.
/// This gate is separate from sync attestation so adoption cannot hydrate a dataless placeholder.
#[cfg(all(target_os = "macos", not(coverage)))]
pub fn require_existing_destination_local_current(
    provider: CloudProvider,
    destination: &std::path::Path,
    expected_bytes: u64,
) -> Result<(), String> {
    use std::os::unix::fs::MetadataExt;

    #[cfg(test)]
    let _ = provider;

    let metadata = std::fs::symlink_metadata(destination)
        .map_err(|_| "existing-destination-status-unavailable".to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("existing-destination-must-be-regular-file".into());
    }
    if metadata.len() != expected_bytes || crate::cloud::metadata_is_dataless(&metadata) {
        return Err("existing-destination-not-materialized".into());
    }
    #[cfg(not(test))]
    let path = destination
        .to_str()
        .ok_or_else(|| "existing-destination-not-unicode".to_string())?;
    #[cfg(not(test))]
    let check_provider_status = |expected_size: u64| -> Result<(), String> {
        match provider {
            CloudProvider::Icloud => {
                let (ubiquitous, _, uploading, current) = foundation_icloud_status(path)?;
                if !ubiquitous || !current || uploading {
                    return Err("existing-destination-not-local-current".into());
                }
            }
            CloudProvider::Onedrive | CloudProvider::GoogleDrive => {
                let snapshot = parse_file_providerctl_snapshot(
                    &file_providerctl_status(path)?,
                    expected_size,
                    "hash-pending",
                )?;
                if !snapshot.is_local_current() {
                    return Err("existing-destination-not-local-current".into());
                }
            }
        }
        Ok(())
    };
    // Production adoption always requires the provider-native status adapter. Unit tests use
    // ordinary temporary files, so they retain deterministic metadata/identity coverage without
    // invoking unavailable Foundation or File Provider CLIs.
    #[cfg(not(test))]
    check_provider_status(metadata.len())?;
    let after = std::fs::symlink_metadata(destination)
        .map_err(|_| "existing-destination-status-unavailable".to_string())?;
    if after.file_type().is_symlink()
        || !after.is_file()
        || crate::cloud::metadata_is_dataless(&after)
        || after.len() != metadata.len()
        || after.dev() != metadata.dev()
        || after.ino() != metadata.ino()
        || after.modified().ok() != metadata.modified().ok()
    {
        return Err("existing-destination-status-changed".into());
    }
    #[cfg(not(test))]
    check_provider_status(after.len())?;
    Ok(())
}

#[cfg(any(not(target_os = "macos"), coverage))]
pub fn require_existing_destination_local_current(
    _provider: CloudProvider,
    destination: &std::path::Path,
    expected_bytes: u64,
) -> Result<(), String> {
    #[cfg(windows)]
    {
        // Windows Files On-Demand placeholders can report the logical file length while their
        // bytes are not local. No provider-native local-current adapter exists on this target;
        // reject adoption rather than letting metadata-only checks hydrate a placeholder.
        let _ = (destination, expected_bytes);
        return Err("existing-destination-provider-status-unavailable".into());
    }
    let metadata = std::fs::symlink_metadata(destination)
        .map_err(|_| "existing-destination-status-unavailable".to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("existing-destination-must-be-regular-file".into());
    }
    if metadata.len() != expected_bytes || crate::cloud::metadata_is_dataless(&metadata) {
        return Err("existing-destination-not-materialized".into());
    }
    Ok(())
}

#[cfg(any(not(target_os = "macos"), coverage))]
pub fn existing_destination_sync_blocker(
    _provider: CloudProvider,
    _destination: &std::path::Path,
    _expected_bytes: u64,
) -> Option<&'static str> {
    None
}

/// Read macOS File Provider status for a OneDrive or Google Drive destination and bind it to the
/// verified local copy. This never hydrates, evicts, uploads, or mutates the file.
#[cfg(all(target_os = "macos", not(coverage)))]
pub fn collect_file_provider_sync_evidence(
    receipt: &CloudCopyReceipt,
    confirmed_at_ms: u64,
) -> Result<ProviderSyncEvidence, String> {
    use std::os::unix::fs::MetadataExt;
    use std::path::Path;

    if !matches!(
        receipt.provider,
        CloudProvider::Onedrive | CloudProvider::GoogleDrive
    ) {
        return Err("third-party-file-provider-receipt-required".into());
    }
    let destination = Path::new(&receipt.destination);
    let metadata = std::fs::symlink_metadata(destination).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("file-provider-destination-must-be-regular-file".into());
    }
    let before_modified = metadata.modified().map_err(|error| error.to_string())?;
    let path = destination
        .to_str()
        .ok_or_else(|| "file-provider-destination-not-unicode".to_string())?;
    let before = parse_file_providerctl_snapshot(
        &file_providerctl_status(path)?,
        metadata.len(),
        "hash-pending",
    )?;
    if !before.is_local_current() {
        return Err("file-provider-destination-not-local-current".into());
    }

    // Hash only after File Provider says the latest version is already local, avoiding hydration.
    let destination_hash = hash_file(destination)?;
    if metadata.len() != receipt.bytes || destination_hash != receipt.blake3 {
        return Err("file-provider-destination-content-mismatch".into());
    }
    let after_status = file_providerctl_status(path)?;
    let after = std::fs::symlink_metadata(destination).map_err(|error| error.to_string())?;
    if after.file_type().is_symlink()
        || !after.is_file()
        || after.len() != metadata.len()
        || after.dev() != metadata.dev()
        || after.ino() != metadata.ino()
        || after.modified().map_err(|error| error.to_string())? != before_modified
    {
        return Err("file-provider-destination-changed-during-status-check".into());
    }
    let snapshot = parse_file_providerctl_snapshot(&after_status, after.len(), &destination_hash)?;
    if !snapshot.is_local_current() {
        return Err("file-provider-destination-status-changed-during-check".into());
    }
    evidence_from_file_provider_snapshot(receipt, &snapshot, confirmed_at_ms)
}

#[cfg(any(not(target_os = "macos"), coverage))]
pub fn collect_file_provider_sync_evidence(
    _receipt: &CloudCopyReceipt,
    _confirmed_at_ms: u64,
) -> Result<ProviderSyncEvidence, String> {
    Err("file-provider-native-status-unsupported-platform".into())
}

/// Read Apple's per-file ubiquitous-item flags and produce provider-native evidence.
///
/// This function is read-only. It does not start a download, evict a local file, or mutate the
/// receipt. The caller must still pass the result through `approve_local_eviction`.
#[cfg(all(target_os = "macos", not(coverage)))]
pub fn collect_icloud_sync_evidence(
    receipt: &CloudCopyReceipt,
    confirmed_at_ms: u64,
) -> Result<ProviderSyncEvidence, String> {
    use std::os::unix::fs::MetadataExt;
    use std::path::Path;

    if receipt.provider != CloudProvider::Icloud {
        return Err("icloud-receipt-required".into());
    }
    let destination = Path::new(&receipt.destination);
    let metadata = std::fs::symlink_metadata(destination).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("icloud-destination-must-be-regular-file".into());
    }
    let before_modified = metadata.modified().map_err(|error| error.to_string())?;
    let path = destination
        .to_str()
        .ok_or_else(|| "icloud-destination-not-unicode".to_string())?;
    let (before_ubiquitous, _, _, before_current) = foundation_icloud_status(path)?;
    if !before_ubiquitous {
        return Err("icloud-destination-not-ubiquitous".into());
    }
    if !before_current {
        return Err("icloud-destination-not-local-current".into());
    }

    // Reading an evicted placeholder could trigger hydration. The `Current` gate above ensures
    // this hash only reads bytes that Foundation already reports as locally current.
    let destination_hash = hash_file(destination)?;
    if metadata.len() != receipt.bytes || destination_hash != receipt.blake3 {
        return Err("icloud-destination-content-mismatch".into());
    }
    let (is_ubiquitous, is_uploaded, is_uploading, is_current) = foundation_icloud_status(path)?;
    if !is_ubiquitous || !is_current {
        return Err("icloud-destination-status-changed-during-check".into());
    }
    let after = std::fs::symlink_metadata(destination).map_err(|error| error.to_string())?;
    if after.file_type().is_symlink()
        || !after.is_file()
        || after.len() != metadata.len()
        || after.dev() != metadata.dev()
        || after.ino() != metadata.ino()
        || after.modified().map_err(|error| error.to_string())? != before_modified
    {
        return Err("icloud-destination-changed-during-status-check".into());
    }
    let snapshot = IcloudStatusSnapshot {
        is_ubiquitous,
        is_uploaded,
        is_uploading,
        is_current,
        observed_bytes: after.len(),
        destination_blake3: destination_hash,
    };

    evidence_from_icloud_snapshot(receipt, &snapshot, confirmed_at_ms)
}

#[cfg(any(not(target_os = "macos"), coverage))]
pub fn collect_icloud_sync_evidence(
    _receipt: &CloudCopyReceipt,
    _confirmed_at_ms: u64,
) -> Result<ProviderSyncEvidence, String> {
    Err("icloud-native-status-unsupported-platform".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn receipt(provider: CloudProvider) -> CloudCopyReceipt {
        CloudCopyReceipt {
            version: LEGACY_RECEIPT_VERSION,
            receipt_id: "receipt-id".into(),
            candidate_fingerprint: "metadata-fingerprint".into(),
            provider,
            source: "/source/report.pdf".into(),
            destination: "/cloud/report.pdf".into(),
            bytes: 42,
            blake3: "content-hash".into(),
            sha256: "sha256-hash".into(),
            quick_xor_base64: "quick-xor".into(),
            source_modified_ms: 10,
            copied_at_ms: 20,
            copy_verified: true,
            provider_sync_confirmed: false,
            lineage_fingerprint: None,
            lineage: None,
        }
    }

    #[test]
    fn uploaded_ubiquitous_item_becomes_complete_native_evidence() {
        let receipt = receipt(CloudProvider::Icloud);
        let snapshot = IcloudStatusSnapshot {
            is_ubiquitous: true,
            is_uploaded: true,
            is_uploading: false,
            is_current: true,
            observed_bytes: 42,
            destination_blake3: "content-hash".into(),
        };
        let evidence = evidence_from_icloud_snapshot(&receipt, &snapshot, 30).unwrap();
        assert!(evidence.sync_complete);
        assert_eq!(evidence.receipt_id, "receipt-id");
        assert_eq!(evidence.provider, CloudProvider::Icloud);
        assert_eq!(evidence.destination, "/cloud/report.pdf");
        assert_eq!(evidence.observed_bytes, 42);
        assert_eq!(evidence.destination_blake3, "content-hash");
        assert_eq!(evidence.confirmed_at_ms, 30);
        assert_eq!(evidence.kind, SyncEvidenceKind::ProviderNativeStatus);
        assert!(evidence.evidence_id.starts_with("foundation:"));
        assert_eq!(evidence.evidence_id.len(), 75);
        assert_eq!(evidence.sync_state, ProviderSyncState::Complete);
        assert_eq!(evidence.remote_content, None);
    }

    #[test]
    fn local_current_but_not_uploaded_is_pending_upload() {
        let receipt = receipt(CloudProvider::Icloud);
        let evidence = evidence_from_icloud_snapshot(
            &receipt,
            &IcloudStatusSnapshot {
                is_ubiquitous: true,
                is_uploaded: false,
                is_uploading: false,
                is_current: true,
                observed_bytes: 42,
                destination_blake3: "content-hash".into(),
            },
            30,
        )
        .unwrap();
        assert_eq!(evidence.sync_state, ProviderSyncState::PendingUpload);
        assert!(!evidence.sync_complete);

        let mut record_evidence = evidence.clone();
        record_evidence.receipt_id = "a".repeat(64);
        record_evidence.destination_blake3 = "b".repeat(64);
        let record =
            crate::provider_evidence::create_sync_evidence_record(&record_evidence).unwrap();
        let blockers = crate::cloud_transfer::approve_local_eviction(&receipt, &record)
            .expect_err("pending iCloud upload must not issue an eviction permit");
        assert!(blockers.contains(&"provider-sync-incomplete".to_string()));
    }

    #[test]
    fn planner_marks_local_current_but_not_uploaded_as_incomplete() {
        let snapshot = IcloudStatusSnapshot {
            is_ubiquitous: true,
            is_uploaded: false,
            is_uploading: false,
            is_current: true,
            observed_bytes: 42,
            destination_blake3: String::new(),
        };
        assert_eq!(
            icloud_sync_blocker(&snapshot),
            Some("provider-sync-incomplete")
        );

        let uploaded = IcloudStatusSnapshot {
            is_uploaded: true,
            ..snapshot
        };
        assert_eq!(icloud_sync_blocker(&uploaded), None);
    }

    #[test]
    fn timeliness_distinguishes_complete_pending_and_overdue_without_approving() {
        let receipt = receipt(CloudProvider::Icloud);
        let mut snapshot = IcloudStatusSnapshot {
            is_ubiquitous: true,
            is_uploaded: false,
            is_uploading: true,
            is_current: true,
            observed_bytes: 42,
            destination_blake3: "content-hash".into(),
        };
        let pending = evidence_from_icloud_snapshot(
            &receipt,
            &snapshot,
            receipt.copied_at_ms + PROVIDER_SYNC_OVERDUE_AFTER_MS - 1,
        )
        .unwrap();
        let pending_assessment = assess_provider_sync_timeliness(&receipt, &pending).unwrap();
        assert_eq!(pending_assessment.state, ProviderSyncTimeliness::Pending);
        assert_eq!(
            pending_assessment.reason_codes,
            ["provider-sync-confirmation-pending"]
        );
        assert!(!pending.sync_complete);
        assert_eq!(pending.sync_state, ProviderSyncState::Uploading);

        let overdue = evidence_from_icloud_snapshot(
            &receipt,
            &snapshot,
            receipt.copied_at_ms + PROVIDER_SYNC_OVERDUE_AFTER_MS,
        )
        .unwrap();
        let overdue_assessment = assess_provider_sync_timeliness(&receipt, &overdue).unwrap();
        assert_eq!(overdue_assessment.state, ProviderSyncTimeliness::Overdue);
        assert_eq!(
            overdue_assessment.pending_age_ms,
            PROVIDER_SYNC_OVERDUE_AFTER_MS
        );
        assert_eq!(
            overdue_assessment.reason_codes,
            ["provider-sync-confirmation-overdue"]
        );
        assert!(!overdue.sync_complete);

        snapshot.is_uploaded = true;
        snapshot.is_uploading = false;
        let complete = evidence_from_icloud_snapshot(
            &receipt,
            &snapshot,
            receipt.copied_at_ms + PROVIDER_SYNC_OVERDUE_AFTER_MS * 2,
        )
        .unwrap();
        let complete_assessment = assess_provider_sync_timeliness(&receipt, &complete).unwrap();
        assert_eq!(complete_assessment.state, ProviderSyncTimeliness::Complete);
        assert_eq!(complete_assessment.pending_age_ms, 0);
        assert!(complete_assessment.reason_codes.is_empty());
    }

    #[test]
    fn legacy_unknown_sync_state_is_not_timely_complete() {
        let receipt = receipt(CloudProvider::GoogleDrive);
        let evidence = ProviderSyncEvidence {
            receipt_id: receipt.receipt_id.clone(),
            provider: receipt.provider,
            destination: receipt.destination.clone(),
            observed_bytes: receipt.bytes,
            destination_blake3: receipt.blake3.clone(),
            confirmed_at_ms: receipt.copied_at_ms + 1,
            kind: SyncEvidenceKind::ProviderNativeStatus,
            evidence_id: "legacy-unknown".into(),
            sync_complete: true,
            sync_state: ProviderSyncState::Unknown,
            remote_content: None,
        };
        let assessment = assess_provider_sync_timeliness(&receipt, &evidence).unwrap();
        assert_eq!(assessment.state, ProviderSyncTimeliness::Pending);
        assert_eq!(
            assessment.reason_codes,
            ["provider-sync-confirmation-pending"]
        );
    }

    #[test]
    fn timeliness_rejects_mismatched_or_time_reversed_evidence() {
        let receipt = receipt(CloudProvider::Icloud);
        let snapshot = IcloudStatusSnapshot {
            is_ubiquitous: true,
            is_uploaded: false,
            is_uploading: true,
            is_current: true,
            observed_bytes: 42,
            destination_blake3: "content-hash".into(),
        };
        let mut evidence = evidence_from_icloud_snapshot(&receipt, &snapshot, 30).unwrap();
        evidence.receipt_id = "different".into();
        assert_eq!(
            assess_provider_sync_timeliness(&receipt, &evidence).unwrap_err(),
            "provider-sync-timeliness-evidence-mismatch"
        );

        let mut evidence = evidence_from_icloud_snapshot(&receipt, &snapshot, 30).unwrap();
        evidence.observed_bytes += 1;
        assert_eq!(
            assess_provider_sync_timeliness(&receipt, &evidence).unwrap_err(),
            "provider-sync-timeliness-evidence-mismatch"
        );

        let evidence = evidence_from_icloud_snapshot(&receipt, &snapshot, 19).unwrap();
        assert_eq!(
            assess_provider_sync_timeliness(&receipt, &evidence).unwrap_err(),
            "provider-sync-timeliness-time-order-invalid"
        );
    }

    fn uploaded_file_provider_output() -> &'static str {
        r#"
            capabilities = 805306495;
            documentSize = 42;
            hasUnresolvedConflicts = 0;
            isDownloaded = 1;
            isDownloading = 0;
            isMostRecentVersionDownloaded = 1;
            isUploaded = 1;
            isUploading = 0;
            isExcludedFromSync = 0;
            isSyncPaused = 0;
            isTrashed = 0;
            itemIdentifier = opaque-provider-item;
        "#
    }

    #[test]
    fn third_party_file_provider_status_becomes_complete_native_evidence() {
        let snapshot =
            parse_file_providerctl_snapshot(uploaded_file_provider_output(), 42, "content-hash")
                .unwrap();
        assert!(snapshot.is_local_current());
        assert!(snapshot.is_sync_complete());
        assert!(snapshot.item.allows_eviction);
        assert_eq!(snapshot.item.item_identifier_fingerprint.len(), 64);

        for provider in [CloudProvider::Onedrive, CloudProvider::GoogleDrive] {
            let evidence =
                evidence_from_file_provider_snapshot(&receipt(provider), &snapshot, 30).unwrap();
            assert!(evidence.sync_complete);
            assert_eq!(evidence.provider, provider);
            assert_eq!(evidence.kind, SyncEvidenceKind::ProviderNativeStatus);
            assert!(evidence.evidence_id.starts_with("file-provider:"));
            assert_eq!(evidence.remote_content, None);
        }
        assert_eq!(
            evidence_from_file_provider_snapshot(&receipt(CloudProvider::Icloud), &snapshot, 30,)
                .unwrap_err(),
            "third-party-file-provider-receipt-required"
        );
    }

    #[test]
    fn planner_marks_pending_file_provider_item_as_incomplete() {
        let output = uploaded_file_provider_output().replace("isUploaded = 1", "isUploaded = 0");
        let snapshot = parse_file_providerctl_snapshot(&output, 42, "content-hash").unwrap();
        assert_eq!(
            file_provider_sync_blocker(&snapshot.item),
            Some("provider-sync-incomplete")
        );
    }

    #[test]
    fn trashed_file_provider_item_remains_incomplete() {
        let output = uploaded_file_provider_output().replace("isTrashed = 0", "isTrashed = 1");
        let snapshot = parse_file_providerctl_snapshot(&output, 42, "content-hash").unwrap();
        let evidence =
            evidence_from_file_provider_snapshot(&receipt(CloudProvider::Onedrive), &snapshot, 30)
                .unwrap();
        assert!(!snapshot.is_sync_complete());
        assert!(!evidence.sync_complete);
        assert_eq!(evidence.sync_state, ProviderSyncState::RemoteUnavailable);
    }

    #[test]
    fn file_provider_status_binds_exactly_one_provider_reported_size() {
        let duplicate = uploaded_file_provider_output().replace(
            "documentSize = 42;",
            "documentSize = 42;\n            documentSize = 42;",
        );
        assert_eq!(
            parse_file_providerctl_snapshot(&duplicate, 42, "content-hash").unwrap_err(),
            "file-provider-status-field-duplicate:documentSize"
        );

        let missing = uploaded_file_provider_output().replace("documentSize = 42;", "");
        assert_eq!(
            parse_file_providerctl_snapshot(&missing, 42, "content-hash").unwrap_err(),
            "file-provider-status-field-missing:documentSize"
        );

        let changed =
            uploaded_file_provider_output().replace("documentSize = 42", "documentSize = 41");
        assert_eq!(
            parse_file_providerctl_snapshot(&changed, 42, "content-hash").unwrap_err(),
            "file-provider-status-document-size-mismatch"
        );
    }

    #[test]
    fn file_provider_status_rejects_duplicate_decision_flags() {
        let duplicate = uploaded_file_provider_output().replace(
            "isUploaded = 1;",
            "isUploaded = 1;\n            isUploaded = 0;",
        );
        assert_eq!(
            parse_file_providerctl_snapshot(&duplicate, 42, "content-hash").unwrap_err(),
            "file-provider-status-field-duplicate:isUploaded"
        );
    }

    #[test]
    fn file_provider_item_identity_and_capabilities_are_strict_and_redacted() {
        let status =
            parse_file_providerctl_item_status(uploaded_file_provider_output(), 42).unwrap();
        assert_eq!(status.capabilities, 805_306_495);
        assert!(status.allows_eviction);
        assert!(!status
            .item_identifier_fingerprint
            .contains("opaque-provider-item"));

        let onedrive = uploaded_file_provider_output()
            .replace("capabilities = 805306495", "capabilities = 536870975");
        assert!(
            parse_file_providerctl_item_status(&onedrive, 42)
                .unwrap()
                .allows_eviction
        );

        let no_eviction =
            uploaded_file_provider_output().replace("capabilities = 805306495", "capabilities = 0");
        assert!(
            !parse_file_providerctl_item_status(&no_eviction, 42)
                .unwrap()
                .allows_eviction
        );

        for invalid in [
            uploaded_file_provider_output().replace("itemIdentifier = opaque-provider-item;", ""),
            uploaded_file_provider_output().replace(
                "itemIdentifier = opaque-provider-item;",
                "itemIdentifier = first;\n            itemIdentifier = second;",
            ),
        ] {
            assert!(parse_file_providerctl_item_status(&invalid, 42).is_err());
        }
    }

    #[test]
    fn file_provider_status_fails_closed_on_upload_locality_or_policy_flags() {
        for (field, replacement) in [
            ("isDownloaded = 1", "isDownloaded = 0"),
            ("isDownloading = 0", "isDownloading = 1"),
            (
                "isMostRecentVersionDownloaded = 1",
                "isMostRecentVersionDownloaded = 0",
            ),
            ("isUploaded = 1", "isUploaded = 0"),
            ("isUploading = 0", "isUploading = 1"),
            ("hasUnresolvedConflicts = 0", "hasUnresolvedConflicts = 1"),
            ("isExcludedFromSync = 0", "isExcludedFromSync = 1"),
            ("isSyncPaused = 0", "isSyncPaused = 1"),
            ("isTrashed = 0", "isTrashed = 1"),
        ] {
            let output = uploaded_file_provider_output().replace(field, replacement);
            let snapshot = parse_file_providerctl_snapshot(&output, 42, "content-hash").unwrap();
            assert!(!snapshot.is_sync_complete(), "{field}");
        }
        assert!(parse_file_providerctl_snapshot("isUploaded = maybe;", 42, "hash").is_err());
    }

    #[cfg(all(not(target_os = "macos"), not(windows)))]
    #[test]
    fn existing_destination_gate_rejects_missing_or_wrong_size_without_reading_bytes() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("existing.bin");
        std::fs::write(&path, b"bytes").unwrap();
        assert_eq!(
            require_existing_destination_local_current(CloudProvider::Onedrive, &path, 99)
                .unwrap_err(),
            "existing-destination-not-materialized"
        );
        assert!(
            require_existing_destination_local_current(CloudProvider::Onedrive, &path, 5).is_ok()
        );
    }

    #[cfg(windows)]
    #[test]
    fn existing_destination_gate_fails_closed_without_provider_status_on_windows() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("existing.bin");
        std::fs::write(&path, b"bytes").unwrap();
        assert_eq!(
            require_existing_destination_local_current(CloudProvider::Onedrive, &path, 5)
                .unwrap_err(),
            "existing-destination-provider-status-unavailable"
        );
    }

    fn api_snapshot(provider: CloudProvider, checksum: &str) -> ProviderApiSnapshot {
        ProviderApiSnapshot {
            provider,
            remote_object_id: "remote-id".into(),
            remote_revision: "revision-1".into(),
            remote_checksum: checksum.into(),
            observed_bytes: 42,
            destination_blake3: "content-hash".into(),
            available: true,
            trashed: false,
        }
    }

    #[test]
    fn provider_api_snapshots_bind_onedrive_and_google_checksums() {
        for (provider, checksum, algorithm) in [
            (
                CloudProvider::Onedrive,
                "quick-xor",
                RemoteChecksumAlgorithm::QuickXor,
            ),
            (
                CloudProvider::GoogleDrive,
                "SHA256-HASH",
                RemoteChecksumAlgorithm::Sha256,
            ),
        ] {
            let evidence = evidence_from_provider_api_snapshot(
                &receipt(provider),
                &api_snapshot(provider, checksum),
                30,
            )
            .unwrap();
            assert!(evidence.sync_complete);
            assert_eq!(evidence.kind, SyncEvidenceKind::ProviderApi);
            assert!(evidence.evidence_id.starts_with("provider-api:"));
            assert_eq!(evidence.evidence_id.len(), 77);
            let proof = evidence.remote_content.unwrap();
            assert_eq!(proof.algorithm, algorithm);
            assert!(!proof.location_bound);

            let location_bound = evidence_from_provider_api_snapshot_with_location(
                &receipt(provider),
                &api_snapshot(provider, checksum),
                Some("provider-path-v1:proof"),
                30,
            )
            .unwrap();
            let location_proof = location_bound.remote_content.unwrap();
            assert!(location_proof.location_bound);
            assert_eq!(
                location_proof.location_proof.as_deref(),
                Some("provider-path-v1:proof")
            );
            assert_ne!(evidence.evidence_id, location_bound.evidence_id);
        }
    }

    #[test]
    fn provider_api_snapshots_fail_closed_on_remote_or_local_drift() {
        let api_receipt = receipt(CloudProvider::Onedrive);
        let mut snapshot = api_snapshot(CloudProvider::Onedrive, "wrong");
        snapshot.remote_object_id = " ".into();
        snapshot.remote_revision = " ".into();
        snapshot.observed_bytes = 41;
        snapshot.destination_blake3 = "wrong".into();
        snapshot.available = false;
        snapshot.trashed = true;
        assert!(
            !evidence_from_provider_api_snapshot(&api_receipt, &snapshot, 30)
                .unwrap()
                .sync_complete
        );

        let mut empty_expected = api_receipt;
        empty_expected.quick_xor_base64.clear();
        snapshot.remote_checksum.clear();
        snapshot.remote_object_id = "remote-id".into();
        snapshot.remote_revision = "revision-1".into();
        snapshot.observed_bytes = 42;
        snapshot.destination_blake3 = "content-hash".into();
        snapshot.available = true;
        snapshot.trashed = false;
        assert!(
            !evidence_from_provider_api_snapshot(&empty_expected, &snapshot, 30)
                .unwrap()
                .sync_complete
        );
    }

    #[test]
    fn provider_api_snapshot_rejects_provider_mismatch_and_icloud() {
        assert_eq!(
            evidence_from_provider_api_snapshot(
                &receipt(CloudProvider::Onedrive),
                &api_snapshot(CloudProvider::GoogleDrive, "sha256-hash"),
                30,
            )
            .unwrap_err(),
            "provider-mismatch"
        );
        assert_eq!(
            evidence_from_provider_api_snapshot(
                &receipt(CloudProvider::Icloud),
                &api_snapshot(CloudProvider::Icloud, "unused"),
                30,
            )
            .unwrap_err(),
            "icloud-native-status-required"
        );
    }

    #[test]
    fn provider_api_response_parsers_keep_only_bounded_remote_proof_fields() {
        let onedrive = parse_onedrive_item_snapshot(
            r#"{
                "id":"one-id","size":42,"eTag":"one-etag",
                "file":{"mimeType":"application/pdf","hashes":{"quickXorHash":"quick-xor"}},
                "name":"not retained"
            }"#,
            "content-hash",
        )
        .unwrap();
        assert_eq!(onedrive.provider, CloudProvider::Onedrive);
        assert_eq!(onedrive.remote_object_id, "one-id");
        assert_eq!(onedrive.remote_revision, "one-etag");
        assert_eq!(onedrive.remote_checksum, "quick-xor");
        assert_eq!(onedrive.observed_bytes, 42);
        assert_eq!(onedrive.destination_blake3, "content-hash");
        assert!(onedrive.available);
        assert!(!onedrive.trashed);

        let google = parse_google_drive_file_snapshot(
            r#"{
                "id":"google-id","version":"7","size":"42",
                "sha256Checksum":"sha256-hash","trashed":true,
                "name":"not retained"
            }"#,
            "content-hash",
        )
        .unwrap();
        assert_eq!(google.provider, CloudProvider::GoogleDrive);
        assert_eq!(google.remote_object_id, "google-id");
        assert_eq!(google.remote_revision, "7");
        assert_eq!(google.remote_checksum, "sha256-hash");
        assert_eq!(google.observed_bytes, 42);
        assert_eq!(google.destination_blake3, "content-hash");
        assert!(google.available);
        assert!(google.trashed);
    }

    #[test]
    fn provider_api_response_parsers_reject_malformed_or_unverifiable_shapes() {
        assert_eq!(
            parse_onedrive_item_snapshot("not-json", "hash").unwrap_err(),
            "onedrive-response-invalid"
        );
        for json in [r#"{}"#, r#"{"file":{}}"#] {
            assert_eq!(
                parse_onedrive_item_snapshot(json, "hash").unwrap_err(),
                "onedrive-file-hashes-missing"
            );
        }
        assert_eq!(
            parse_google_drive_file_snapshot("not-json", "hash").unwrap_err(),
            "google-drive-response-invalid"
        );
        assert_eq!(
            parse_google_drive_file_snapshot(r#"{}"#, "hash").unwrap_err(),
            "google-drive-size-missing"
        );
        assert_eq!(
            parse_google_drive_file_snapshot(r#"{"size":"NaN"}"#, "hash").unwrap_err(),
            "google-drive-size-invalid"
        );

        let defaults = parse_google_drive_file_snapshot(r#"{"size":"0"}"#, "hash").unwrap();
        assert!(defaults.remote_object_id.is_empty());
        assert!(defaults.remote_revision.is_empty());
        assert!(defaults.remote_checksum.is_empty());
        assert!(!defaults.trashed);
    }

    #[test]
    fn incomplete_uploading_non_ubiquitous_or_non_current_status_fails_closed() {
        let receipt = receipt(CloudProvider::Icloud);
        for snapshot in [
            IcloudStatusSnapshot {
                is_ubiquitous: true,
                is_uploaded: false,
                is_uploading: true,
                is_current: true,
                observed_bytes: 42,
                destination_blake3: "content-hash".into(),
            },
            IcloudStatusSnapshot {
                is_ubiquitous: false,
                is_uploaded: true,
                is_uploading: false,
                is_current: true,
                observed_bytes: 42,
                destination_blake3: "content-hash".into(),
            },
            IcloudStatusSnapshot {
                is_ubiquitous: true,
                is_uploaded: true,
                is_uploading: true,
                is_current: true,
                observed_bytes: 42,
                destination_blake3: "content-hash".into(),
            },
            IcloudStatusSnapshot {
                is_ubiquitous: true,
                is_uploaded: true,
                is_uploading: false,
                is_current: false,
                observed_bytes: 42,
                destination_blake3: "content-hash".into(),
            },
        ] {
            let evidence = evidence_from_icloud_snapshot(&receipt, &snapshot, 30).unwrap();
            assert!(!evidence.sync_complete);
        }
    }

    #[test]
    fn adapter_rejects_wrong_provider_and_missing_destination() {
        assert_eq!(
            evidence_from_icloud_snapshot(
                &receipt(CloudProvider::Onedrive),
                &IcloudStatusSnapshot {
                    is_ubiquitous: true,
                    is_uploaded: true,
                    is_uploading: false,
                    is_current: true,
                    observed_bytes: 42,
                    destination_blake3: "content-hash".into(),
                },
                30,
            )
            .unwrap_err(),
            "icloud-receipt-required"
        );

        let mut missing = receipt(CloudProvider::Icloud);
        missing.destination = " ".into();
        assert_eq!(
            evidence_from_icloud_snapshot(
                &missing,
                &IcloudStatusSnapshot {
                    is_ubiquitous: true,
                    is_uploaded: true,
                    is_uploading: false,
                    is_current: true,
                    observed_bytes: 42,
                    destination_blake3: "content-hash".into(),
                },
                30,
            )
            .unwrap_err(),
            "destination-missing"
        );
    }

    #[cfg(all(target_os = "macos", not(coverage)))]
    #[test]
    fn native_probe_rejects_non_icloud_file_without_mutation() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("local-only.bin");
        std::fs::write(&path, b"local-only").unwrap();
        let mut local_receipt = receipt(CloudProvider::Icloud);
        local_receipt.destination = path.to_string_lossy().into_owned();
        local_receipt.bytes = 10;
        local_receipt.blake3 = blake3::hash(b"local-only").to_hex().to_string();

        let result = collect_icloud_sync_evidence(&local_receipt, 30);
        assert!(
            !result
                .map(|evidence| evidence.sync_complete)
                .unwrap_or(false),
            "a non-iCloud file must never produce complete upload evidence"
        );
        assert_eq!(std::fs::read(path).unwrap(), b"local-only");
    }

    #[cfg(all(target_os = "macos", not(coverage)))]
    #[test]
    #[ignore = "requires DISKSAGE_ICLOUD_LIVE_PATH pointing to an already-local iCloud file"]
    fn live_foundation_probe_is_read_only_and_hash_bound() {
        use std::time::{SystemTime, UNIX_EPOCH};

        let path = std::env::var("DISKSAGE_ICLOUD_LIVE_PATH").unwrap();
        let metadata = std::fs::symlink_metadata(&path).unwrap();
        let (is_ubiquitous, is_uploaded, is_uploading, is_current) =
            foundation_icloud_status(&path).unwrap();
        assert!(is_ubiquitous && is_uploaded && !is_uploading && is_current);
        let content_hash = hash_file(std::path::Path::new(&path)).unwrap();
        let mut live_receipt = receipt(CloudProvider::Icloud);
        live_receipt.destination = path;
        live_receipt.bytes = metadata.len();
        live_receipt.blake3 = content_hash;
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let evidence = collect_icloud_sync_evidence(&live_receipt, now_ms).unwrap();
        assert!(evidence.sync_complete);
        assert_eq!(evidence.observed_bytes, live_receipt.bytes);
        assert_eq!(evidence.destination_blake3, live_receipt.blake3);
    }

    #[cfg(coverage)]
    #[test]
    fn coverage_build_has_explicit_unsupported_native_adapter() {
        assert_eq!(
            collect_icloud_sync_evidence(&receipt(CloudProvider::Icloud), 30).unwrap_err(),
            "icloud-native-status-unsupported-platform"
        );
    }
}
