//! Read-only cloud-provider capacity evidence and conservative copy assessment.
//!
//! OneDrive and Google Drive expose account-level quota through their authenticated metadata APIs.
//! iCloud's File Provider surface does not expose the user's account quota, so it is represented as
//! explicitly unavailable rather than inferred from local APFS free space.

use crate::cloud::CloudProvider;

pub const CAPACITY_SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_CAPACITY_RESERVE_BYTES: u64 = 1024 * 1024 * 1024;

#[cfg(not(coverage))]
const MAX_CAPACITY_RESPONSE_BYTES: u64 = 128 * 1024;
#[cfg(not(coverage))]
const MAX_BEARER_TOKEN_BYTES: usize = 64 * 1024;

const ONEDRIVE_CAPACITY_URL: &str =
    "https://graph.microsoft.com/v1.0/me/drive?%24select=id%2CdriveType%2Cquota";
const GOOGLE_DRIVE_CAPACITY_URL: &str = "https://www.googleapis.com/drive/v3/about?fields=user%28permissionId%29%2CstorageQuota%28limit%2Cusage%2CusageInDrive%2CusageInDriveTrash%29%2CmaxUploadSize";

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CapacityEvidenceKind {
    ProviderApi,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CloudCapacityState {
    Normal,
    Nearing,
    Critical,
    Exceeded,
    Unlimited,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CloudCapacitySnapshot {
    pub schema_version: u32,
    pub provider: CloudProvider,
    pub evidence_kind: CapacityEvidenceKind,
    pub observed_at_ms: u64,
    pub total_bytes: Option<u64>,
    pub used_bytes: Option<u64>,
    pub remaining_bytes: Option<u64>,
    pub trashed_bytes: Option<u64>,
    pub max_upload_size_bytes: Option<u64>,
    pub state: CloudCapacityState,
    pub evidence_fingerprint: Option<String>,
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CloudCapacityAssessment {
    pub snapshot: CloudCapacitySnapshot,
    pub requested_bytes: u64,
    pub largest_candidate_bytes: u64,
    pub reserve_bytes: u64,
    pub required_bytes: Option<u64>,
    pub can_fit: Option<bool>,
    pub blockers: Vec<String>,
    pub notices: Vec<String>,
}

pub fn provider_capacity_url(provider: CloudProvider) -> Result<&'static str, String> {
    match provider {
        CloudProvider::Onedrive => Ok(ONEDRIVE_CAPACITY_URL),
        CloudProvider::GoogleDrive => Ok(GOOGLE_DRIVE_CAPACITY_URL),
        CloudProvider::Icloud => Err("icloud-quota-api-unavailable".into()),
    }
}

fn update_optional_u64(hasher: &mut blake3::Hasher, value: Option<u64>) {
    hasher.update(&[value.is_some() as u8]);
    hasher.update(&value.unwrap_or_default().to_le_bytes());
}

fn evidence_fingerprint(snapshot: &CloudCapacitySnapshot, provider_binding: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-cloud-capacity-v1\0");
    hasher.update(snapshot.provider.as_str().as_bytes());
    hasher.update(&[0]);
    hasher.update(provider_binding.as_bytes());
    hasher.update(&[0]);
    hasher.update(&snapshot.observed_at_ms.to_le_bytes());
    for value in [
        snapshot.total_bytes,
        snapshot.used_bytes,
        snapshot.remaining_bytes,
        snapshot.trashed_bytes,
        snapshot.max_upload_size_bytes,
    ] {
        update_optional_u64(&mut hasher, value);
    }
    hasher.update(&[match snapshot.state {
        CloudCapacityState::Normal => 1,
        CloudCapacityState::Nearing => 2,
        CloudCapacityState::Critical => 3,
        CloudCapacityState::Exceeded => 4,
        CloudCapacityState::Unlimited => 5,
        CloudCapacityState::Unavailable => 6,
    }]);
    hasher.finalize().to_hex().to_string()
}

#[derive(serde::Deserialize)]
struct OneDriveQuotaResponse {
    deleted: Option<u64>,
    remaining: Option<u64>,
    state: Option<String>,
    total: Option<u64>,
    used: Option<u64>,
}

#[derive(serde::Deserialize)]
struct OneDriveCapacityResponse {
    id: Option<String>,
    #[serde(rename = "driveType")]
    drive_type: Option<String>,
    quota: Option<OneDriveQuotaResponse>,
}

fn parse_onedrive_state(value: &str) -> Result<CloudCapacityState, String> {
    match value {
        "normal" => Ok(CloudCapacityState::Normal),
        "nearing" => Ok(CloudCapacityState::Nearing),
        "critical" => Ok(CloudCapacityState::Critical),
        "exceeded" => Ok(CloudCapacityState::Exceeded),
        _ => Err("onedrive-quota-state-invalid".into()),
    }
}

pub fn parse_onedrive_capacity(
    json: &str,
    observed_at_ms: u64,
) -> Result<CloudCapacitySnapshot, String> {
    let response: OneDriveCapacityResponse =
        serde_json::from_str(json).map_err(|_| "onedrive-capacity-response-invalid".to_string())?;
    let drive_id = response
        .id
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 1_024
                && !value.bytes().any(|byte| byte.is_ascii_control())
        })
        .ok_or_else(|| "onedrive-capacity-drive-id-invalid".to_string())?;
    let drive_type = response
        .drive_type
        .filter(|value| matches!(value.as_str(), "personal" | "business" | "documentLibrary"))
        .ok_or_else(|| "onedrive-capacity-drive-type-invalid".to_string())?;
    let quota = response
        .quota
        .ok_or_else(|| "onedrive-quota-missing".to_string())?;
    let total = quota
        .total
        .ok_or_else(|| "onedrive-quota-total-missing".to_string())?;
    let used = quota
        .used
        .ok_or_else(|| "onedrive-quota-used-missing".to_string())?;
    let remaining = quota
        .remaining
        .ok_or_else(|| "onedrive-quota-remaining-missing".to_string())?;
    if remaining > total {
        return Err("onedrive-quota-inconsistent".into());
    }
    let state = parse_onedrive_state(
        quota
            .state
            .as_deref()
            .ok_or_else(|| "onedrive-quota-state-missing".to_string())?,
    )?;
    let mut snapshot = CloudCapacitySnapshot {
        schema_version: CAPACITY_SCHEMA_VERSION,
        provider: CloudProvider::Onedrive,
        evidence_kind: CapacityEvidenceKind::ProviderApi,
        observed_at_ms,
        total_bytes: Some(total),
        used_bytes: Some(used),
        remaining_bytes: Some(remaining),
        trashed_bytes: quota.deleted,
        max_upload_size_bytes: None,
        state,
        evidence_fingerprint: None,
        unavailable_reason: None,
    };
    snapshot.evidence_fingerprint = Some(evidence_fingerprint(
        &snapshot,
        &format!("{drive_type}\0{drive_id}"),
    ));
    Ok(snapshot)
}

#[derive(serde::Deserialize)]
struct GoogleStorageQuotaResponse {
    limit: Option<String>,
    usage: Option<String>,
    #[serde(rename = "usageInDrive")]
    usage_in_drive: Option<String>,
    #[serde(rename = "usageInDriveTrash")]
    usage_in_drive_trash: Option<String>,
}

#[derive(serde::Deserialize)]
struct GoogleCapacityResponse {
    user: Option<GoogleUserResponse>,
    #[serde(rename = "storageQuota")]
    storage_quota: Option<GoogleStorageQuotaResponse>,
    #[serde(rename = "maxUploadSize")]
    max_upload_size: Option<String>,
}

#[derive(serde::Deserialize)]
struct GoogleUserResponse {
    #[serde(rename = "permissionId")]
    permission_id: Option<String>,
}

fn parse_google_u64(value: Option<&str>, missing: &str, invalid: &str) -> Result<u64, String> {
    let value = value.ok_or_else(|| missing.to_string())?;
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(invalid.into());
    }
    value.parse::<u64>().map_err(|_| invalid.to_string())
}

fn state_from_limit(limit: u64, usage: u64) -> CloudCapacityState {
    if usage >= limit {
        return CloudCapacityState::Exceeded;
    }
    let remaining = limit - usage;
    if u128::from(remaining) * 100 < u128::from(limit) {
        CloudCapacityState::Critical
    } else if u128::from(remaining) * 10 < u128::from(limit) {
        CloudCapacityState::Nearing
    } else {
        CloudCapacityState::Normal
    }
}

pub fn parse_google_drive_capacity(
    json: &str,
    observed_at_ms: u64,
) -> Result<CloudCapacitySnapshot, String> {
    let response: GoogleCapacityResponse = serde_json::from_str(json)
        .map_err(|_| "google-drive-capacity-response-invalid".to_string())?;
    let permission_id = response
        .user
        .and_then(|user| user.permission_id)
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 1_024
                && !value.bytes().any(|byte| byte.is_ascii_control())
        })
        .ok_or_else(|| "google-drive-capacity-user-binding-invalid".to_string())?;
    let quota = response
        .storage_quota
        .ok_or_else(|| "google-drive-storage-quota-missing".to_string())?;
    let usage = parse_google_u64(
        quota.usage.as_deref(),
        "google-drive-quota-usage-missing",
        "google-drive-quota-usage-invalid",
    )?;
    let usage_in_drive = parse_google_u64(
        quota.usage_in_drive.as_deref(),
        "google-drive-quota-drive-usage-missing",
        "google-drive-quota-drive-usage-invalid",
    )?;
    if usage_in_drive > usage {
        return Err("google-drive-quota-inconsistent".into());
    }
    let trashed = parse_google_u64(
        quota.usage_in_drive_trash.as_deref(),
        "google-drive-quota-trash-usage-missing",
        "google-drive-quota-trash-usage-invalid",
    )?;
    let max_upload_size = parse_google_u64(
        response.max_upload_size.as_deref(),
        "google-drive-max-upload-size-missing",
        "google-drive-max-upload-size-invalid",
    )?;
    let (total, remaining, state) = match quota.limit.as_deref() {
        Some(limit) => {
            let limit = parse_google_u64(
                Some(limit),
                "google-drive-quota-limit-missing",
                "google-drive-quota-limit-invalid",
            )?;
            (
                Some(limit),
                Some(limit.saturating_sub(usage)),
                state_from_limit(limit, usage),
            )
        }
        None => (None, None, CloudCapacityState::Unlimited),
    };
    let mut snapshot = CloudCapacitySnapshot {
        schema_version: CAPACITY_SCHEMA_VERSION,
        provider: CloudProvider::GoogleDrive,
        evidence_kind: CapacityEvidenceKind::ProviderApi,
        observed_at_ms,
        total_bytes: total,
        used_bytes: Some(usage),
        remaining_bytes: remaining,
        trashed_bytes: Some(trashed),
        max_upload_size_bytes: Some(max_upload_size),
        state,
        evidence_fingerprint: None,
        unavailable_reason: None,
    };
    snapshot.evidence_fingerprint = Some(evidence_fingerprint(
        &snapshot,
        &format!("{permission_id}\0usage-in-drive:{usage_in_drive}"),
    ));
    Ok(snapshot)
}

pub fn unavailable_capacity(
    provider: CloudProvider,
    observed_at_ms: u64,
    reason: &str,
) -> CloudCapacitySnapshot {
    CloudCapacitySnapshot {
        schema_version: CAPACITY_SCHEMA_VERSION,
        provider,
        evidence_kind: CapacityEvidenceKind::Unavailable,
        observed_at_ms,
        total_bytes: None,
        used_bytes: None,
        remaining_bytes: None,
        trashed_bytes: None,
        max_upload_size_bytes: None,
        state: CloudCapacityState::Unavailable,
        evidence_fingerprint: None,
        unavailable_reason: Some(reason.into()),
    }
}

/// Reduce OAuth and provider failures to stable, non-secret reasons suitable for UI and receipts.
///
/// Transport details and provider response bodies must never cross the command boundary because
/// future clients could accidentally include credentials or customer identifiers in their errors.
pub fn unavailable_capacity_from_error(
    provider: CloudProvider,
    observed_at_ms: u64,
    error: &str,
) -> CloudCapacitySnapshot {
    let reason = if provider == CloudProvider::Icloud {
        "icloud-quota-api-unavailable"
    } else if matches!(
        error,
        "provider-oauth-connection-missing" | "provider-capacity-oauth-connections-required"
    ) {
        "provider-oauth-connection-missing"
    } else if error == "provider-oauth-connection-ambiguous" {
        "provider-oauth-connection-ambiguous"
    } else if error.starts_with("oauth-connection-document-") {
        "provider-oauth-connection-document-invalid"
    } else if matches!(
        error,
        "provider-oauth-keyring-unavailable"
            | "provider-oauth-refresh-token-unavailable"
            | "provider-oauth-refresh-token-invalid"
    ) {
        "provider-oauth-credential-unavailable"
    } else if error.starts_with("oauth-token-")
        || error.starts_with("oauth-access-token-")
        || error == "oauth-required-scope-missing"
    {
        "provider-oauth-refresh-failed"
    } else {
        "cloud-capacity-provider-api-unavailable"
    };
    unavailable_capacity(provider, observed_at_ms, reason)
}

pub fn assess_capacity(
    snapshot: CloudCapacitySnapshot,
    requested_bytes: u64,
    largest_candidate_bytes: u64,
    reserve_bytes: u64,
) -> CloudCapacityAssessment {
    let required_bytes = requested_bytes.checked_add(reserve_bytes);
    let mut blockers = Vec::new();
    let mut notices = Vec::new();
    let can_fit = if snapshot.evidence_kind == CapacityEvidenceKind::Unavailable {
        blockers.push(
            snapshot
                .unavailable_reason
                .clone()
                .unwrap_or_else(|| "cloud-capacity-unavailable".into()),
        );
        None
    } else {
        if required_bytes.is_none() {
            blockers.push("cloud-capacity-required-bytes-overflow".into());
        }
        if snapshot.state == CloudCapacityState::Exceeded {
            blockers.push("cloud-capacity-provider-state-exceeded".into());
        } else if snapshot.state == CloudCapacityState::Critical {
            notices.push("cloud-capacity-provider-state-critical".into());
        } else if snapshot.state == CloudCapacityState::Nearing {
            notices.push("cloud-capacity-provider-state-nearing".into());
        } else if snapshot.state == CloudCapacityState::Unlimited {
            notices.push("cloud-capacity-provider-reports-unlimited".into());
        }
        if snapshot.provider == CloudProvider::GoogleDrive {
            notices.push("google-capacity-may-reflect-pooled-organization-storage".into());
        }
        if let Some(max_upload) = snapshot.max_upload_size_bytes {
            if largest_candidate_bytes > max_upload {
                blockers.push("cloud-max-upload-size-exceeded".into());
            }
        }
        match (required_bytes, snapshot.remaining_bytes, snapshot.state) {
            (Some(required), Some(remaining), _) if required > remaining => {
                blockers.push("cloud-capacity-insufficient-with-reserve".into())
            }
            (Some(_), Some(_), _) | (Some(_), None, CloudCapacityState::Unlimited) => {}
            _ => blockers.push("cloud-capacity-remaining-unverified".into()),
        }
        Some(blockers.is_empty())
    };
    blockers.sort();
    blockers.dedup();
    notices.sort();
    notices.dedup();
    CloudCapacityAssessment {
        snapshot,
        requested_bytes,
        largest_candidate_bytes,
        reserve_bytes,
        required_bytes,
        can_fit,
        blockers,
        notices,
    }
}

#[cfg(not(coverage))]
pub trait ProviderCapacityTransport {
    fn fetch_json(&self, provider: CloudProvider, bearer_token: &str) -> Result<String, String>;
}

#[cfg(not(coverage))]
pub struct FixedHostProviderCapacityClient {
    agent: ureq::Agent,
}

#[cfg(not(coverage))]
impl Default for FixedHostProviderCapacityClient {
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
        ureq::Error::StatusCode(code) => format!("provider-capacity-http-status:{code}"),
        ureq::Error::Timeout(_) => "provider-capacity-timeout".into(),
        ureq::Error::HostNotFound => "provider-capacity-host-not-found".into(),
        ureq::Error::BodyExceedsLimit(_) => "provider-capacity-response-too-large".into(),
        _ => "provider-capacity-request-failed".into(),
    }
}

#[cfg(not(coverage))]
impl ProviderCapacityTransport for FixedHostProviderCapacityClient {
    fn fetch_json(&self, provider: CloudProvider, bearer_token: &str) -> Result<String, String> {
        if bearer_token.is_empty()
            || bearer_token.len() > MAX_BEARER_TOKEN_BYTES
            || bearer_token.bytes().any(|byte| byte.is_ascii_control())
        {
            return Err("provider-capacity-bearer-token-invalid".into());
        }
        let authorization = format!("Bearer {bearer_token}");
        let mut response = self
            .agent
            .get(provider_capacity_url(provider)?)
            .header("Authorization", &authorization)
            .header("Accept", "application/json")
            .call()
            .map_err(safe_transport_error)?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(format!("provider-capacity-http-status:{status}"));
        }
        response
            .body_mut()
            .with_config()
            .limit(MAX_CAPACITY_RESPONSE_BYTES)
            .read_to_string()
            .map_err(safe_transport_error)
    }
}

#[cfg(not(coverage))]
pub fn collect_authenticated_capacity(
    provider: CloudProvider,
    bearer_token: &str,
    observed_at_ms: u64,
    transport: &dyn ProviderCapacityTransport,
) -> Result<CloudCapacitySnapshot, String> {
    let json = transport.fetch_json(provider, bearer_token)?;
    match provider {
        CloudProvider::Onedrive => parse_onedrive_capacity(&json, observed_at_ms),
        CloudProvider::GoogleDrive => parse_google_drive_capacity(&json, observed_at_ms),
        CloudProvider::Icloud => Err("icloud-quota-api-unavailable".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capacity_urls_are_fixed_and_icloud_is_explicitly_unavailable() {
        assert_eq!(
            provider_capacity_url(CloudProvider::Onedrive).unwrap(),
            ONEDRIVE_CAPACITY_URL
        );
        assert_eq!(
            provider_capacity_url(CloudProvider::GoogleDrive).unwrap(),
            GOOGLE_DRIVE_CAPACITY_URL
        );
        assert_eq!(
            provider_capacity_url(CloudProvider::Icloud).unwrap_err(),
            "icloud-quota-api-unavailable"
        );
    }

    #[test]
    fn parses_onedrive_quota_and_assesses_reserved_capacity() {
        let snapshot = parse_onedrive_capacity(
            r#"{"id":"drive-id","driveType":"personal","quota":{"deleted":5,"remaining":4000,"state":"normal","total":10000,"used":6000}}"#,
            30,
        )
        .unwrap();
        assert_eq!(snapshot.remaining_bytes, Some(4_000));
        assert_eq!(snapshot.state, CloudCapacityState::Normal);
        assert_eq!(snapshot.evidence_fingerprint.as_ref().unwrap().len(), 64);

        let fits = assess_capacity(snapshot.clone(), 2_000, 2_000, 1_000);
        assert_eq!(fits.required_bytes, Some(3_000));
        assert_eq!(fits.can_fit, Some(true));
        assert!(fits.blockers.is_empty());

        let full = assess_capacity(snapshot, 3_500, 3_500, 1_000);
        assert_eq!(full.can_fit, Some(false));
        assert!(full
            .blockers
            .contains(&"cloud-capacity-insufficient-with-reserve".to_string()));
    }

    #[test]
    fn onedrive_quota_rejects_missing_or_inconsistent_authority_fields() {
        assert!(parse_onedrive_capacity(
            r#"{"id":"drive-id","driveType":"personal","quota":{"remaining":1,"state":"normal","total":1}}"#,
            1,
        )
        .is_err());
        assert!(parse_onedrive_capacity(
            r#"{"id":"drive-id","driveType":"personal","quota":{"remaining":2,"state":"normal","total":1,"used":0}}"#,
            1,
        )
        .is_err());
        assert!(parse_onedrive_capacity(
            r#"{"id":"drive-id","driveType":"personal","quota":{"remaining":0,"state":"future-state","total":1,"used":1}}"#,
            1,
        )
        .is_err());
    }

    #[test]
    fn parses_google_limited_and_unlimited_quota_without_conflating_drive_usage() {
        let limited = parse_google_drive_capacity(
            r#"{"user":{"permissionId":"google-user-id"},"storageQuota":{"limit":"10000","usage":"9951","usageInDrive":"8000","usageInDriveTrash":"300"},"maxUploadSize":"5000"}"#,
            40,
        )
        .unwrap();
        assert_eq!(limited.used_bytes, Some(9_951));
        assert_eq!(limited.remaining_bytes, Some(49));
        assert_eq!(limited.state, CloudCapacityState::Critical);
        let assessment = assess_capacity(limited, 10, 10, 30);
        assert_eq!(assessment.can_fit, Some(true));
        assert!(assessment
            .notices
            .contains(&"cloud-capacity-provider-state-critical".to_string()));
        assert!(assessment
            .notices
            .contains(&"google-capacity-may-reflect-pooled-organization-storage".to_string()));

        let unlimited = parse_google_drive_capacity(
            r#"{"user":{"permissionId":"google-user-id"},"storageQuota":{"usage":"9501","usageInDrive":"8000","usageInDriveTrash":"300"},"maxUploadSize":"5000"}"#,
            41,
        )
        .unwrap();
        assert_eq!(unlimited.total_bytes, None);
        assert_eq!(unlimited.remaining_bytes, None);
        assert_eq!(unlimited.state, CloudCapacityState::Unlimited);
        assert_eq!(
            assess_capacity(unlimited, 4_000, 4_000, 1_000).can_fit,
            Some(true)
        );
    }

    #[test]
    fn google_capacity_rejects_invalid_numeric_and_max_upload_shapes() {
        assert!(parse_google_drive_capacity(
            r#"{"user":{"permissionId":"google-user-id"},"storageQuota":{"limit":"100","usage":"x","usageInDrive":"1","usageInDriveTrash":"0"},"maxUploadSize":"5"}"#,
            1,
        )
        .is_err());
        assert!(parse_google_drive_capacity(
            r#"{"user":{"permissionId":"google-user-id"},"storageQuota":{"limit":"100","usage":"10","usageInDrive":"11","usageInDriveTrash":"0"},"maxUploadSize":"5"}"#,
            1,
        )
        .is_err());
        assert!(parse_google_drive_capacity(
            r#"{"user":{"permissionId":"google-user-id"},"storageQuota":{"limit":"100","usage":"10","usageInDrive":"1","usageInDriveTrash":"0"}}"#,
            1,
        )
        .is_err());
        assert!(parse_google_drive_capacity(
            r#"{"storageQuota":{"limit":"100","usage":"10","usageInDrive":"1","usageInDriveTrash":"0"},"maxUploadSize":"5"}"#,
            1,
        )
        .is_err());
    }

    #[test]
    fn assessment_blocks_exceeded_upload_limit_overflow_and_unavailable_capacity() {
        let mut google = parse_google_drive_capacity(
            r#"{"user":{"permissionId":"google-user-id"},"storageQuota":{"limit":"100","usage":"100","usageInDrive":"90","usageInDriveTrash":"5"},"maxUploadSize":"10"}"#,
            1,
        )
        .unwrap();
        google.state = CloudCapacityState::Exceeded;
        let blocked = assess_capacity(google, u64::MAX, 11, 1);
        assert_eq!(blocked.can_fit, Some(false));
        for expected in [
            "cloud-capacity-provider-state-exceeded",
            "cloud-capacity-required-bytes-overflow",
            "cloud-max-upload-size-exceeded",
        ] {
            assert!(
                blocked.blockers.contains(&expected.to_string()),
                "{expected}"
            );
        }

        let unavailable = assess_capacity(
            unavailable_capacity(CloudProvider::Icloud, 1, "icloud-quota-api-unavailable"),
            1,
            1,
            1,
        );
        assert_eq!(unavailable.can_fit, None);
        assert_eq!(
            unavailable.blockers,
            ["icloud-quota-api-unavailable".to_string()]
        );
    }

    #[test]
    fn connection_failures_are_redacted_into_actionable_capacity_reasons() {
        let cases = [
            (
                "provider-oauth-connection-missing",
                "provider-oauth-connection-missing",
            ),
            (
                "provider-capacity-oauth-connections-required",
                "provider-oauth-connection-missing",
            ),
            (
                "oauth-connection-document-invalid",
                "provider-oauth-connection-document-invalid",
            ),
            (
                "provider-oauth-refresh-token-unavailable",
                "provider-oauth-credential-unavailable",
            ),
            (
                "oauth-token-http-status:401",
                "provider-oauth-refresh-failed",
            ),
            (
                "provider-capacity-http-status:503",
                "cloud-capacity-provider-api-unavailable",
            ),
            (
                "secret-bearing unexpected transport detail",
                "cloud-capacity-provider-api-unavailable",
            ),
        ];
        for (error, expected) in cases {
            let snapshot = unavailable_capacity_from_error(CloudProvider::Onedrive, 42, error);
            assert_eq!(snapshot.unavailable_reason.as_deref(), Some(expected));
            if error.contains("secret") {
                assert!(!serde_json::to_string(&snapshot).unwrap().contains(error));
            }
        }
    }

    #[test]
    fn fixed_host_client_rejects_invalid_tokens_and_icloud_without_network() {
        let client = FixedHostProviderCapacityClient::default();
        assert_eq!(
            client.fetch_json(CloudProvider::Onedrive, "").unwrap_err(),
            "provider-capacity-bearer-token-invalid"
        );
        assert_eq!(
            client
                .fetch_json(CloudProvider::Icloud, "not-a-secret-test-token")
                .unwrap_err(),
            "icloud-quota-api-unavailable"
        );
    }
}
