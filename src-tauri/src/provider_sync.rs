use crate::cloud::CloudProvider;
use crate::cloud_transfer::{
    CloudCopyReceipt, ProviderSyncEvidence, RemoteChecksumAlgorithm, RemoteContentProof,
    SyncEvidenceKind,
};

#[cfg(test)]
use crate::cloud_transfer::LEGACY_RECEIPT_VERSION;

const ICLOUD_UPLOADED_KEY: &str = "NSURLUbiquitousItemIsUploadedKey";

/// Minimal, provider-native facts collected for one destination.
///
/// Keeping this value independent from Foundation makes the decision logic deterministic and
/// testable. The macOS adapter below is only responsible for collecting these facts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IcloudStatusSnapshot {
    pub is_ubiquitous: bool,
    pub is_uploaded: bool,
    pub is_current: bool,
    pub observed_bytes: u64,
    pub destination_blake3: String,
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
    hasher.update(&[
        snapshot.is_ubiquitous as u8,
        snapshot.is_uploaded as u8,
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
    let sync_complete = snapshot.is_ubiquitous && snapshot.is_uploaded && snapshot.is_current;
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
        remote_content: None,
    })
}

const FILE_PROVIDER_CTL_EVALUATE: &str = "fileproviderctl:evaluate";

/// Provider-neutral facts exposed by macOS File Provider for third-party cloud roots.
///
/// Acquisition of the facts is platform-specific, while this value and its decision policy stay
/// deterministic and unit-testable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileProviderStatusSnapshot {
    pub is_downloaded: bool,
    pub is_downloading: bool,
    pub is_most_recent_version_downloaded: bool,
    pub is_uploaded: bool,
    pub is_uploading: bool,
    pub is_excluded_from_sync: bool,
    pub is_sync_paused: bool,
    pub observed_bytes: u64,
    pub destination_blake3: String,
}

impl FileProviderStatusSnapshot {
    fn is_local_current(&self) -> bool {
        self.is_downloaded && !self.is_downloading && self.is_most_recent_version_downloaded
    }

    fn is_sync_complete(&self) -> bool {
        self.is_local_current()
            && self.is_uploaded
            && !self.is_uploading
            && !self.is_excluded_from_sync
            && !self.is_sync_paused
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
        snapshot.is_downloaded as u8,
        snapshot.is_downloading as u8,
        snapshot.is_most_recent_version_downloaded as u8,
        snapshot.is_uploaded as u8,
        snapshot.is_uploading as u8,
        snapshot.is_excluded_from_sync as u8,
        snapshot.is_sync_paused as u8,
    ]);
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
        remote_content: None,
    })
}

fn file_provider_status_bool(output: &str, key: &str) -> Result<bool, String> {
    let prefix = format!("{key} = ");
    let value = output
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix(&prefix))
        .map(|value| value.trim().trim_end_matches(';'))
        .ok_or_else(|| format!("file-provider-status-field-missing:{key}"))?;
    match value {
        "1" => Ok(true),
        "0" => Ok(false),
        _ => Err(format!("file-provider-status-field-invalid:{key}")),
    }
}

pub fn parse_file_providerctl_snapshot(
    output: &str,
    observed_bytes: u64,
    destination_blake3: &str,
) -> Result<FileProviderStatusSnapshot, String> {
    Ok(FileProviderStatusSnapshot {
        is_downloaded: file_provider_status_bool(output, "isDownloaded")?,
        is_downloading: file_provider_status_bool(output, "isDownloading")?,
        is_most_recent_version_downloaded: file_provider_status_bool(
            output,
            "isMostRecentVersionDownloaded",
        )?,
        is_uploaded: file_provider_status_bool(output, "isUploaded")?,
        is_uploading: file_provider_status_bool(output, "isUploading")?,
        is_excluded_from_sync: file_provider_status_bool(output, "isExcludedFromSync")?,
        is_sync_paused: file_provider_status_bool(output, "isSyncPaused")?,
        observed_bytes,
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
fn foundation_icloud_status(path: &str) -> Result<(bool, bool, bool), String> {
    use objc2::rc::autoreleasepool;
    use objc2_foundation::{
        NSString, NSURLIsUbiquitousItemKey, NSURLUbiquitousItemDownloadingStatusCurrent,
        NSURLUbiquitousItemDownloadingStatusKey, NSURLUbiquitousItemIsUploadedKey, NSURL,
    };

    autoreleasepool(|_| {
        let url = NSURL::fileURLWithPath(&NSString::from_str(path));
        // SAFETY: These are Foundation-exported, process-lifetime NSURL resource-key and value
        // constants with the types declared by objc2-foundation.
        unsafe {
            let is_ubiquitous = foundation_bool_resource(&url, NSURLIsUbiquitousItemKey)?;
            if !is_ubiquitous {
                return Ok((false, false, false));
            }
            let downloading_status =
                foundation_string_resource(&url, NSURLUbiquitousItemDownloadingStatusKey)?;
            Ok((
                is_ubiquitous,
                foundation_bool_resource(&url, NSURLUbiquitousItemIsUploadedKey)?,
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
fn file_providerctl_status(path: &str) -> Result<String, String> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    const TIMEOUT: Duration = Duration::from_secs(5);
    const OUTPUT_LIMIT: u64 = 256 * 1_024;

    let mut child = Command::new("/usr/bin/fileproviderctl")
        .arg("evaluate")
        .arg(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "file-provider-status-command-unavailable".to_string())?;
    let deadline = Instant::now() + TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(25));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("file-provider-status-command-timeout".into());
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("file-provider-status-command-wait-failed".into());
            }
        }
    };
    let mut output = Vec::new();
    child
        .stdout
        .take()
        .ok_or_else(|| "file-provider-status-output-missing".to_string())?
        .take(OUTPUT_LIMIT + 1)
        .read_to_end(&mut output)
        .map_err(|_| "file-provider-status-output-read-failed".to_string())?;
    if !status.success() {
        return Err("file-provider-status-command-failed".into());
    }
    if output.len() as u64 > OUTPUT_LIMIT {
        return Err("file-provider-status-output-too-large".into());
    }
    String::from_utf8(output).map_err(|_| "file-provider-status-output-not-utf8".into())
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
    let (before_ubiquitous, _, before_current) = foundation_icloud_status(path)?;
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
    let (is_ubiquitous, is_uploaded, is_current) = foundation_icloud_status(path)?;
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
        assert_eq!(evidence.remote_content, None);
    }

    fn uploaded_file_provider_output() -> &'static str {
        r#"
            isDownloaded = 1;
            isDownloading = 0;
            isMostRecentVersionDownloaded = 1;
            isUploaded = 1;
            isUploading = 0;
            isExcludedFromSync = 0;
            isSyncPaused = 0;
        "#
    }

    #[test]
    fn third_party_file_provider_status_becomes_complete_native_evidence() {
        let snapshot =
            parse_file_providerctl_snapshot(uploaded_file_provider_output(), 42, "content-hash")
                .unwrap();
        assert!(snapshot.is_local_current());
        assert!(snapshot.is_sync_complete());

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
            ("isExcludedFromSync = 0", "isExcludedFromSync = 1"),
            ("isSyncPaused = 0", "isSyncPaused = 1"),
        ] {
            let output = uploaded_file_provider_output().replace(field, replacement);
            let snapshot = parse_file_providerctl_snapshot(&output, 42, "content-hash").unwrap();
            assert!(!snapshot.is_sync_complete(), "{field}");
        }
        assert!(parse_file_providerctl_snapshot("isUploaded = maybe;", 42, "hash").is_err());
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
    fn incomplete_non_ubiquitous_or_non_current_status_fails_closed() {
        let receipt = receipt(CloudProvider::Icloud);
        for snapshot in [
            IcloudStatusSnapshot {
                is_ubiquitous: true,
                is_uploaded: false,
                is_current: true,
                observed_bytes: 42,
                destination_blake3: "content-hash".into(),
            },
            IcloudStatusSnapshot {
                is_ubiquitous: false,
                is_uploaded: true,
                is_current: true,
                observed_bytes: 42,
                destination_blake3: "content-hash".into(),
            },
            IcloudStatusSnapshot {
                is_ubiquitous: true,
                is_uploaded: true,
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
        let (is_ubiquitous, is_uploaded, is_current) = foundation_icloud_status(&path).unwrap();
        assert!(is_ubiquitous && is_uploaded && is_current);
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
