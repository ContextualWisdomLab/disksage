//! Authenticated, read-only metadata clients for cloud-provider content proof.
//!
//! Callers supply an ephemeral OAuth access token and a provider-native object identifier or a
//! validated provider path. The production transport only talks to fixed Microsoft Graph or Google
//! Drive API hosts, never persists the token, and never includes it in returned errors.

use crate::cloud::CloudProvider;
use crate::cloud_transfer::{CloudCopyReceipt, ProviderSyncEvidence};
use crate::content_digest::ContentDigests;
#[cfg(not(coverage))]
use crate::content_digest::ContentHasher;
use crate::provider_sync::{parse_google_drive_file_snapshot, parse_onedrive_item_snapshot};
#[cfg(not(coverage))]
use std::io::Read;
use std::path::Path;

const MAX_REMOTE_ID_BYTES: usize = 1_024;
const MAX_REMOTE_PATH_BYTES: usize = 4_096;
const MAX_GOOGLE_DRIVE_PATH_SEGMENTS: usize = 101;
const GOOGLE_DRIVE_FOLDER_MIME_TYPE: &str = "application/vnd.google-apps.folder";
#[cfg(not(coverage))]
const MAX_METADATA_RESPONSE_BYTES: u64 = 256 * 1_024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OneDrivePath(String);

/// An opaque Google Drive file ID paired with the exact My Drive-relative path expected from the
/// receipt destination. Construction validates local path containment and Unicode normalization;
/// collection still has to prove the authenticated remote parent chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoogleDrivePath {
    file_id: String,
    segments: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderRemoteLocator {
    OneDriveItemId(String),
    /// A drive-root-relative path. Unlike an opaque item ID, this binds the response to the exact
    /// receipt destination hierarchy.
    OneDriveItemPath(OneDrivePath),
    GoogleDriveFileId(String),
}

impl ProviderRemoteLocator {
    pub fn provider(&self) -> CloudProvider {
        match self {
            Self::OneDriveItemId(_) | Self::OneDriveItemPath(_) => CloudProvider::Onedrive,
            Self::GoogleDriveFileId(_) => CloudProvider::GoogleDrive,
        }
    }

    pub fn object_id(&self) -> Option<&str> {
        match self {
            Self::OneDriveItemId(id) | Self::GoogleDriveFileId(id) => Some(id),
            Self::OneDriveItemPath(_) => None,
        }
    }

    pub fn location_bound(&self) -> bool {
        matches!(self, Self::OneDriveItemPath(_))
    }

    fn location_proof(&self) -> Option<String> {
        let Self::OneDriveItemPath(path) = self else {
            return None;
        };
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"onedrive-path-v1\0");
        hasher.update(path.0.as_bytes());
        Some(format!("onedrive-path-v1:{}", hasher.finalize().to_hex()))
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
    Ok(match locator {
        ProviderRemoteLocator::OneDriveItemId(object_id)
        | ProviderRemoteLocator::GoogleDriveFileId(object_id) => {
            if object_id.is_empty()
                || object_id.trim() != object_id
                || object_id.len() > MAX_REMOTE_ID_BYTES
                || object_id.bytes().any(|byte| byte.is_ascii_control())
            {
                return Err("provider-object-id-invalid".into());
            }
            let encoded = percent_encode_segment(object_id);
            match locator {
                ProviderRemoteLocator::OneDriveItemId(_) => format!(
                    "https://graph.microsoft.com/v1.0/me/drive/items/{encoded}?%24select=id%2Csize%2CeTag%2Cfile%2Cdeleted"
                ),
                ProviderRemoteLocator::GoogleDriveFileId(_) => format!(
                    "https://www.googleapis.com/drive/v3/files/{encoded}?fields=id%2Cname%2Cparents%2CmimeType%2CdriveId%2Cversion%2Csize%2Csha256Checksum%2Ctrashed&supportsAllDrives=true"
                ),
                ProviderRemoteLocator::OneDriveItemPath(_) => unreachable!(),
            }
        }
        ProviderRemoteLocator::OneDriveItemPath(path) => {
            let path = &path.0;
            if path.is_empty()
                || path.trim() != path
                || path.len() > MAX_REMOTE_PATH_BYTES
                || path.bytes().any(|byte| byte.is_ascii_control())
            {
                return Err("provider-path-invalid".into());
            }
            let segments: Vec<_> = path.split('/').collect();
            if segments
                .iter()
                .any(|segment| segment.is_empty() || matches!(*segment, "." | ".."))
            {
                return Err("provider-path-invalid".into());
            }
            let encoded = segments
                .into_iter()
                .map(percent_encode_segment)
                .collect::<Vec<_>>()
                .join("/");
            format!(
                "https://graph.microsoft.com/v1.0/me/drive/root:/{encoded}?%24select=id%2Csize%2CeTag%2Cfile%2Cdeleted"
            )
        }
    })
}

fn normalized_relative_path_segments(
    local_root: &Path,
    destination: &Path,
) -> Result<Vec<String>, String> {
    use std::path::Component;
    use unicode_normalization::UnicodeNormalization;

    let relative = destination
        .strip_prefix(local_root)
        .map_err(|_| "destination-outside-cloud-root".to_string())?;
    let mut segments = Vec::new();
    for component in relative.components() {
        let Component::Normal(segment) = component else {
            return Err("provider-path-invalid".into());
        };
        let segment = segment
            .to_str()
            .ok_or_else(|| "provider-path-not-unicode".to_string())?;
        let normalized = segment.nfc().collect::<String>();
        if normalized.is_empty()
            || matches!(normalized.as_str(), "." | "..")
            || normalized.bytes().any(|byte| byte.is_ascii_control())
        {
            return Err("provider-path-invalid".into());
        }
        segments.push(normalized);
    }
    if segments.is_empty() || segments.join("/").len() > MAX_REMOTE_PATH_BYTES {
        return Err("provider-path-invalid".into());
    }
    Ok(segments)
}

/// Build a OneDrive drive-root-relative locator from the exact local File Provider root and
/// receipt destination. Parent traversal, non-Unicode segments, and the root itself are rejected.
pub fn onedrive_path_locator(
    local_root: &Path,
    destination: &Path,
) -> Result<ProviderRemoteLocator, String> {
    let segments = normalized_relative_path_segments(local_root, destination)?;
    let locator = ProviderRemoteLocator::OneDriveItemPath(OneDrivePath(segments.join("/")));
    provider_metadata_url(&locator)?;
    Ok(locator)
}

/// Build a My Drive-relative expectation around an operator-supplied Google file ID. The ID alone
/// is never location proof; callers must use the parent-chain collection function below.
pub fn google_drive_path_locator(
    local_root: &Path,
    destination: &Path,
    file_id: &str,
) -> Result<GoogleDrivePath, String> {
    let segments = normalized_relative_path_segments(local_root, destination)?;
    if segments.len() > MAX_GOOGLE_DRIVE_PATH_SEGMENTS {
        return Err("google-drive-path-too-deep".into());
    }
    let id_locator = ProviderRemoteLocator::GoogleDriveFileId(file_id.to_owned());
    provider_metadata_url(&id_locator)?;
    Ok(GoogleDrivePath {
        file_id: file_id.to_owned(),
        segments,
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

#[derive(serde::Deserialize)]
struct GoogleDrivePathItemResponse {
    id: Option<String>,
    name: Option<String>,
    parents: Option<Vec<String>>,
    #[serde(rename = "mimeType")]
    mime_type: Option<String>,
    #[serde(rename = "driveId")]
    drive_id: Option<String>,
    version: Option<String>,
    size: Option<String>,
    #[serde(rename = "sha256Checksum")]
    sha256_checksum: Option<String>,
    trashed: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GoogleDrivePathItem {
    id: String,
    name: String,
    parents: Vec<String>,
    mime_type: String,
    drive_id: Option<String>,
    version: Option<String>,
    size: Option<String>,
    sha256_checksum: Option<String>,
    trashed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GoogleDrivePathProof {
    root: GoogleDrivePathItem,
    /// Target first, followed by its parent folders. The My Drive root is stored separately.
    nodes: Vec<GoogleDrivePathItem>,
}

fn parse_google_drive_path_item(json: &str) -> Result<GoogleDrivePathItem, String> {
    use unicode_normalization::UnicodeNormalization;

    let response: GoogleDrivePathItemResponse =
        serde_json::from_str(json).map_err(|_| "google-drive-path-response-invalid".to_string())?;
    let id = response
        .id
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "google-drive-path-id-missing".to_string())?;
    provider_metadata_url(&ProviderRemoteLocator::GoogleDriveFileId(id.clone()))?;
    let name = response
        .name
        .ok_or_else(|| "google-drive-path-name-missing".to_string())?
        .nfc()
        .collect::<String>();
    if name.is_empty()
        || matches!(name.as_str(), "." | "..")
        || name.contains('/')
        || name.bytes().any(|byte| byte.is_ascii_control())
    {
        return Err("google-drive-path-name-invalid".into());
    }
    let parents = response.parents.unwrap_or_default();
    if parents.len() > 1 {
        return Err("google-drive-multiple-parents-unsupported".into());
    }
    for parent in &parents {
        provider_metadata_url(&ProviderRemoteLocator::GoogleDriveFileId(parent.clone()))?;
    }
    Ok(GoogleDrivePathItem {
        id,
        name,
        parents,
        mime_type: response.mime_type.unwrap_or_default(),
        drive_id: response.drive_id,
        version: response.version,
        size: response.size,
        sha256_checksum: response.sha256_checksum,
        trashed: response.trashed.unwrap_or(false),
    })
}

#[cfg(not(coverage))]
fn fetch_google_drive_path_item(
    requested_id: &str,
    bearer_token: &str,
    transport: &dyn ProviderMetadataTransport,
) -> Result<(GoogleDrivePathItem, String), String> {
    let locator = ProviderRemoteLocator::GoogleDriveFileId(requested_id.to_owned());
    provider_metadata_url(&locator)?;
    let json = transport.fetch_json(&locator, bearer_token)?;
    let item = parse_google_drive_path_item(&json)?;
    if requested_id != "root" && item.id != requested_id {
        return Err("provider-object-id-mismatch".into());
    }
    Ok((item, json))
}

#[cfg(not(coverage))]
fn collect_google_drive_path_pass(
    locator: &GoogleDrivePath,
    bearer_token: &str,
    transport: &dyn ProviderMetadataTransport,
) -> Result<(GoogleDrivePathProof, String), String> {
    use std::collections::HashSet;

    let (root, _) = fetch_google_drive_path_item("root", bearer_token, transport)?;
    if root.trashed || !root.parents.is_empty() || root.mime_type != GOOGLE_DRIVE_FOLDER_MIME_TYPE {
        return Err("google-drive-root-invalid".into());
    }
    if root.drive_id.is_some() {
        return Err("google-drive-shared-drive-unsupported".into());
    }

    let mut current_id = locator.file_id.clone();
    let mut visited = HashSet::new();
    let mut nodes = Vec::with_capacity(locator.segments.len());
    let mut target_json = None;
    for (index, expected_name) in locator.segments.iter().rev().enumerate() {
        if current_id == root.id || !visited.insert(current_id.clone()) {
            return Err("google-drive-parent-chain-invalid".into());
        }
        let (item, json) = fetch_google_drive_path_item(&current_id, bearer_token, transport)?;
        if item.drive_id.is_some() {
            return Err("google-drive-shared-drive-unsupported".into());
        }
        if item.trashed {
            return Err("google-drive-path-item-trashed".into());
        }
        if item.name != *expected_name {
            return Err("google-drive-path-mismatch".into());
        }
        if index > 0 && item.mime_type != GOOGLE_DRIVE_FOLDER_MIME_TYPE {
            return Err("google-drive-parent-not-folder".into());
        }
        let [parent_id] = item.parents.as_slice() else {
            return Err("google-drive-parent-chain-invalid".into());
        };
        if index == 0 {
            target_json = Some(json);
        }
        current_id = parent_id.clone();
        nodes.push(item);
    }
    if current_id != root.id {
        return Err("google-drive-path-mismatch".into());
    }
    Ok((
        GoogleDrivePathProof { root, nodes },
        target_json.ok_or_else(|| "google-drive-target-response-missing".to_string())?,
    ))
}

fn google_drive_location_proof(locator: &GoogleDrivePath, proof: &GoogleDrivePathProof) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"google-drive-parent-chain-v1\0");
    hasher.update(locator.file_id.as_bytes());
    hasher.update(&[0]);
    for segment in &locator.segments {
        hasher.update(segment.as_bytes());
        hasher.update(&[0]);
    }
    for value in [
        proof.root.id.as_str(),
        proof.root.name.as_str(),
        proof.root.mime_type.as_str(),
        proof.root.version.as_deref().unwrap_or_default(),
    ] {
        hasher.update(value.as_bytes());
        hasher.update(&[0]);
    }
    for node in &proof.nodes {
        for value in [
            node.id.as_str(),
            node.name.as_str(),
            node.mime_type.as_str(),
            node.version.as_deref().unwrap_or_default(),
        ] {
            hasher.update(value.as_bytes());
            hasher.update(&[0]);
        }
        for parent in &node.parents {
            hasher.update(parent.as_bytes());
            hasher.update(&[0]);
        }
    }
    format!(
        "google-drive-parent-chain-v1:{}",
        hasher.finalize().to_hex()
    )
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
        ProviderRemoteLocator::OneDriveItemId(_) | ProviderRemoteLocator::OneDriveItemPath(_) => {
            parse_onedrive_item_snapshot(json, &destination_digests.blake3)?
        }
        ProviderRemoteLocator::GoogleDriveFileId(_) => {
            parse_google_drive_file_snapshot(json, &destination_digests.blake3)?
        }
    };
    if locator
        .object_id()
        .is_some_and(|object_id| snapshot.remote_object_id != object_id)
    {
        return Err("provider-object-id-mismatch".into());
    }
    let location_proof = locator.location_proof();
    crate::provider_sync::evidence_from_provider_api_snapshot_with_location(
        receipt,
        &snapshot,
        location_proof.as_deref(),
        confirmed_at_ms,
    )
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

/// Fetch authenticated remote metadata while anchoring the content proof to the source that may be
/// evicted. This avoids reading or hydrating a cloud placeholder merely to prove remote content.
/// The source is hashed before and after the fixed-host API request and must remain identical to the
/// immutable receipt throughout.
#[cfg(not(coverage))]
pub fn collect_authenticated_provider_api_evidence_from_source(
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
    let source = Path::new(&receipt.source);
    if !source.is_absolute() {
        return Err("source-path-not-absolute".into());
    }
    let source_error = |error: String| {
        error
            .strip_prefix("destination-")
            .map(|suffix| format!("source-{suffix}"))
            .unwrap_or(error)
    };
    let (before_identity, before_digests) = stable_local_snapshot(source).map_err(&source_error)?;
    if before_identity.bytes != receipt.bytes || !digests_match_receipt(&before_digests, receipt) {
        return Err("source-content-mismatch".into());
    }
    let json = transport.fetch_json(locator, bearer_token)?;
    let (after_identity, after_digests) = stable_local_snapshot(source).map_err(source_error)?;
    if after_identity != before_identity || after_digests != before_digests {
        return Err("source-changed-during-provider-check".into());
    }
    evidence_from_provider_api_json(receipt, locator, &json, &after_digests, confirmed_at_ms)
}

/// Prove a Google Drive object's exact My Drive-relative location by reading the target and every
/// parent to the authenticated `root` alias twice. The two normalized chains must be identical, and
/// the source that may later be evicted must remain byte-identical to the immutable receipt.
#[cfg(not(coverage))]
pub fn collect_authenticated_google_drive_path_evidence_from_source(
    receipt: &CloudCopyReceipt,
    locator: &GoogleDrivePath,
    bearer_token: &str,
    transport: &dyn ProviderMetadataTransport,
    confirmed_at_ms: u64,
) -> Result<ProviderSyncEvidence, String> {
    if bearer_token.trim().is_empty() {
        return Err("provider-access-token-missing".into());
    }
    if receipt.provider != CloudProvider::GoogleDrive {
        return Err("provider-mismatch".into());
    }
    provider_metadata_url(&ProviderRemoteLocator::GoogleDriveFileId(
        locator.file_id.clone(),
    ))?;
    let source = Path::new(&receipt.source);
    if !source.is_absolute() {
        return Err("source-path-not-absolute".into());
    }
    let source_error = |error: String| {
        error
            .strip_prefix("destination-")
            .map(|suffix| format!("source-{suffix}"))
            .unwrap_or(error)
    };
    let (before_identity, before_digests) = stable_local_snapshot(source).map_err(&source_error)?;
    if before_identity.bytes != receipt.bytes || !digests_match_receipt(&before_digests, receipt) {
        return Err("source-content-mismatch".into());
    }

    let (first_path, _) = collect_google_drive_path_pass(locator, bearer_token, transport)?;
    let (second_path, target_json) =
        collect_google_drive_path_pass(locator, bearer_token, transport)?;
    if first_path != second_path {
        return Err("google-drive-hierarchy-changed-during-provider-check".into());
    }

    let (after_identity, after_digests) = stable_local_snapshot(source).map_err(source_error)?;
    if after_identity != before_identity || after_digests != before_digests {
        return Err("source-changed-during-provider-check".into());
    }
    let snapshot = parse_google_drive_file_snapshot(&target_json, &after_digests.blake3)?;
    if snapshot.remote_object_id != locator.file_id {
        return Err("provider-object-id-mismatch".into());
    }
    let location_proof = google_drive_location_proof(locator, &second_path);
    crate::provider_sync::evidence_from_provider_api_snapshot_with_location(
        receipt,
        &snapshot,
        Some(&location_proof),
        confirmed_at_ms,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud_transfer::LEGACY_RECEIPT_VERSION;
    #[cfg(not(coverage))]
    use std::cell::{Cell, RefCell};
    #[cfg(not(coverage))]
    use std::collections::HashMap;
    #[cfg(not(coverage))]
    use std::path::PathBuf;

    fn receipt(provider: CloudProvider, destination: &Path, bytes: &[u8]) -> CloudCopyReceipt {
        let digests = crate::content_digest::digest_bytes(bytes);
        CloudCopyReceipt {
            version: LEGACY_RECEIPT_VERSION,
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
            lineage_fingerprint: None,
            lineage: None,
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

    #[cfg(not(coverage))]
    struct RoutedTransport {
        responses: RefCell<HashMap<String, Vec<String>>>,
        calls: RefCell<Vec<String>>,
    }

    #[cfg(not(coverage))]
    impl ProviderMetadataTransport for RoutedTransport {
        fn fetch_json(
            &self,
            locator: &ProviderRemoteLocator,
            bearer_token: &str,
        ) -> Result<String, String> {
            assert_eq!(bearer_token, "secret-token");
            let id = locator
                .object_id()
                .expect("routed transport accepts ID lookups only")
                .to_owned();
            self.calls.borrow_mut().push(id.clone());
            let mut responses = self.responses.borrow_mut();
            let values = responses
                .get_mut(&id)
                .ok_or_else(|| format!("missing-test-response:{id}"))?;
            if values.len() > 1 {
                Ok(values.remove(0))
            } else {
                values
                    .first()
                    .cloned()
                    .ok_or_else(|| "empty-test-response".into())
            }
        }
    }

    #[cfg(not(coverage))]
    fn routed_transport(entries: Vec<(&str, Vec<String>)>) -> RoutedTransport {
        RoutedTransport {
            responses: RefCell::new(
                entries
                    .into_iter()
                    .map(|(id, values)| (id.to_owned(), values))
                    .collect(),
            ),
            calls: RefCell::new(Vec::new()),
        }
    }

    #[cfg(not(coverage))]
    fn google_folder(id: &str, name: &str, parent: Option<&str>, version: &str) -> String {
        let parents = parent
            .map(|value| format!(r#"["{value}"]"#))
            .unwrap_or_else(|| "[]".into());
        format!(
            r#"{{"id":"{id}","name":"{name}","parents":{parents},"mimeType":"{GOOGLE_DRIVE_FOLDER_MIME_TYPE}","version":"{version}","trashed":false}}"#
        )
    }

    #[cfg(not(coverage))]
    fn google_file(checksum: &str, name: &str, parent: &str, drive_id: Option<&str>) -> String {
        let drive_id = drive_id
            .map(|value| format!(r#","driveId":"{value}""#))
            .unwrap_or_default();
        format!(
            r#"{{"id":"google-id","name":"{name}","parents":["{parent}"],"mimeType":"application/pdf","version":"7","size":"11","sha256Checksum":"{checksum}","trashed":false{drive_id}}}"#
        )
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
            "https://www.googleapis.com/drive/v3/files/g%2Fid?fields=id%2Cname%2Cparents%2CmimeType%2CdriveId%2Cversion%2Csize%2Csha256Checksum%2Ctrashed&supportsAllDrives=true"
        );
        assert_eq!(
            provider_metadata_url(&ProviderRemoteLocator::OneDriveItemPath(
                OneDrivePath("DiskSage Archive/보고서 #1.pdf".into())
            ))
            .unwrap(),
            "https://graph.microsoft.com/v1.0/me/drive/root:/DiskSage%20Archive/%EB%B3%B4%EA%B3%A0%EC%84%9C%20%231.pdf?%24select=id%2Csize%2CeTag%2Cfile%2Cdeleted"
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
        for path in ["", "/leading", "trailing/", "a/../b", "line\nbreak"] {
            assert_eq!(
                provider_metadata_url(&ProviderRemoteLocator::OneDriveItemPath(OneDrivePath(
                    path.into()
                )))
                .unwrap_err(),
                "provider-path-invalid"
            );
        }

        let root = Path::new("/cloud");
        let locator = onedrive_path_locator(
            root,
            Path::new("/cloud/DiskSage Archive/e\u{301}vidence.pdf"),
        )
        .unwrap();
        assert_eq!(
            locator,
            ProviderRemoteLocator::OneDriveItemPath(OneDrivePath(
                "DiskSage Archive/évidence.pdf".into()
            ))
        );
        assert!(locator.location_bound());
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
        assert!(!evidence.remote_content.unwrap().location_bound);
        assert_eq!(remote.calls.get(), 1);

        let path_locator = ProviderRemoteLocator::OneDriveItemPath(OneDrivePath(
            "DiskSage Archive/2026/report.pdf".into(),
        ));
        let path_remote = transport(&format!(
            r#"{{"id":"one-id","size":11,"eTag":"revision-1","file":{{"hashes":{{"quickXorHash":"{}"}}}}}}"#,
            receipt.quick_xor_base64
        ));
        let path_evidence = collect_authenticated_provider_api_evidence(
            &receipt,
            &path_locator,
            "secret-token",
            &path_remote,
            30,
        )
        .unwrap();
        assert!(path_evidence.sync_complete);
        assert!(path_evidence.remote_content.unwrap().location_bound);
    }

    #[test]
    #[cfg(not(coverage))]
    fn source_anchored_collection_does_not_read_cloud_placeholder() {
        let temporary = tempfile::tempdir().unwrap();
        let source = temporary.path().join("source.pdf");
        let cloud_placeholder = temporary.path().join("cloud-placeholder.pdf");
        std::fs::write(&source, b"hello-cloud").unwrap();
        let mut receipt = receipt(CloudProvider::Onedrive, &cloud_placeholder, b"hello-cloud");
        receipt.source = source.to_string_lossy().into_owned();
        let locator = ProviderRemoteLocator::OneDriveItemPath(OneDrivePath(
            "DiskSage Archive/report.pdf".into(),
        ));
        let remote = transport(&format!(
            r#"{{"id":"one-id","size":11,"eTag":"revision-1","file":{{"hashes":{{"quickXorHash":"{}"}}}}}}"#,
            receipt.quick_xor_base64
        ));

        let evidence = collect_authenticated_provider_api_evidence_from_source(
            &receipt,
            &locator,
            "secret-token",
            &remote,
            30,
        )
        .unwrap();
        assert!(evidence.sync_complete);
        assert!(evidence.remote_content.unwrap().location_bound);
        assert!(!cloud_placeholder.exists());
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
    fn google_parent_chain_binds_exact_path_and_source_without_hydrating_destination() {
        let temporary = tempfile::tempdir().unwrap();
        let cloud_root = temporary.path().join("내 드라이브");
        let destination = cloud_root.join("DiskSage Archive/2026/report.pdf");
        let source = temporary.path().join("source.pdf");
        std::fs::write(&source, b"hello-cloud").unwrap();
        let mut receipt = receipt(CloudProvider::GoogleDrive, &destination, b"hello-cloud");
        receipt.source = source.to_string_lossy().into_owned();
        let locator = google_drive_path_locator(&cloud_root, &destination, "google-id").unwrap();
        let remote = routed_transport(vec![
            (
                "root",
                vec![google_folder("root-id", "내 드라이브", None, "1")],
            ),
            (
                "archive-id",
                vec![google_folder(
                    "archive-id",
                    "DiskSage Archive",
                    Some("root-id"),
                    "2",
                )],
            ),
            (
                "year-id",
                vec![google_folder("year-id", "2026", Some("archive-id"), "3")],
            ),
            (
                "google-id",
                vec![google_file(&receipt.sha256, "report.pdf", "year-id", None)],
            ),
        ]);

        let evidence = collect_authenticated_google_drive_path_evidence_from_source(
            &receipt,
            &locator,
            "secret-token",
            &remote,
            30,
        )
        .unwrap();
        assert!(evidence.sync_complete);
        let content = evidence.remote_content.unwrap();
        assert!(content.location_bound);
        assert!(content
            .location_proof
            .as_deref()
            .unwrap()
            .starts_with("google-drive-parent-chain-v1:"));
        assert_eq!(remote.calls.borrow().len(), 8);
        assert!(!destination.exists());
    }

    #[test]
    #[cfg(not(coverage))]
    fn google_parent_chain_rejects_path_substitution_and_shared_drive() {
        let temporary = tempfile::tempdir().unwrap();
        let cloud_root = temporary.path().join("My Drive");
        let destination = cloud_root.join("DiskSage Archive/report.pdf");
        let source = temporary.path().join("source.pdf");
        std::fs::write(&source, b"hello-cloud").unwrap();
        let mut receipt = receipt(CloudProvider::GoogleDrive, &destination, b"hello-cloud");
        receipt.source = source.to_string_lossy().into_owned();
        let locator = google_drive_path_locator(&cloud_root, &destination, "google-id").unwrap();

        let substituted = routed_transport(vec![
            (
                "root",
                vec![google_folder("root-id", "My Drive", None, "1")],
            ),
            (
                "google-id",
                vec![google_file(&receipt.sha256, "other.pdf", "root-id", None)],
            ),
        ]);
        assert_eq!(
            collect_authenticated_google_drive_path_evidence_from_source(
                &receipt,
                &locator,
                "secret-token",
                &substituted,
                30,
            )
            .unwrap_err(),
            "google-drive-path-mismatch"
        );

        let shared = routed_transport(vec![
            (
                "root",
                vec![google_folder("root-id", "My Drive", None, "1")],
            ),
            (
                "google-id",
                vec![google_file(
                    &receipt.sha256,
                    "report.pdf",
                    "root-id",
                    Some("shared-drive-id"),
                )],
            ),
        ]);
        assert_eq!(
            collect_authenticated_google_drive_path_evidence_from_source(
                &receipt,
                &locator,
                "secret-token",
                &shared,
                30,
            )
            .unwrap_err(),
            "google-drive-shared-drive-unsupported"
        );
    }

    #[test]
    #[cfg(not(coverage))]
    fn google_parent_chain_rejects_hierarchy_drift_and_multiple_parents() {
        let temporary = tempfile::tempdir().unwrap();
        let cloud_root = temporary.path().join("My Drive");
        let destination = cloud_root.join("DiskSage Archive/2026/report.pdf");
        let source = temporary.path().join("source.pdf");
        std::fs::write(&source, b"hello-cloud").unwrap();
        let mut receipt = receipt(CloudProvider::GoogleDrive, &destination, b"hello-cloud");
        receipt.source = source.to_string_lossy().into_owned();
        let locator = google_drive_path_locator(&cloud_root, &destination, "google-id").unwrap();
        let drifting = routed_transport(vec![
            (
                "root",
                vec![google_folder("root-id", "My Drive", None, "1")],
            ),
            (
                "archive-id",
                vec![google_folder(
                    "archive-id",
                    "DiskSage Archive",
                    Some("root-id"),
                    "2",
                )],
            ),
            (
                "year-id",
                vec![
                    google_folder("year-id", "2026", Some("archive-id"), "3"),
                    google_folder("year-id", "2026", Some("archive-id"), "4"),
                ],
            ),
            (
                "google-id",
                vec![google_file(&receipt.sha256, "report.pdf", "year-id", None)],
            ),
        ]);
        assert_eq!(
            collect_authenticated_google_drive_path_evidence_from_source(
                &receipt,
                &locator,
                "secret-token",
                &drifting,
                30,
            )
            .unwrap_err(),
            "google-drive-hierarchy-changed-during-provider-check"
        );

        let multiple = r#"{"id":"google-id","name":"report.pdf","parents":["a","b"],"mimeType":"application/pdf"}"#;
        assert_eq!(
            parse_google_drive_path_item(multiple).unwrap_err(),
            "google-drive-multiple-parents-unsupported"
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
            version: LEGACY_RECEIPT_VERSION,
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
            lineage_fingerprint: None,
            lineage: None,
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
