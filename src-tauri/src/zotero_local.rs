//! Bounded, localhost-only Zotero metadata writes.
//!
//! The local API is deliberately separate from cloud OAuth.  DiskSage sends only the
//! bibliographic fields and the original source URL supplied in the manifest. Optional full-text
//! uploads are explicit, bounded, and use Zotero's local three-phase file flow.

use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};

pub const DEFAULT_LOCAL_API_BASE: &str = "http://127.0.0.1:23119/api/users/0";
const LOCAL_API_ROOT: &str = "http://127.0.0.1:23119/api/";
pub const MAX_REFERENCE_COUNT: usize = 100;
pub const MAX_REFERENCE_MANIFEST_BYTES: usize = 256 * 1024;
pub const MAX_FULL_TEXT_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const MAX_TITLE_CHARS: usize = 512;
const MAX_TEXT_CHARS: usize = 32 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ZoteroCreator {
    pub creator_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ZoteroReference {
    pub item_type: String,
    pub title: String,
    #[serde(default)]
    pub creators: Vec<ZoteroCreator>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    #[serde(rename = "DOI", skip_serializing_if = "Option::is_none")]
    pub doi: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub abstract_note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publication_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issue: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pages: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra: Option<String>,
    /// Optional local original document to import as a stored Zotero attachment.
    #[serde(default, rename = "fullTextPath", skip_serializing)]
    pub full_text_path: Option<PathBuf>,
}

pub fn parse_manifest(bytes: &[u8]) -> Result<Vec<ZoteroReference>, String> {
    if bytes.len() > MAX_REFERENCE_MANIFEST_BYTES {
        return Err("zotero-reference-manifest-too-large".into());
    }
    let references: Vec<ZoteroReference> = serde_json::from_slice(bytes)
        .map_err(|_| "zotero-reference-manifest-invalid".to_string())?;
    validate_references(&references)?;
    Ok(references)
}

pub fn validate_references(references: &[ZoteroReference]) -> Result<(), String> {
    if references.is_empty() {
        return Err("zotero-reference-manifest-empty".into());
    }
    if references.len() > MAX_REFERENCE_COUNT {
        return Err("zotero-reference-count-exceeded".into());
    }
    for reference in references {
        if reference.item_type.is_empty() || reference.item_type.len() > 64 {
            return Err("zotero-item-type-invalid".into());
        }
        if reference.title.trim().is_empty() || reference.title.chars().count() > MAX_TITLE_CHARS {
            return Err("zotero-title-invalid".into());
        }
        if reference.creators.len() > 50 {
            return Err("zotero-creator-count-exceeded".into());
        }
        for creator in &reference.creators {
            if creator.creator_type.trim().is_empty()
                || creator.creator_type.len() > 64
                || (creator.name.as_deref().unwrap_or("").trim().is_empty()
                    && creator
                        .first_name
                        .as_deref()
                        .unwrap_or("")
                        .trim()
                        .is_empty()
                    && creator.last_name.as_deref().unwrap_or("").trim().is_empty())
            {
                return Err("zotero-creator-invalid".into());
            }
        }
        if let Some(url) = &reference.url {
            if !(url.starts_with("https://") || url.starts_with("http://"))
                || url.bytes().any(|byte| byte.is_ascii_control())
                || url.len() > 4 * 1024
            {
                return Err("zotero-source-url-invalid".into());
            }
        }
        if let Some(path) = &reference.full_text_path {
            let metadata = std::fs::symlink_metadata(path)
                .map_err(|_| "zotero-full-text-unavailable".to_string())?;
            if !path.is_absolute() || metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err("zotero-full-text-must-be-regular-file".into());
            }
            if metadata.len() > MAX_FULL_TEXT_BYTES {
                return Err("zotero-full-text-too-large".into());
            }
        }
        for text in [
            reference.date.as_deref(),
            reference.doi.as_deref(),
            reference.abstract_note.as_deref(),
            reference.publication_title.as_deref(),
            reference.publisher.as_deref(),
            reference.volume.as_deref(),
            reference.issue.as_deref(),
            reference.pages.as_deref(),
            reference.extra.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            if text.chars().count() > MAX_TEXT_CHARS
                || text
                    .bytes()
                    .any(|byte| byte.is_ascii_control() && byte != b'\n' && byte != b'\t')
            {
                return Err("zotero-field-invalid".into());
            }
        }
    }
    Ok(())
}

pub fn dry_run_summary(references: &[ZoteroReference]) -> serde_json::Value {
    serde_json::json!({
        "executed": false,
        "local_api": DEFAULT_LOCAL_API_BASE,
        "item_count": references.len(),
        "titles": references.iter().map(|reference| reference.title.clone()).collect::<Vec<_>>(),
        "original_urls": references.iter().filter_map(|reference| reference.url.clone()).collect::<Vec<_>>(),
        "notice": "metadata and original source URLs only; pass --execute with ZOTERO_LOCAL_API_KEY to write"
    })
}

fn classify_write_response(status: u16, body: &str) -> Result<(), String> {
    if (200..300).contains(&status) {
        return Ok(());
    }
    if status == 400 && body.contains("Endpoint does not support method") {
        return Err("zotero-local-api-write-unsupported".into());
    }
    if status == 401 || status == 403 {
        return Err("zotero-local-api-write-unauthorized".into());
    }
    Err(format!("zotero-http-status:{status}"))
}

#[cfg(not(coverage))]
pub fn write_references(
    references: &[ZoteroReference],
    api_key: &str,
) -> Result<serde_json::Value, String> {
    validate_references(references)?;
    if api_key.is_empty()
        || api_key.len() > 256
        || api_key.bytes().any(|byte| byte.is_ascii_control())
    {
        return Err("zotero-local-api-key-invalid".into());
    }
    let body =
        serde_json::to_vec(references).map_err(|_| "zotero-request-encode-failed".to_string())?;
    let config = ureq::Agent::config_builder()
        .max_redirects(0)
        .http_status_as_error(false)
        .timeout_global(Some(std::time::Duration::from_secs(10)))
        .build();
    let agent = ureq::Agent::new_with_config(config);
    let server_id = fetch_server_id(&agent)?;
    let request = add_server_id(
        agent
            .post(format!("{DEFAULT_LOCAL_API_BASE}/items"))
            .header("Zotero-API-Key", api_key)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json"),
        server_id.as_deref(),
    );
    let mut response = request.send(body).map_err(|error| match error {
        ureq::Error::StatusCode(code) => format!("zotero-http-status:{code}"),
        ureq::Error::Timeout(_) => "zotero-local-api-timeout".into(),
        ureq::Error::HostNotFound => "zotero-local-api-unavailable".into(),
        ureq::Error::BodyExceedsLimit(_) => "zotero-response-too-large".into(),
        _ => "zotero-local-api-request-failed".into(),
    })?;
    let response_body = response
        .body_mut()
        .with_config()
        .limit(256 * 1024)
        .read_to_string()
        .map_err(|_| "zotero-response-read-failed".to_string())?;
    classify_write_response(response.status().as_u16(), &response_body)?;
    let response: serde_json::Value =
        serde_json::from_str(&response_body).map_err(|_| "zotero-response-invalid".to_string())?;
    if response
        .get("failed")
        .and_then(serde_json::Value::as_object)
        .is_some_and(|failed| !failed.is_empty())
    {
        return Err("zotero-item-write-failed".into());
    }
    let mut attachment_count = 0usize;
    for (index, reference) in references.iter().enumerate() {
        let Some(path) = reference.full_text_path.as_deref() else {
            continue;
        };
        let parent_key = response
            .get("successful")
            .and_then(|successful| successful.get(index.to_string()))
            .and_then(|item| item.get("key"))
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "zotero-parent-key-missing".to_string())?;
        upload_full_text_attachment(&agent, server_id.as_deref(), api_key, parent_key, path)?;
        attachment_count += 1;
    }
    Ok(serde_json::json!({
        "items": response,
        "full_text_attachments": attachment_count
    }))
}

#[cfg(not(coverage))]
fn fetch_server_id(agent: &ureq::Agent) -> Result<Option<String>, String> {
    let mut response = agent.get(LOCAL_API_ROOT).call().map_err(safe_ureq_error)?;
    if !(200..300).contains(&response.status().as_u16()) {
        return Err(format!("zotero-http-status:{}", response.status().as_u16()));
    }
    response
        .body_mut()
        .with_config()
        .limit(64 * 1024)
        .read_to_vec()
        .map_err(|_| "zotero-response-read-failed".to_string())?;
    Ok(response
        .headers()
        .get("Zotero-Server-ID")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty() && value.len() <= 128)
        .map(str::to_owned))
}

#[cfg(not(coverage))]
fn add_server_id<B>(
    request: ureq::RequestBuilder<B>,
    server_id: Option<&str>,
) -> ureq::RequestBuilder<B> {
    match server_id {
        Some(server_id) => request.header("Zotero-Server-ID", server_id),
        None => request,
    }
}

#[cfg(not(coverage))]
fn safe_ureq_error(error: ureq::Error) -> String {
    match error {
        ureq::Error::StatusCode(code) => format!("zotero-http-status:{code}"),
        ureq::Error::Timeout(_) => "zotero-local-api-timeout".into(),
        ureq::Error::HostNotFound => "zotero-local-api-unavailable".into(),
        ureq::Error::BodyExceedsLimit(_) => "zotero-response-too-large".into(),
        _ => "zotero-local-api-request-failed".into(),
    }
}

#[cfg(not(coverage))]
fn read_json_response(
    response: &mut ureq::http::Response<ureq::Body>,
) -> Result<serde_json::Value, String> {
    if !(200..300).contains(&response.status().as_u16()) {
        return Err(format!("zotero-http-status:{}", response.status().as_u16()));
    }
    let body = response
        .body_mut()
        .with_config()
        .limit(256 * 1024)
        .read_to_string()
        .map_err(|_| "zotero-response-read-failed".to_string())?;
    serde_json::from_str(&body).map_err(|_| "zotero-response-invalid".into())
}

#[cfg(not(coverage))]
struct FullTextObservation {
    path: PathBuf,
    bytes: u64,
    md5_hex: String,
    filename: String,
    mtime_ms: u64,
    content_type: &'static str,
}

#[cfg(not(coverage))]
fn observe_full_text(path: &Path) -> Result<FullTextObservation, String> {
    let metadata =
        std::fs::symlink_metadata(path).map_err(|_| "zotero-full-text-unavailable".to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("zotero-full-text-must-be-regular-file".into());
    }
    if metadata.len() > MAX_FULL_TEXT_BYTES {
        return Err("zotero-full-text-too-large".into());
    }
    let mut file =
        std::fs::File::open(path).map_err(|_| "zotero-full-text-unreadable".to_string())?;
    let mut digest = Md5::new();
    let mut buffer = [0u8; 1024 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| "zotero-full-text-read-failed".to_string())?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    let mtime_ms = metadata_mtime_ms(&metadata)?;
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty() && !name.bytes().any(|byte| byte.is_ascii_control()))
        .ok_or_else(|| "zotero-full-text-filename-invalid".to_string())?
        .to_string();
    let content_type = match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("pdf") => "application/pdf",
        Some("txt") => "text/plain",
        Some("html" | "htm") => "text/html",
        _ => "application/octet-stream",
    };
    Ok(FullTextObservation {
        path: path.to_path_buf(),
        bytes: metadata.len(),
        md5_hex: format!("{:x}", digest.finalize()),
        filename,
        mtime_ms,
        content_type,
    })
}

#[cfg(not(coverage))]
fn metadata_mtime_ms(metadata: &std::fs::Metadata) -> Result<u64, String> {
    metadata
        .modified()
        .map_err(|_| "zotero-full-text-mtime-unavailable".to_string())?
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .map_err(|_| "zotero-full-text-mtime-invalid".to_string())
}

#[cfg(not(coverage))]
fn upload_full_text_attachment(
    agent: &ureq::Agent,
    server_id: Option<&str>,
    api_key: &str,
    parent_key: &str,
    path: &Path,
) -> Result<(), String> {
    let observation = observe_full_text(path)?;
    let attachment = serde_json::json!([{
        "itemType": "attachment",
        "linkMode": "imported_file",
        "parentItem": parent_key,
        "title": observation.filename,
        "filename": observation.filename.clone(),
        "contentType": observation.content_type
    }]);
    let request = add_server_id(
        agent
            .post(format!("{DEFAULT_LOCAL_API_BASE}/items"))
            .header("Zotero-API-Key", api_key)
            .header("Content-Type", "application/json"),
        server_id,
    );
    let mut response = request
        .send(
            serde_json::to_vec(&attachment)
                .map_err(|_| "zotero-attachment-encode-failed".to_string())?,
        )
        .map_err(safe_ureq_error)?;
    let created = read_json_response(&mut response)?;
    let attachment_key = created
        .get("successful")
        .and_then(|successful| successful.get("0"))
        .and_then(|item| item.get("key"))
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "zotero-attachment-key-missing".to_string())?;
    let filesize = observation.bytes.to_string();
    let mtime = observation.mtime_ms.to_string();
    let authorize_request = add_server_id(
        agent
            .post(format!(
                "{DEFAULT_LOCAL_API_BASE}/items/{attachment_key}/file"
            ))
            .header("Zotero-API-Key", api_key)
            .header("If-None-Match", "*"),
        server_id,
    );
    let mut authorize = authorize_request
        .send_form([
            ("md5", observation.md5_hex.as_str()),
            ("filename", observation.filename.as_str()),
            ("filesize", filesize.as_str()),
            ("mtime", mtime.as_str()),
        ])
        .map_err(safe_ureq_error)?;
    let authorization = read_json_response(&mut authorize)?;
    if authorization
        .get("exists")
        .and_then(serde_json::Value::as_i64)
        == Some(1)
    {
        return Ok(());
    }
    let upload_url = authorization
        .get("url")
        .and_then(serde_json::Value::as_str)
        .filter(|url| url.starts_with("http://127.0.0.1:23119/"))
        .ok_or_else(|| "zotero-upload-url-not-local".to_string())?;
    let upload_key = authorization
        .get("uploadKey")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "zotero-upload-key-missing".to_string())?;
    let file = std::fs::File::open(&observation.path)
        .map_err(|_| "zotero-full-text-unreadable".to_string())?;
    let current = std::fs::symlink_metadata(&observation.path)
        .map_err(|_| "zotero-full-text-unavailable".to_string())?;
    if current.file_type().is_symlink()
        || !current.is_file()
        || current.len() != observation.bytes
        || metadata_mtime_ms(&current)? != observation.mtime_ms
    {
        return Err("zotero-full-text-changed-before-upload".into());
    }
    let mut uploaded = agent
        .post(upload_url)
        .header(
            "Content-Type",
            authorization
                .get("contentType")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(observation.content_type),
        )
        .send(&file)
        .map_err(safe_ureq_error)?;
    if uploaded.status().as_u16() != 201 {
        return Err(format!(
            "zotero-upload-http-status:{}",
            uploaded.status().as_u16()
        ));
    }
    uploaded
        .body_mut()
        .read_to_vec()
        .map_err(|_| "zotero-upload-response-read-failed".to_string())?;
    let register_request = add_server_id(
        agent
            .post(format!(
                "{DEFAULT_LOCAL_API_BASE}/items/{attachment_key}/file"
            ))
            .header("Zotero-API-Key", api_key)
            .header("If-None-Match", "*"),
        server_id,
    );
    let mut registered = register_request
        .send_form([("upload", upload_key)])
        .map_err(safe_ureq_error)?;
    if registered.status().as_u16() != 204 {
        return Err(format!(
            "zotero-registration-http-status:{}",
            registered.status().as_u16()
        ));
    }
    registered
        .body_mut()
        .read_to_vec()
        .map_err(|_| "zotero-registration-response-read-failed".to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference() -> ZoteroReference {
        ZoteroReference {
            item_type: "journalArticle".into(),
            title: "Metadata-first storage planning".into(),
            creators: vec![ZoteroCreator {
                creator_type: "author".into(),
                first_name: Some("Ada".into()),
                last_name: Some("Lovelace".into()),
                name: None,
            }],
            date: Some("2026".into()),
            doi: Some("10.0000/example".into()),
            url: Some("https://example.org/paper".into()),
            abstract_note: Some("Bounded evidence.".into()),
            publication_title: None,
            publisher: None,
            volume: None,
            issue: None,
            pages: None,
            extra: Some("source=DiskSage".into()),
            full_text_path: None,
        }
    }

    #[test]
    fn manifest_round_trip_preserves_doi_and_original_url() {
        let bytes = serde_json::to_vec(&vec![reference()]).unwrap();
        let parsed = parse_manifest(&bytes).unwrap();
        assert_eq!(parsed[0].doi.as_deref(), Some("10.0000/example"));
        assert_eq!(parsed[0].url.as_deref(), Some("https://example.org/paper"));
    }

    #[test]
    fn validation_rejects_non_http_source_urls() {
        let mut item = reference();
        item.url = Some("file:///private/source.pdf".into());
        assert_eq!(
            validate_references(&[item]).unwrap_err(),
            "zotero-source-url-invalid"
        );
    }

    #[test]
    fn unsupported_local_api_is_fail_closed() {
        assert_eq!(
            classify_write_response(400, "Endpoint does not support method").unwrap_err(),
            "zotero-local-api-write-unsupported"
        );
        assert_eq!(
            classify_write_response(401, "unauthorized").unwrap_err(),
            "zotero-local-api-write-unauthorized"
        );
    }

    #[test]
    fn full_text_observation_hashes_bounded_regular_file() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("paper.pdf");
        std::fs::write(&path, b"hello").unwrap();
        let observation = observe_full_text(&path).unwrap();
        assert_eq!(observation.bytes, 5);
        assert_eq!(observation.md5_hex, "5d41402abc4b2a76b9719d911017c592");
        assert_eq!(observation.content_type, "application/pdf");
    }
}
