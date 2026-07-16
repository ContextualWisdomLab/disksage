//! Authenticated, read-only metadata clients for cloud-provider content proof.
//!
//! Callers supply an ephemeral OAuth access token and a provider-native object identifier. The
//! production transport only talks to fixed Microsoft Graph or Google Drive API hosts, never
//! persists the token, and never includes it in returned errors.

use crate::cloud::CloudProvider;
use crate::cloud_transfer::{CloudCopyReceipt, ProviderSyncEvidence};
use crate::content_digest::ContentDigests;
#[cfg(not(coverage))]
use crate::content_digest::ContentHasher;
use crate::provider_sync::{
    evidence_from_provider_api_snapshot, parse_google_drive_file_snapshot,
    parse_onedrive_item_snapshot,
};
#[cfg(not(coverage))]
use std::io::Read;
use std::path::Path;

const MAX_REMOTE_ID_BYTES: usize = 1_024;
#[cfg(not(coverage))]
const MAX_METADATA_RESPONSE_BYTES: u64 = 256 * 1_024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderRemoteLocator {
    OneDriveItemId(String),
    GoogleDriveFileId(String),
}

impl ProviderRemoteLocator {
    pub fn provider(&self) -> CloudProvider {
        match self {
            Self::OneDriveItemId(_) => CloudProvider::Onedrive,
            Self::GoogleDriveFileId(_) => CloudProvider::GoogleDrive,
        }
    }

    pub fn object_id(&self) -> &str {
        match self {
            Self::OneDriveItemId(id) | Self::GoogleDriveFileId(id) => id,
        }
    }
}

fn percent_encode_segment(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            use std::fmt::Write;
            write!(&mut encoded, "%{byte:02X}").expect("writing to String cannot fail");
        }
    }
    encoded
}

pub fn provider_metadata_url(locator: &ProviderRemoteLocator) -> Result<String, String> {
    let object_id = locator.object_id();
    if object_id.is_empty()
        || object_id.trim() != object_id
        || object_id.len() > MAX_REMOTE_ID_BYTES
        || object_id.bytes().any(|byte| byte.is_ascii_control())
    {
        return Err("provider-object-id-invalid".into());
    }
    let encoded = percent_encode_segment(object_id);
    Ok(match locator {
        ProviderRemoteLocator::OneDriveItemId(_) => format!(
            "https://graph.microsoft.com/v1.0/me/drive/items/{encoded}?%24select=id%2Csize%2CeTag%2Cfile%2Cdeleted"
        ),
        ProviderRemoteLocator::GoogleDriveFileId(_) => format!(
            "https://www.googleapis.com/drive/v3/files/{encoded}?fields=id%2Cversion%2Csize%2Csha256Checksum%2Ctrashed"
        ),
    })
}

#[cfg(not(coverage))]
pub trait ProviderMetadataTransport {
    fn fetch_json(
        &self,
        locator: &ProviderRemoteLocator,
        bearer_token: &str,
    ) -> Result<String, String>;
}

#[cfg(not(coverage))]
pub struct FixedHostProviderMetadataClient {
    agent: ureq::Agent,
}

#[cfg(not(coverage))]
impl Default for FixedHostProviderMetadataClient {
    fn default() -> Self {
        let config = ureq::Agent::config_builder()
            .https_only(true)
            .max_redirects(0)
            .timeout_global(Some(std::time::Duration::from_secs(20)))
            .build();
        Self {
            agent: ureq::Agent::new_with_config(config),
        }
    }
}

#[cfg(not(coverage))]
fn safe_transport_error(error: ureq::Error) -> String {
    match error {
        ureq::Error::StatusCode(code) => format!("provider-api-http-status:{code}"),
        ureq::Error::Timeout(_) => "provider-api-timeout".into(),
        ureq::Error::HostNotFound => "provider-api-host-not-found".into(),
        ureq::Error::BodyExceedsLimit(_) => "provider-api-response-too-large".into(),
        _ => "provider-api-request-failed".into(),
    }
}

#[cfg(not(coverage))]
impl ProviderMetadataTransport for FixedHostProviderMetadataClient {
    fn fetch_json(
        &self,
        locator: &ProviderRemoteLocator,
        bearer_token: &str,
    ) -> Result<String, String> {
        let url = provider_metadata_url(locator)?;
        let authorization = format!("Bearer {bearer_token}");
        let mut response = self
            .agent
            .get(&url)
            .header("Authorization", &authorization)
            .header("Accept", "application/json")
            .call()
            .map_err(safe_transport_error)?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(format!("provider-api-http-status:{status}"));
        }
        response
            .body_mut()
            .with_config()
            .limit(MAX_METADATA_RESPONSE_BYTES)
            .read_to_string()
            .map_err(safe_transport_error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg(not(coverage))]
struct LocalFileIdentity {
    bytes: u64,
    modified: std::time::SystemTime,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
}

#[cfg(not(coverage))]
fn local_file_identity(path: &Path) -> Result<LocalFileIdentity, String> {
    let metadata = std::fs::symlink_metadata(path).map_err(|_| "destination-unavailable")?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("destination-must-be-regular-file".into());
    }
    #[cfg(unix)]
    use std::os::unix::fs::MetadataExt;
    Ok(LocalFileIdentity {
        bytes: metadata.len(),
        modified: metadata
            .modified()
            .map_err(|_| "destination-modified-time-unavailable")?,
        #[cfg(unix)]
        device: metadata.dev(),
        #[cfg(unix)]
        inode: metadata.ino(),
    })
}

#[cfg(not(coverage))]
fn hash_file(path: &Path) -> Result<ContentDigests, String> {
    let mut file = std::fs::File::open(path).map_err(|_| "destination-unreadable")?;
    let mut hasher = ContentHasher::default();
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| "destination-read-failed")?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize())
}

#[cfg(not(coverage))]
fn stable_local_snapshot(path: &Path) -> Result<(LocalFileIdentity, ContentDigests), String> {
    let before = local_file_identity(path)?;
    let digests = hash_file(path)?;
    let after = local_file_identity(path)?;
    finish_stable_snapshot(before, digests, after)
}

#[cfg(not(coverage))]
fn finish_stable_snapshot(
    before: LocalFileIdentity,
    digests: ContentDigests,
    after: LocalFileIdentity,
) -> Result<(LocalFileIdentity, ContentDigests), String> {
    if before != after {
        return Err("destination-changed-during-hash".into());
    }
    Ok((after, digests))
}

fn digests_match_receipt(digests: &ContentDigests, receipt: &CloudCopyReceipt) -> bool {
    digests.blake3 == receipt.blake3
        && digests.sha256.eq_ignore_ascii_case(&receipt.sha256)
        && digests.quick_xor_base64 == receipt.quick_xor_base64
}

pub fn evidence_from_provider_api_json(
    receipt: &CloudCopyReceipt,
    locator: &ProviderRemoteLocator,
    json: &str,
    destination_digests: &ContentDigests,
    confirmed_at_ms: u64,
) -> Result<ProviderSyncEvidence, String> {
    if locator.provider() != receipt.provider {
        return Err("provider-mismatch".into());
    }
    if !digests_match_receipt(destination_digests, receipt) {
        return Err("destination-content-mismatch".into());
    }
    let snapshot = match locator {
        ProviderRemoteLocator::OneDriveItemId(_) => {
            parse_onedrive_item_snapshot(json, &destination_digests.blake3)?
        }
        ProviderRemoteLocator::GoogleDriveFileId(_) => {
            parse_google_drive_file_snapshot(json, &destination_digests.blake3)?
        }
    };
    if snapshot.remote_object_id != locator.object_id() {
        return Err("provider-object-id-mismatch".into());
    }
    evidence_from_provider_api_snapshot(receipt, &snapshot, confirmed_at_ms)
}

/// Fetch authenticated provider metadata between two stable local content snapshots.
///
/// The access token is borrowed only for the request and is neither persisted nor included in an
/// error. This function is read-only: it does not upload, move, evict, or delete any file.
#[cfg(not(coverage))]
pub fn collect_authenticated_provider_api_evidence(
    receipt: &CloudCopyReceipt,
    locator: &ProviderRemoteLocator,
    bearer_token: &str,
    transport: &dyn ProviderMetadataTransport,
    confirmed_at_ms: u64,
) -> Result<ProviderSyncEvidence, String> {
    if bearer_token.trim().is_empty() {
        return Err("provider-access-token-missing".into());
    }
    if locator.provider() != receipt.provider {
        return Err("provider-mismatch".into());
    }
    provider_metadata_url(locator)?;
    let destination = Path::new(&receipt.destination);
    if !destination.is_absolute() {
        return Err("destination-path-not-absolute".into());
    }
    let (before_identity, before_digests) = stable_local_snapshot(destination)?;
    if before_identity.bytes != receipt.bytes || !digests_match_receipt(&before_digests, receipt) {
        return Err("destination-content-mismatch".into());
    }
    let json = transport.fetch_json(locator, bearer_token)?;
    let (after_identity, after_digests) = stable_local_snapshot(destination)?;
    if after_identity != before_identity || after_digests != before_digests {
        return Err("destination-changed-during-provider-check".into());
    }
    evidence_from_provider_api_json(receipt, locator, &json, &after_digests, confirmed_at_ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud_transfer::RECEIPT_VERSION;
    #[cfg(not(coverage))]
    use std::cell::Cell;
    #[cfg(not(coverage))]
    use std::path::PathBuf;

    fn receipt(provider: CloudProvider, destination: &Path, bytes: &[u8]) -> CloudCopyReceipt {
        let digests = crate::content_digest::digest_bytes(bytes);
        CloudCopyReceipt {
            version: RECEIPT_VERSION,
            receipt_id: "receipt-id".into(),
            candidate_fingerprint: "metadata-fingerprint".into(),
            provider,
            source: "/source/report.pdf".into(),
            destination: destination.to_string_lossy().into_owned(),
            bytes: bytes.len() as u64,
            blake3: digests.blake3,
            sha256: digests.sha256,
            quick_xor_base64: digests.quick_xor_base64,
            source_modified_ms: 10,
            copied_at_ms: 20,
            copy_verified: true,
            provider_sync_confirmed: false,
        }
    }

    #[cfg(not(coverage))]
    struct StaticTransport {
        json: String,
        calls: Cell<usize>,
        expected_token: String,
        mutate_path: Option<PathBuf>,
    }

    #[cfg(not(coverage))]
    impl ProviderMetadataTransport for StaticTransport {
        fn fetch_json(
            &self,
            _locator: &ProviderRemoteLocator,
            bearer_token: &str,
        ) -> Result<String, String> {
            assert_eq!(bearer_token, self.expected_token);
            self.calls.set(self.calls.get() + 1);
            if let Some(path) = &self.mutate_path {
                std::fs::write(path, b"changed").unwrap();
            }
            Ok(self.json.clone())
        }
    }

    #[cfg(not(coverage))]
    fn transport(json: &str) -> StaticTransport {
        StaticTransport {
            json: json.into(),
            calls: Cell::new(0),
            expected_token: "secret-token".into(),
            mutate_path: None,
        }
    }

    #[test]
    fn fixed_host_urls_percent_encode_only_the_object_id() {
        assert_eq!(
            provider_metadata_url(&ProviderRemoteLocator::OneDriveItemId("abc! 1".into()))
                .unwrap(),
            "https://graph.microsoft.com/v1.0/me/drive/items/abc%21%201?%24select=id%2Csize%2CeTag%2Cfile%2Cdeleted"
        );
        assert_eq!(
            provider_metadata_url(&ProviderRemoteLocator::GoogleDriveFileId("g/id".into()))
                .unwrap(),
            "https://www.googleapis.com/drive/v3/files/g%2Fid?fields=id%2Cversion%2Csize%2Csha256Checksum%2Ctrashed"
        );
        for id in ["", " leading", "trailing ", "line\nbreak"] {
            assert_eq!(
                provider_metadata_url(&ProviderRemoteLocator::GoogleDriveFileId(id.into()))
                    .unwrap_err(),
                "provider-object-id-invalid"
            );
        }
        assert_eq!(
            provider_metadata_url(&ProviderRemoteLocator::OneDriveItemId(
                "a".repeat(MAX_REMOTE_ID_BYTES + 1)
            ))
            .unwrap_err(),
            "provider-object-id-invalid"
        );
    }

    #[test]
    #[cfg(not(coverage))]
    fn authenticated_collection_binds_onedrive_response_to_stable_local_bytes() {
        let temporary = tempfile::tempdir().unwrap();
        let destination = temporary.path().join("report.pdf");
        std::fs::write(&destination, b"hello-cloud").unwrap();
        let receipt = receipt(CloudProvider::Onedrive, &destination, b"hello-cloud");
        let locator = ProviderRemoteLocator::OneDriveItemId("one-id".into());
        let remote = transport(&format!(
            r#"{{"id":"one-id","size":11,"eTag":"revision-1","file":{{"hashes":{{"quickXorHash":"{}"}}}}}}"#,
            receipt.quick_xor_base64
        ));

        let evidence = collect_authenticated_provider_api_evidence(
            &receipt,
            &locator,
            "secret-token",
            &remote,
            30,
        )
        .unwrap();
        assert!(evidence.sync_complete);
        assert_eq!(remote.calls.get(), 1);
    }

    #[test]
    #[cfg(not(coverage))]
    fn authenticated_collection_binds_google_response_and_rejects_object_substitution() {
        let temporary = tempfile::tempdir().unwrap();
        let destination = temporary.path().join("report.pdf");
        std::fs::write(&destination, b"hello-cloud").unwrap();
        let receipt = receipt(CloudProvider::GoogleDrive, &destination, b"hello-cloud");
        let locator = ProviderRemoteLocator::GoogleDriveFileId("google-id".into());
        let remote = transport(&format!(
            r#"{{"id":"google-id","version":"7","size":"11","sha256Checksum":"{}","trashed":false}}"#,
            receipt.sha256
        ));
        assert!(
            collect_authenticated_provider_api_evidence(
                &receipt,
                &locator,
                "secret-token",
                &remote,
                30,
            )
            .unwrap()
            .sync_complete
        );

        let substituted = transport(&format!(
            r#"{{"id":"other-id","version":"7","size":"11","sha256Checksum":"{}"}}"#,
            receipt.sha256
        ));
        assert_eq!(
            collect_authenticated_provider_api_evidence(
                &receipt,
                &locator,
                "secret-token",
                &substituted,
                30,
            )
            .unwrap_err(),
            "provider-object-id-mismatch"
        );
    }

    #[test]
    #[cfg(not(coverage))]
    fn collection_fails_closed_before_network_for_invalid_local_or_auth_state() {
        let temporary = tempfile::tempdir().unwrap();
        let destination = temporary.path().join("report.pdf");
        std::fs::write(&destination, b"hello-cloud").unwrap();
        let mut receipt = receipt(CloudProvider::Onedrive, &destination, b"hello-cloud");
        let locator = ProviderRemoteLocator::OneDriveItemId("one-id".into());
        let remote = transport("{}");

        assert_eq!(
            collect_authenticated_provider_api_evidence(&receipt, &locator, " ", &remote, 30)
                .unwrap_err(),
            "provider-access-token-missing"
        );
        receipt.provider = CloudProvider::GoogleDrive;
        assert_eq!(
            collect_authenticated_provider_api_evidence(
                &receipt,
                &locator,
                "secret-token",
                &remote,
                30,
            )
            .unwrap_err(),
            "provider-mismatch"
        );
        receipt.provider = CloudProvider::Onedrive;
        receipt.destination = "relative.pdf".into();
        assert_eq!(
            collect_authenticated_provider_api_evidence(
                &receipt,
                &locator,
                "secret-token",
                &remote,
                30,
            )
            .unwrap_err(),
            "destination-path-not-absolute"
        );
        assert_eq!(remote.calls.get(), 0);
    }

    #[test]
    #[cfg(not(coverage))]
    fn local_snapshot_rejects_non_file_and_identity_change() {
        let temporary = tempfile::tempdir().unwrap();
        assert_eq!(
            local_file_identity(temporary.path()).unwrap_err(),
            "destination-must-be-regular-file"
        );
        assert_eq!(
            local_file_identity(&temporary.path().join("missing")).unwrap_err(),
            "destination-unavailable"
        );

        let first = LocalFileIdentity {
            bytes: 1,
            modified: std::time::UNIX_EPOCH,
            #[cfg(unix)]
            device: 1,
            #[cfg(unix)]
            inode: 1,
        };
        let mut changed = first.clone();
        changed.bytes = 2;
        assert_eq!(
            finish_stable_snapshot(first, crate::content_digest::digest_bytes(b"a"), changed,)
                .unwrap_err(),
            "destination-changed-during-hash"
        );
    }

    #[test]
    #[cfg(not(coverage))]
    fn collection_rejects_local_drift_before_or_during_provider_request() {
        let temporary = tempfile::tempdir().unwrap();
        let destination = temporary.path().join("report.pdf");
        std::fs::write(&destination, b"hello-cloud").unwrap();
        let mut wrong_receipt = receipt(CloudProvider::Onedrive, &destination, b"hello-cloud");
        let locator = ProviderRemoteLocator::OneDriveItemId("one-id".into());
        wrong_receipt.blake3 = "wrong".into();
        let remote = transport("{}");
        assert_eq!(
            collect_authenticated_provider_api_evidence(
                &wrong_receipt,
                &locator,
                "secret-token",
                &remote,
                30,
            )
            .unwrap_err(),
            "destination-content-mismatch"
        );
        assert_eq!(remote.calls.get(), 0);

        let receipt = receipt(CloudProvider::Onedrive, &destination, b"hello-cloud");
        let mut mutating = transport("{}");
        mutating.mutate_path = Some(destination.clone());
        assert_eq!(
            collect_authenticated_provider_api_evidence(
                &receipt,
                &locator,
                "secret-token",
                &mutating,
                30,
            )
            .unwrap_err(),
            "destination-changed-during-provider-check"
        );
        assert_eq!(mutating.calls.get(), 1);
    }

    #[test]
    fn json_evidence_rejects_wrong_provider_or_local_digest() {
        let digests = crate::content_digest::digest_bytes(b"hello-cloud");
        let receipt = CloudCopyReceipt {
            version: RECEIPT_VERSION,
            receipt_id: "receipt-id".into(),
            candidate_fingerprint: "fingerprint".into(),
            provider: CloudProvider::Onedrive,
            source: "/source".into(),
            destination: "/destination".into(),
            bytes: 11,
            blake3: digests.blake3.clone(),
            sha256: digests.sha256.clone(),
            quick_xor_base64: digests.quick_xor_base64.clone(),
            source_modified_ms: 1,
            copied_at_ms: 2,
            copy_verified: true,
            provider_sync_confirmed: false,
        };
        let google = ProviderRemoteLocator::GoogleDriveFileId("google-id".into());
        assert_eq!(
            evidence_from_provider_api_json(&receipt, &google, "{}", &digests, 3).unwrap_err(),
            "provider-mismatch"
        );
        let mut wrong = digests;
        wrong.sha256 = "wrong".into();
        assert_eq!(
            evidence_from_provider_api_json(
                &receipt,
                &ProviderRemoteLocator::OneDriveItemId("one-id".into()),
                "{}",
                &wrong,
                3,
            )
            .unwrap_err(),
            "destination-content-mismatch"
        );
    }

    #[test]
    fn json_evidence_binds_both_providers_and_rejects_bad_responses() {
        let destination = Path::new("/destination");
        let one_receipt = receipt(CloudProvider::Onedrive, destination, b"hello-cloud");
        let one_digests = crate::content_digest::digest_bytes(b"hello-cloud");
        let one_locator = ProviderRemoteLocator::OneDriveItemId("one-id".into());
        let one_json = format!(
            r#"{{"id":"one-id","size":11,"eTag":"revision-1","file":{{"hashes":{{"quickXorHash":"{}"}}}}}}"#,
            one_receipt.quick_xor_base64
        );
        assert!(
            evidence_from_provider_api_json(
                &one_receipt,
                &one_locator,
                &one_json,
                &one_digests,
                30,
            )
            .unwrap()
            .sync_complete
        );

        let google_receipt = receipt(CloudProvider::GoogleDrive, destination, b"hello-cloud");
        let google_digests = crate::content_digest::digest_bytes(b"hello-cloud");
        let google_locator = ProviderRemoteLocator::GoogleDriveFileId("google-id".into());
        let google_json = format!(
            r#"{{"id":"google-id","version":"7","size":"11","sha256Checksum":"{}","trashed":false}}"#,
            google_receipt.sha256
        );
        assert!(
            evidence_from_provider_api_json(
                &google_receipt,
                &google_locator,
                &google_json,
                &google_digests,
                30,
            )
            .unwrap()
            .sync_complete
        );

        assert_eq!(
            evidence_from_provider_api_json(
                &one_receipt,
                &one_locator,
                "not-json",
                &one_digests,
                30,
            )
            .unwrap_err(),
            "onedrive-response-invalid"
        );
        assert_eq!(
            evidence_from_provider_api_json(
                &google_receipt,
                &google_locator,
                "not-json",
                &google_digests,
                30,
            )
            .unwrap_err(),
            "google-drive-response-invalid"
        );
        assert_eq!(
            evidence_from_provider_api_json(
                &one_receipt,
                &one_locator,
                r#"{"id":"other-id","size":11,"eTag":"revision-1","file":{"hashes":{"quickXorHash":"unused"}}}"#,
                &one_digests,
                30,
            )
            .unwrap_err(),
            "provider-object-id-mismatch"
        );
    }
}
