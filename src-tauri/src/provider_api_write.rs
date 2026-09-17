//! Explicit OAuth write paths for provider copies when a desktop File Provider is unavailable.
//!
//! This module never accepts a token from the UI. Callers obtain a short-lived access token from
//! `provider_oauth`, bind the upload to the already-reviewed local source/destination, and keep the
//! The returned object id is bound into the immutable provider-evidence record by the normal
//! attestation path; subsequent checks still re-prove it against the source and remote metadata.

use crate::cloud::CloudProvider;
use crate::provider_api_client::{onedrive_path_locator, ProviderRemoteLocator};
use serde::Deserialize;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

const ONEDRIVE_GRAPH_ROOT: &str = "https://graph.microsoft.com/v1.0/me/drive/root";
const ONEDRIVE_GRAPH_ITEMS: &str = "https://graph.microsoft.com/v1.0/me/drive/items";
const GOOGLE_FILES: &str = "https://www.googleapis.com/drive/v3/files";
const GOOGLE_UPLOAD_FILES: &str = "https://www.googleapis.com/upload/drive/v3/files";
const MAX_BEARER_TOKEN_BYTES: usize = 64 * 1024;
const MAX_RESPONSE_BYTES: u64 = 256 * 1024;
const MAX_NEXT_EXPECTED_RANGES: usize = 128;
const MAX_UPLOAD_NO_PROGRESS_RESPONSES: usize = 3;
const GOOGLE_CHUNK_BYTES: usize = 8 * 1024 * 1024;
const ONEDRIVE_CHUNK_BYTES: usize = 320 * 1024 * 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderApiUploadResult {
    pub provider: CloudProvider,
    pub object_id: String,
    pub locator: ProviderRemoteLocator,
}

/// Command-owned cancellation and overall deadline for one provider upload.
pub struct ProviderUploadControl<'a> {
    cancel: &'a AtomicBool,
    deadline: Instant,
}

impl<'a> ProviderUploadControl<'a> {
    pub fn new(cancel: &'a AtomicBool, deadline: Instant) -> Self { Self { cancel, deadline } }
    fn check_at(&self, now: Instant) -> Result<(), String> {
        if self.cancel.load(Ordering::SeqCst) { return Err("cloud-copy-cancelled".into()); }
        if now >= self.deadline { return Err("cloud-copy-deadline-exceeded".into()); }
        Ok(())
    }
    fn check(&self) -> Result<(), String> { self.check_at(Instant::now()) }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UploadResponseKind {
    Complete,
    Progress,
}

#[derive(Debug, Deserialize)]
struct UploadedItem {
    id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GoogleFileList {
    files: Vec<GoogleFileEntry>,
}

#[derive(Debug, Deserialize)]
struct GoogleFileEntry {
    id: Option<String>,
    #[serde(rename = "mimeType")]
    mime_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OneDriveUploadSession {
    #[serde(rename = "uploadUrl")]
    upload_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OneDriveUploadProgress {
    #[serde(rename = "nextExpectedRanges")]
    next_expected_ranges: Vec<String>,
}

fn validate_bearer_token(token: &str) -> Result<(), String> {
    if token.is_empty()
        || token.len() > MAX_BEARER_TOKEN_BYTES
        || token.bytes().any(|byte| byte.is_ascii_control())
    {
        return Err("provider-api-bearer-token-invalid".into());
    }
    Ok(())
}

fn agent() -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .https_only(true)
        .max_redirects(0)
        .timeout_global(Some(std::time::Duration::from_secs(60)))
        .build();
    ureq::Agent::new_with_config(config)
}

fn safe_transport_error(error: ureq::Error) -> String {
    match error {
        ureq::Error::StatusCode(code) => format!("provider-api-http-status:{code}"),
        ureq::Error::Timeout(_) => "provider-api-timeout".into(),
        ureq::Error::HostNotFound => "provider-api-host-not-found".into(),
        ureq::Error::BodyExceedsLimit(_) => "provider-api-response-too-large".into(),
        _ => "provider-api-request-failed".into(),
    }
}

fn read_json<T: for<'de> Deserialize<'de>>(
    response: &mut ureq::http::Response<ureq::Body>,
) -> Result<T, String> {
    if !(200..300).contains(&response.status().as_u16()) {
        return Err(format!(
            "provider-api-http-status:{}",
            response.status().as_u16()
        ));
    }
    let body = response
        .body_mut()
        .with_config()
        .limit(MAX_RESPONSE_BYTES)
        .read_to_string()
        .map_err(safe_transport_error)?;
    serde_json::from_str(&body).map_err(|_| "provider-api-response-invalid".into())
}

fn drain_response_body(response: &mut ureq::http::Response<ureq::Body>) -> Result<(), String> {
    response
        .body_mut()
        .with_config()
        .limit(MAX_RESPONSE_BYTES)
        .read_to_vec()
        .map_err(safe_transport_error)
        .map(|_| ())
}

fn read_response_body(response: &mut ureq::http::Response<ureq::Body>) -> Result<(), String> {
    if !(200..300).contains(&response.status().as_u16()) {
        return Err(format!(
            "provider-api-http-status:{}",
            response.status().as_u16()
        ));
    }
    drain_response_body(response)
}

fn classify_upload_response_status(status: u16) -> Result<UploadResponseKind, String> {
    match status {
        200 | 201 => Ok(UploadResponseKind::Complete),
        202 | 308 => Ok(UploadResponseKind::Progress),
        _ => Err(format!("provider-api-http-status:{status}")),
    }
}

fn parse_onedrive_next_expected_offset(body: &str, sent_end: u64) -> Result<u64, String> {
    let progress: OneDriveUploadProgress =
        serde_json::from_str(body).map_err(|_| "provider-api-upload-progress-invalid".to_string())?;
    if progress.next_expected_ranges.is_empty()
        || progress.next_expected_ranges.len() > MAX_NEXT_EXPECTED_RANGES
    {
        return Err("provider-api-upload-next-range-required".into());
    }
    let max_next_offset = sent_end.saturating_add(1);
    progress
        .next_expected_ranges
        .iter()
        .map(|range| {
            let start = range
                .split_once('-')
                .map(|(start, _)| start)
                .unwrap_or(range.as_str());
            if start.is_empty() || !start.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err("provider-api-upload-next-range-invalid".to_string());
            }
            start
                .parse::<u64>()
                .ok()
                .filter(|offset| *offset <= max_next_offset)
                .ok_or_else(|| "provider-api-upload-next-range-invalid".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .min()
        .ok_or_else(|| "provider-api-upload-next-range-required".into())
}

fn read_onedrive_progress_offset(
    response: &mut ureq::http::Response<ureq::Body>,
    sent_end: u64,
) -> Result<u64, String> {
    let body = response
        .body_mut()
        .with_config()
        .limit(MAX_RESPONSE_BYTES)
        .read_to_string()
        .map_err(safe_transport_error)?;
    parse_onedrive_next_expected_offset(&body, sent_end)
}

fn google_next_upload_offset(range: Option<&str>, sent_end: u64) -> Result<u64, String> {
    if let Some(range) = range {
        let acknowledged_end = range
            .strip_prefix("bytes=")
            .and_then(|value| value.rsplit_once('-'))
            .and_then(|(_, value)| value.parse::<u64>().ok())
            .filter(|acknowledged_end| *acknowledged_end <= sent_end)
            .ok_or_else(|| "provider-api-upload-range-invalid".to_string())?;
        return Ok(acknowledged_end.saturating_add(1));
    }
    // Google Drive specifies that a 308 without Range means the session has committed no bytes.
    Ok(0)
}

fn guard_upload_progress(
    proposed_offset: u64,
    highest_offset: &mut u64,
    no_progress_responses: &mut usize,
) -> Result<u64, String> {
    if proposed_offset > *highest_offset {
        *highest_offset = proposed_offset;
        *no_progress_responses = 0;
    } else {
        *no_progress_responses = no_progress_responses.saturating_add(1);
        if *no_progress_responses >= MAX_UPLOAD_NO_PROGRESS_RESPONSES {
            return Err("provider-api-upload-no-progress".into());
        }
    }
    Ok(proposed_offset)
}

fn response_location(response: &ureq::http::Response<ureq::Body>) -> Result<String, String> {
    response
        .headers()
        .get("Location")
        .and_then(|value| value.to_str().ok())
        .filter(|value| value.starts_with("https://"))
        .filter(|value| !value.bytes().any(|byte| byte.is_ascii_control()))
        .map(str::to_owned)
        .ok_or_else(|| "provider-api-upload-session-location-missing".into())
}

fn validate_local_source(source: &Path, expected_bytes: u64) -> Result<std::fs::File, String> {
    let metadata = std::fs::symlink_metadata(source).map_err(|_| "source-unavailable".to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("source-must-be-regular-file".into());
    }
    if metadata.len() != expected_bytes {
        return Err("source-size-changed-before-provider-upload".into());
    }
    std::fs::File::open(source).map_err(|_| "source-unreadable".into())
}

fn percent_encode_segment(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            use std::fmt::Write;
            write!(&mut encoded, "%{byte:02X}").expect("String formatting cannot fail");
        }
    }
    encoded
}

fn onedrive_session_url(relative_path: &str) -> String {
    let path = relative_path
        .split('/')
        .map(percent_encode_segment)
        .collect::<Vec<_>>()
        .join("/");
    format!("{ONEDRIVE_GRAPH_ROOT}:/{path}:/createUploadSession")
}

fn onedrive_metadata_url(relative_path: &str) -> String {
    let path = relative_path
        .split('/')
        .map(percent_encode_segment)
        .collect::<Vec<_>>()
        .join("/");
    format!("{ONEDRIVE_GRAPH_ROOT}:/{path}")
}

fn google_query_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\'', "\\'")
}

fn google_list_children(
    agent: &ureq::Agent,
    token: &str,
    parent_id: &str,
    name: &str,
) -> Result<Vec<GoogleFileEntry>, String> {
    let query = format!(
        "'{parent_id}' in parents and name = '{}' and trashed = false",
        google_query_escape(name)
    );
    let mut response = agent
        .get(GOOGLE_FILES)
        .query("q", query)
        .query("spaces", "drive")
        .query("pageSize", "100")
        .query("fields", "files(id,mimeType)")
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/json")
        .call()
        .map_err(safe_transport_error)?;
    let list: GoogleFileList = read_json(&mut response)?;
    Ok(list.files)
}

fn google_create_folder(
    agent: &ureq::Agent,
    token: &str,
    parent_id: &str,
    name: &str,
) -> Result<String, String> {
    let metadata = serde_json::json!({
        "name": name,
        "mimeType": "application/vnd.google-apps.folder",
        "parents": [parent_id]
    });
    let body = serde_json::to_vec(&metadata)
        .map_err(|_| "provider-api-request-encode-failed".to_string())?;
    let mut response = agent
        .post(GOOGLE_FILES)
        .query("supportsAllDrives", "false")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .send(body)
        .map_err(safe_transport_error)?;
    let item: UploadedItem = read_json(&mut response)?;
    item.id
        .filter(|id| !id.trim().is_empty())
        .ok_or_else(|| "provider-api-folder-id-missing".into())
}

fn google_parent_id(
    agent: &ureq::Agent,
    token: &str,
    segments: &[String],
) -> Result<String, String> {
    let mut parent = "root".to_owned();
    for segment in segments {
        let matches = google_list_children(agent, token, &parent, segment)?;
        match matches.as_slice() {
            [] => parent = google_create_folder(agent, token, &parent, segment)?,
            [only] => {
                if only.mime_type.as_deref()
                    != Some("application/vnd.google-apps.folder")
                {
                    return Err("provider-api-parent-is-not-folder".into());
                }
                parent = only
                    .id
                    .clone()
                    .filter(|id| !id.trim().is_empty())
                    .ok_or_else(|| "provider-api-folder-id-missing".to_string())?;
            }
            _ => return Err("provider-api-parent-ambiguous".into()),
        }
    }
    Ok(parent)
}

fn google_upload_session(
    agent: &ureq::Agent,
    token: &str,
    parent_id: &str,
    name: &str,
    bytes: u64,
) -> Result<String, String> {
    let metadata = serde_json::json!({"name": name, "parents": [parent_id]});
    let body = serde_json::to_vec(&metadata)
        .map_err(|_| "provider-api-request-encode-failed".to_string())?;
    let response = agent
        .post(GOOGLE_UPLOAD_FILES)
        .query("uploadType", "resumable")
        .query("supportsAllDrives", "false")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json; charset=UTF-8")
        .header("X-Upload-Content-Type", "application/octet-stream")
        .header("X-Upload-Content-Length", bytes.to_string())
        .send(body)
        .map_err(safe_transport_error)?;
    if !(200..300).contains(&response.status().as_u16()) {
        return Err(format!(
            "provider-api-http-status:{}",
            response.status().as_u16()
        ));
    }
    response_location(&response)
}

fn read_source_chunk_at(
    source: &mut std::fs::File,
    offset: u64,
    buffer: &mut [u8],
) -> Result<(), String> {
    source
        .seek(SeekFrom::Start(offset))
        .map_err(|_| "source-seek-failed-during-provider-upload".to_string())?;
    source
        .read_exact(buffer)
        .map_err(|_| "source-read-failed-during-provider-upload".to_string())
}

fn upload_chunks(
    agent: &ureq::Agent,
    session_url: &str,
    mut source: std::fs::File,
    bytes: u64,
    chunk_bytes: usize,
    authorization: Option<&str>,
    control: &ProviderUploadControl<'_>,
) -> Result<String, String> {
    let mut offset = 0_u64;
    let mut highest_offset = 0_u64;
    let mut no_progress_responses = 0_usize;
    let mut buffer = vec![0_u8; chunk_bytes];
    while offset < bytes {
        control.check()?;
        let want = (bytes - offset).min(chunk_bytes as u64) as usize;
        read_source_chunk_at(&mut source, offset, &mut buffer[..want])?;
        let end = offset + want as u64 - 1;
        let content_range = format!("bytes {offset}-{end}/{bytes}");
        let mut request = agent
            .put(session_url)
            .header("Content-Length", want.to_string())
            .header("Content-Range", content_range)
            .header("Content-Type", "application/octet-stream");
        if let Some(authorization) = authorization {
            request = request.header("Authorization", authorization);
        }
        let mut response = request
            .send(&buffer[..want])
            .map_err(safe_transport_error)?;
        let status = response.status().as_u16();
        match classify_upload_response_status(status)? {
            UploadResponseKind::Complete => {
                let item: UploadedItem = read_json(&mut response)?;
                return item
                    .id
                    .filter(|id| !id.trim().is_empty())
                    .ok_or_else(|| "provider-api-upload-object-id-missing".into());
            }
            UploadResponseKind::Progress if status == 202 => {
                let proposed_offset = read_onedrive_progress_offset(&mut response, end)?;
                offset = guard_upload_progress(
                    proposed_offset,
                    &mut highest_offset,
                    &mut no_progress_responses,
                )?;
            }
            UploadResponseKind::Progress => {
                let range = response.headers().get("Range").and_then(|value| value.to_str().ok());
                let proposed_offset = google_next_upload_offset(range, end)?;
                drain_response_body(&mut response)?;
                offset = guard_upload_progress(
                    proposed_offset,
                    &mut highest_offset,
                    &mut no_progress_responses,
                )?;
            }
        }
    }
    Err("provider-api-upload-completion-missing".into())
}

fn abandon_upload_session(agent: &ureq::Agent, session_url: &str, authorization: Option<&str>) -> Result<(), String> {
    let mut request = agent.delete(session_url);
    if let Some(authorization) = authorization { request = request.header("Authorization", authorization); }
    match request.call() {
        Ok(mut response) => read_response_body(&mut response),
        Err(ureq::Error::StatusCode(404 | 410)) => Ok(()),
        Err(error) => Err(safe_transport_error(error)),
    }
}

fn preserve_upload_error_with_session_cleanup(agent: &ureq::Agent, session_url: &str, authorization: Option<&str>, upload: Result<String, String>) -> Result<String, String> {
    match upload {
        Ok(object_id) => Ok(object_id),
        Err(error) => match abandon_upload_session(agent, session_url, authorization) {
            Ok(()) => Err(error),
            Err(cleanup_error) => Err(format!("{error},provider-api-upload-session-cleanup-failed:{cleanup_error}")),
        },
    }
}

fn one_drive_upload(
    agent: &ureq::Agent,
    token: &str,
    source: &Path,
    relative_path: &str,
    bytes: u64,
    control: &ProviderUploadControl<'_>,
) -> Result<String, String> {
    let metadata_probe = agent
        .get(onedrive_metadata_url(relative_path))
        .query("%24select", "id")
        .header("Authorization", format!("Bearer {token}"))
        .call();
    match metadata_probe {
        Ok(mut response) => {
            let _ = read_response_body(&mut response);
            return Err("provider-api-destination-already-exists".into());
        }
        Err(ureq::Error::StatusCode(404)) => {}
        Err(error) => return Err(safe_transport_error(error)),
    }
    let session_body = serde_json::json!({
        "item": {"@microsoft.graph.conflictBehavior": "fail"}
    });
    let body = serde_json::to_vec(&session_body)
        .map_err(|_| "provider-api-request-encode-failed".to_string())?;
    let mut session = agent
        .post(onedrive_session_url(relative_path))
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .send(body)
        .map_err(safe_transport_error)?;
    let session: OneDriveUploadSession = read_json(&mut session)?;
    let upload_url = session
        .upload_url
        .filter(|url| url.starts_with("https://") && !url.bytes().any(|byte| byte.is_ascii_control()))
        .ok_or_else(|| "provider-api-upload-session-url-invalid".to_string())?;
    let source_file = validate_local_source(source, bytes)?;
    let upload = upload_chunks(
        agent,
        &upload_url,
        source_file,
        bytes,
        ONEDRIVE_CHUNK_BYTES,
        None,
        control,
    );
    preserve_upload_error_with_session_cleanup(agent, &upload_url, None, upload)
}

fn google_upload(
    agent: &ureq::Agent,
    token: &str,
    source: &Path,
    destination: &Path,
    local_root: &Path,
    bytes: u64,
    control: &ProviderUploadControl<'_>,
) -> Result<String, String> {
    let segments = crate::provider_api_client::destination_path_segments(local_root, destination)?;
    let (name, folders) = segments
        .split_last()
        .ok_or_else(|| "provider-api-destination-path-invalid".to_string())?;
    let parent = google_parent_id(agent, token, folders)?;
    if !google_list_children(agent, token, &parent, name)?.is_empty() {
        return Err("provider-api-destination-already-exists".into());
    }
    let session = google_upload_session(agent, token, &parent, name, bytes)?;
    let source_file = validate_local_source(source, bytes)?;
    let authorization = format!("Bearer {token}");
    let upload = upload_chunks(agent, &session, source_file, bytes, GOOGLE_CHUNK_BYTES, Some(&authorization), control);
    preserve_upload_error_with_session_cleanup(agent, &session, Some(&authorization), upload)
}

pub fn upload_file(
    provider: CloudProvider,
    local_root: &Path,
    destination: &Path,
    source: &Path,
    bytes: u64,
    bearer_token: &str,
    control: &ProviderUploadControl<'_>,
) -> Result<ProviderApiUploadResult, String> {
    control.check()?;
    validate_bearer_token(bearer_token)?;
    let agent = agent();
    let object_id = match provider {
        CloudProvider::Onedrive => {
            let locator = onedrive_path_locator(local_root, destination)?;
            let path = locator
                .onedrive_path()
                .ok_or_else(|| "provider-api-path-locator-invalid".to_string())?;
            one_drive_upload(&agent, bearer_token, source, path, bytes, control)?
        }
        CloudProvider::GoogleDrive => {
            google_upload(&agent, bearer_token, source, destination, local_root, bytes, control)?
        }
        CloudProvider::Icloud => return Err("provider-api-icloud-unsupported".into()),
    };
    let locator = match provider {
        CloudProvider::Onedrive => onedrive_path_locator(local_root, destination)?,
        CloudProvider::GoogleDrive => ProviderRemoteLocator::GoogleDriveFileId(object_id.clone()),
        CloudProvider::Icloud => unreachable!(),
    };
    if let Err(error) = control.check() {
        let cleanup = delete_uploaded_object(provider, &object_id, bearer_token);
        return Err(match cleanup { Ok(()) => error, Err(cleanup_error) => format!("{error},provider-api-upload-cleanup-failed:{cleanup_error}") });
    }
    Ok(ProviderApiUploadResult {
        provider,
        object_id,
        locator,
    })
}

pub fn delete_uploaded_object(
    provider: CloudProvider,
    object_id: &str,
    bearer_token: &str,
) -> Result<(), String> {
    validate_bearer_token(bearer_token)?;
    if object_id.trim().is_empty() || object_id.bytes().any(|byte| byte.is_ascii_control()) {
        return Err("provider-api-object-id-invalid".into());
    }
    let encoded = percent_encode_segment(object_id);
    let url = match provider {
        CloudProvider::Onedrive => format!("{ONEDRIVE_GRAPH_ITEMS}/{encoded}"),
        CloudProvider::GoogleDrive => format!("{GOOGLE_FILES}/{encoded}"),
        CloudProvider::Icloud => return Err("provider-api-icloud-unsupported".into()),
    };
    let mut response = agent()
        .delete(&url)
        .header("Authorization", format!("Bearer {bearer_token}"))
        .call()
        .map_err(safe_transport_error)?;
    read_response_body(&mut response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Seek, SeekFrom, Write};
    use std::sync::atomic::AtomicBool;
    use std::time::Duration;

    #[test]
    fn provider_upload_rejects_pre_start_cancellation() {
        let cancel = AtomicBool::new(true);
        let control = ProviderUploadControl::new(&cancel, Instant::now() + Duration::from_secs(1));
        assert_eq!(control.check().unwrap_err(), "cloud-copy-cancelled");
    }

    #[test]
    fn provider_upload_observes_cancellation_between_chunks() {
        let cancel = AtomicBool::new(false);
        let control = ProviderUploadControl::new(&cancel, Instant::now() + Duration::from_secs(1));
        control.check().unwrap();
        cancel.store(true, Ordering::SeqCst);
        assert_eq!(control.check().unwrap_err(), "cloud-copy-cancelled");
    }

    #[test]
    fn provider_upload_observes_post_success_cancellation_before_receipt() {
        let cancel = AtomicBool::new(false);
        let control = ProviderUploadControl::new(&cancel, Instant::now() + Duration::from_secs(1));
        cancel.store(true, Ordering::SeqCst);
        assert_eq!(control.check().unwrap_err(), "cloud-copy-cancelled");
    }

    #[test]
    fn provider_upload_enforces_one_overall_deadline() {
        let cancel = AtomicBool::new(false);
        let now = Instant::now();
        let control = ProviderUploadControl::new(&cancel, now);
        assert_eq!(control.check_at(now).unwrap_err(), "cloud-copy-deadline-exceeded");
    }

    #[test]
    fn upload_session_urls_encode_each_path_segment() {
        assert_eq!(
            onedrive_session_url("DiskSage Archive/a b.txt"),
            "https://graph.microsoft.com/v1.0/me/drive/root:/DiskSage%20Archive/a%20b.txt:/createUploadSession"
        );
    }

    #[test]
    fn google_query_escapes_drive_expression_literals() {
        assert_eq!(google_query_escape(r"a\\b'c"), r"a\\\\b\'c");
    }

    #[test]
    fn chunk_reader_rewinds_to_server_acknowledged_offset() {
        let mut source = tempfile::tempfile().unwrap();
        source.write_all(b"0123456789").unwrap();
        source.seek(SeekFrom::Start(8)).unwrap();
        let mut buffer = [0_u8; 4];

        read_source_chunk_at(&mut source, 2, &mut buffer).unwrap();

        assert_eq!(&buffer, b"2345");
    }

    #[test]
    fn resumable_upload_statuses_distinguish_progress_from_completion() {
        assert_eq!(
            classify_upload_response_status(200).unwrap(),
            UploadResponseKind::Complete
        );
        assert_eq!(
            classify_upload_response_status(201).unwrap(),
            UploadResponseKind::Complete
        );
        assert_eq!(
            classify_upload_response_status(202).unwrap(),
            UploadResponseKind::Progress
        );
        assert_eq!(
            classify_upload_response_status(308).unwrap(),
            UploadResponseKind::Progress
        );
        assert_eq!(
            classify_upload_response_status(307).unwrap_err(),
            "provider-api-http-status:307"
        );
    }

    #[test]
    fn google_308_without_range_retries_from_zero_instead_of_skipping_bytes() {
        assert_eq!(google_next_upload_offset(None, 1023).unwrap(), 0);
        assert_eq!(
            google_next_upload_offset(Some("bytes=0-42"), 1023).unwrap(),
            43
        );
        assert_eq!(
            google_next_upload_offset(Some("bytes=0-2048"), 1023).unwrap_err(),
            "provider-api-upload-range-invalid"
        );
        assert_eq!(
            google_next_upload_offset(Some("nonsense-42"), 1023).unwrap_err(),
            "provider-api-upload-range-invalid"
        );
    }

    #[test]
    fn resumable_upload_progress_is_bounded_when_server_never_advances() {
        let mut highest_offset = 0;
        let mut no_progress_responses = 0;

        assert_eq!(
            guard_upload_progress(0, &mut highest_offset, &mut no_progress_responses).unwrap(),
            0
        );
        assert_eq!(
            guard_upload_progress(0, &mut highest_offset, &mut no_progress_responses).unwrap(),
            0
        );
        assert_eq!(
            guard_upload_progress(0, &mut highest_offset, &mut no_progress_responses).unwrap_err(),
            "provider-api-upload-no-progress"
        );
    }

    #[test]
    fn resumable_upload_progress_resets_only_after_a_new_high_watermark() {
        let mut highest_offset = 0;
        let mut no_progress_responses = 0;

        assert_eq!(
            guard_upload_progress(512, &mut highest_offset, &mut no_progress_responses).unwrap(),
            512
        );
        assert_eq!(
            guard_upload_progress(256, &mut highest_offset, &mut no_progress_responses).unwrap(),
            256
        );
        assert_eq!(no_progress_responses, 1);
        assert_eq!(
            guard_upload_progress(768, &mut highest_offset, &mut no_progress_responses).unwrap(),
            768
        );
        assert_eq!(no_progress_responses, 0);
    }

    #[test]
    fn onedrive_202_uses_server_next_expected_ranges_instead_of_assuming_full_chunk() {
        assert_eq!(
            parse_onedrive_next_expected_offset(
                r#"{"nextExpectedRanges":["512-"]}"#,
                1023,
            )
            .unwrap(),
            512
        );
        assert_eq!(
            parse_onedrive_next_expected_offset(
                r#"{"nextExpectedRanges":["900-1000","768-"]}"#,
                1023,
            )
            .unwrap(),
            768
        );
    }

    #[test]
    fn onedrive_202_rejects_missing_malformed_or_forward_skipping_ranges() {
        assert_eq!(
            parse_onedrive_next_expected_offset(r#"{"nextExpectedRanges":[]}"#, 1023)
                .unwrap_err(),
            "provider-api-upload-next-range-required"
        );
        assert_eq!(
            parse_onedrive_next_expected_offset(
                r#"{"nextExpectedRanges":["not-a-range"]}"#,
                1023,
            )
            .unwrap_err(),
            "provider-api-upload-next-range-invalid"
        );
        assert_eq!(
            parse_onedrive_next_expected_offset(
                r#"{"nextExpectedRanges":["2048-"]}"#,
                1023,
            )
            .unwrap_err(),
            "provider-api-upload-next-range-invalid"
        );
        assert_eq!(
            parse_onedrive_next_expected_offset("{}", 1023).unwrap_err(),
            "provider-api-upload-progress-invalid"
        );
    }
}
