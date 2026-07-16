use crate::cloud::CloudProvider;
use crate::cloud_transfer::{CloudCopyReceipt, ProviderSyncEvidence, SyncEvidenceKind};

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
            let downloading_status =
                foundation_string_resource(&url, NSURLUbiquitousItemDownloadingStatusKey)?;
            Ok((
                foundation_bool_resource(&url, NSURLIsUbiquitousItemKey)?,
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
            version: 1,
            receipt_id: "receipt-id".into(),
            candidate_fingerprint: "metadata-fingerprint".into(),
            provider,
            source: "/source/report.pdf".into(),
            destination: "/cloud/report.pdf".into(),
            bytes: 42,
            blake3: "content-hash".into(),
            source_modified_ms: 10,
            copied_at_ms: 20,
            copy_verified: true,
            provider_sync_confirmed: false,
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

        assert!(collect_icloud_sync_evidence(&local_receipt, 30).is_err());
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
