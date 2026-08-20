//! Bounded, localhost-only Zotero metadata writes.
//!
//! The local API is deliberately separate from cloud OAuth.  DiskSage sends only the
//! bibliographic fields and the original source URL supplied in the manifest; local full-text
//! attachment upload remains a separate, explicit operation.

use serde::{Deserialize, Serialize};

pub const DEFAULT_LOCAL_API_BASE: &str = "http://127.0.0.1:23119/api/users/0";
pub const MAX_REFERENCE_COUNT: usize = 100;
pub const MAX_REFERENCE_MANIFEST_BYTES: usize = 256 * 1024;
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
    let mut response = agent
        .post(format!("{DEFAULT_LOCAL_API_BASE}/items"))
        .header("Zotero-API-Key", api_key)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .send(body)
        .map_err(|error| match error {
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
    serde_json::from_str(&response_body).map_err(|_| "zotero-response-invalid".into())
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
}
