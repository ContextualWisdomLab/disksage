//! Path-free cloud-copy readiness export for Naruon.
//!
//! This contract aggregates candidate metadata policy, planner/review blockers, provider runtime,
//! authoritative capacity evidence, and the optional iCloud local queue gate. It never carries a
//! source path, destination path, filename, title, author, account identifier, or raw metadata
//! value, and it never grants cloud-write or source-eviction authority.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::cloud::{CloudPlanOptions, CloudPlanReport, CloudProvider, PreCopyEvidenceCohort};
use crate::cloud_transfer;
use crate::icloud_sync_health::{
    native_pending_scan, native_sync_down_pending, native_sync_up_pending,
    validate_native_status_evidence,
    validate_file_provider_activity_evidence, IcloudFileProviderActivityEvidence,
    IcloudNativeStatusEvidence, IcloudSyncHealthReport, ICLOUD_SYNC_HEALTH_SCHEMA_VERSION,
    FILE_PROVIDER_DISK_IMPORT_NOTICE,
};
use crate::naruon_capacity;
use crate::provider_capacity::{self, CapacityEvidenceKind, CloudCapacityAssessment};
use crate::provider_client_runtime::{self, ProviderClientRuntimeSnapshot};
use crate::provider_global_sync::{self, ProviderGlobalSyncReport, ProviderGlobalSyncState};

/// Schema 8 adds explicit iCloud File Provider lock/stall blockers while preserving the
/// path-free handoff shape; Naruon accepts it alongside schemas 3–7.
pub const NARUON_CLOUD_COPY_READINESS_SCHEMA_VERSION: u32 = 8;
pub const NARUON_CLOUD_COPY_READINESS_MAX_INPUT_BYTES: u64 = 1024 * 1024;
const NARUON_CLOUD_COPY_READINESS_SCHEMA_KIND: &str = "disksage.naruon.cloud-copy-readiness";
const FINGERPRINT_CANONICALIZATION: &str = "lexicographic-json-object-keys-utf8-no-whitespace";
const RUNTIME_BLOCKERS: [&str; 2] = [
    "provider-client-runtime-not-observed",
    "provider-client-runtime-evidence-unavailable",
];
const ICLOUD_ADMISSION_BLOCKERS: [&str; 25] = [
    "icloud-sync-health-evidence-incomplete",
    "icloud-upload-queue-nonempty",
    "icloud-upload-in-flight",
    "icloud-upload-blocked-on-sync-up",
    "icloud-upload-out-of-quota",
    "icloud-upload-queue-state-unclassified",
    "icloud-local-sync-item-error-present",
    "icloud-native-status-evidence-incomplete",
    "icloud-native-status-command-timeout",
    "icloud-native-sync-up-pending",
    "icloud-native-sync-down-pending",
    "icloud-native-status-pending-scan",
    "icloud-file-provider-no-progress",
    "icloud-file-provider-materialization-failed",
    "icloud-file-provider-item-locked",
    "icloud-file-provider-stalled",
    "icloud-file-provider-filename-excluded",
    "icloud-file-provider-root-excluded",
    "icloud-file-provider-indexing-pending",
    "icloud-file-provider-disk-import-active",
    "icloud-file-provider-transfer-active",
    "icloud-file-provider-dump-timeout",
    "icloud-file-provider-dump-output-truncated",
    "icloud-file-provider-evidence-unavailable",
    "icloud-new-copy-admission-evidence-unavailable",
];
const PRE_COPY_EVIDENCE_BLOCKER: &str = "pre-copy-evidence-cohort-unavailable";

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CloudCopyReadinessState {
    NoCandidates,
    Blocked,
    PartiallyReady,
    ReadyWithoutNewReview,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CountBytes {
    pub count: u64,
    pub bytes: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductionTimeEvidenceSummary {
    pub embedded_metadata: CountBytes,
    pub explicit_filename_date: CountBytes,
    pub filesystem_created: CountBytes,
    pub filesystem_modified: CountBytes,
    pub unclassified: CountBytes,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IcloudNewCopyAdmissionSummary {
    pub observed_at_ms: u64,
    pub state: String,
    pub scheduled_waiting_count: u64,
    pub scheduled_waiting_bytes: u64,
    pub scheduled_active_count: u64,
    pub scheduled_active_bytes: u64,
    pub scheduled_count: u64,
    pub scheduled_bytes: u64,
    pub blocked_on_sync_up_count: u64,
    pub out_of_quota_count: u64,
    pub out_of_quota_bytes: u64,
    pub other_state_count: u64,
    pub item_error_count: u64,
    pub item_error_octagon_not_signed_in_count: u64,
    pub item_error_unclassified_count: u64,
    pub newest_item_error_timestamp_ms: Option<u64>,
    pub newest_item_error_age_ms: Option<u64>,
    pub blockers: Vec<String>,
    pub evidence_complete: bool,
    pub database_snapshot_includes_wal: bool,
    #[serde(default)]
    pub native_status: Option<IcloudNativeStatusEvidence>,
    #[serde(default)]
    pub file_provider_activity: Option<IcloudFileProviderActivityEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NaruonCloudCopyReadinessEnvelope {
    pub schema_kind: String,
    pub schema_version: u32,
    pub generated_at_ms: u64,
    pub decision_batch_fingerprint_version: u32,
    pub decision_batch_fingerprint: String,
    pub provider: CloudProvider,
    pub destination_account_scope: crate::cloud::CloudAccountScope,
    pub source_selection_policy: CloudPlanOptions,
    pub candidate_count: u64,
    pub candidate_bytes: u64,
    pub potentially_reclaimable_bytes: u64,
    pub planner_unblocked: CountBytes,
    pub requires_human_review: CountBytes,
    pub ready_without_new_review: CountBytes,
    pub readiness_state: CloudCopyReadinessState,
    pub production_time_evidence: ProductionTimeEvidenceSummary,
    pub candidate_blocker_counts: BTreeMap<String, CountBytes>,
    pub provider_runtime: ProviderClientRuntimeSnapshot,
    pub capacity: CloudCapacityAssessment,
    pub icloud_new_copy_admission: Option<IcloudNewCopyAdmissionSummary>,
    /// Integrity-checked freshness cohort required before a new native iCloud copy.
    #[serde(default)]
    pub pre_copy_evidence: Option<PreCopyEvidenceCohort>,
    pub provider_global_sync: Option<ProviderGlobalSyncReport>,
    pub provider_runtime_prerequisite_met: bool,
    pub remote_capacity_verified: bool,
    pub icloud_new_copy_admission_met: Option<bool>,
    pub pre_copy_evidence_met: Option<bool>,
    pub human_review_decisions_applied: bool,
    pub metadata_policy: Vec<String>,
    pub filename_dates_are_auxiliary: bool,
    pub readiness_fingerprint_canonicalization: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub readiness_fingerprint_sha256: String,
    pub local_paths_included: bool,
    pub relative_names_included: bool,
    pub raw_metadata_values_included: bool,
    pub account_identifiers_included: bool,
    pub provider_sync_attested: bool,
    pub cloud_write_executed: bool,
    pub source_eviction_authorized: bool,
}

/// Read and validate one path-free readiness envelope without following a symlink.
///
/// The bounded input and stable error codes make this suitable for an offline Naruon handoff.
/// This entrypoint is read-only and grants neither cloud-write nor source-eviction authority.
pub fn read_and_validate_naruon_cloud_copy_readiness(
    path: &Path,
) -> Result<NaruonCloudCopyReadinessEnvelope, String> {
    if !path.is_absolute() {
        return Err("naruon-copy-readiness-input-path-not-absolute".into());
    }
    let before = std::fs::symlink_metadata(path)
        .map_err(|_| "naruon-copy-readiness-input-metadata-unavailable".to_string())?;
    if !before.file_type().is_file() {
        return Err("naruon-copy-readiness-input-not-regular-file".into());
    }
    if before.len() == 0 || before.len() > NARUON_CLOUD_COPY_READINESS_MAX_INPUT_BYTES {
        return Err("naruon-copy-readiness-input-size-invalid".into());
    }

    let file = std::fs::File::open(path)
        .map_err(|_| "naruon-copy-readiness-input-open-failed".to_string())?;
    let opened = file
        .metadata()
        .map_err(|_| "naruon-copy-readiness-input-metadata-unavailable".to_string())?;
    if !opened.is_file() || opened.len() != before.len() {
        return Err("naruon-copy-readiness-input-changed".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if opened.dev() != before.dev() || opened.ino() != before.ino() {
            return Err("naruon-copy-readiness-input-changed".into());
        }
    }

    let mut bytes = Vec::with_capacity(
        usize::try_from(before.len())
            .map_err(|_| "naruon-copy-readiness-input-size-invalid".to_string())?,
    );
    file.take(NARUON_CLOUD_COPY_READINESS_MAX_INPUT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "naruon-copy-readiness-input-read-failed".to_string())?;
    if u64::try_from(bytes.len()).ok() != Some(before.len()) {
        return Err("naruon-copy-readiness-input-changed".into());
    }
    let after = std::fs::symlink_metadata(path)
        .map_err(|_| "naruon-copy-readiness-input-changed".to_string())?;
    if !after.file_type().is_file() || after.len() != before.len() {
        return Err("naruon-copy-readiness-input-changed".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if after.dev() != before.dev() || after.ino() != before.ino() {
            return Err("naruon-copy-readiness-input-changed".into());
        }
    }

    let envelope: NaruonCloudCopyReadinessEnvelope = serde_json::from_slice(&bytes)
        .map_err(|_| "naruon-copy-readiness-json-invalid".to_string())?;
    validate_naruon_cloud_copy_readiness(&envelope)?;
    Ok(envelope)
}

fn is_lower_hex_64(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_reason_code(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || (byte == b'-' && index > 0)
        })
        && !value.ends_with('-')
        && !value.contains("--")
}

fn add_count_bytes(target: &mut CountBytes, bytes: u64) -> Result<(), String> {
    target.count = target
        .count
        .checked_add(1)
        .ok_or_else(|| "naruon-copy-readiness-count-overflow".to_string())?;
    target.bytes = target
        .bytes
        .checked_add(bytes)
        .ok_or_else(|| "naruon-copy-readiness-bytes-overflow".to_string())?;
    Ok(())
}

fn production_summary_target<'a>(
    summary: &'a mut ProductionTimeEvidenceSummary,
    source: &str,
) -> &'a mut CountBytes {
    if source.starts_with("embedded:") {
        &mut summary.embedded_metadata
    } else if source.starts_with("filename:") {
        &mut summary.explicit_filename_date
    } else if source == "filesystem:created" {
        &mut summary.filesystem_created
    } else if matches!(
        source,
        "filesystem:modified" | "filesystem:modified-fallback"
    ) {
        &mut summary.filesystem_modified
    } else {
        &mut summary.unclassified
    }
}

fn expected_icloud_admission_blockers(report: &IcloudSyncHealthReport) -> Vec<String> {
    let mut blockers = Vec::new();
    if !report.evidence_complete {
        blockers.push("icloud-sync-health-evidence-incomplete".into());
    }
    if report.upload_queue.scheduled_waiting_count > 0 {
        blockers.push("icloud-upload-queue-nonempty".into());
    }
    if report.upload_queue.scheduled_active_count > 0 {
        blockers.push("icloud-upload-in-flight".into());
    }
    if report.upload_queue.blocked_on_sync_up_count > 0 {
        blockers.push("icloud-upload-blocked-on-sync-up".into());
    }
    if report.upload_queue.out_of_quota_count > 0 {
        blockers.push("icloud-upload-out-of-quota".into());
    }
    if report.upload_queue.other_state_count > 0 {
        blockers.push("icloud-upload-queue-state-unclassified".into());
    }
    if report.upload_queue.item_error_count > 0 {
        blockers.push("icloud-local-sync-item-error-present".into());
    }
    if report
        .native_status
        .as_ref()
        .is_some_and(|status| !status.evidence_complete)
    {
        blockers.push("icloud-native-status-evidence-incomplete".into());
    }
    if report
        .native_status
        .as_ref()
        .is_some_and(|status| status.timed_out)
    {
        blockers.push("icloud-native-status-command-timeout".into());
    }
    if report.native_status.as_ref().is_some_and(native_sync_up_pending) {
        blockers.push("icloud-native-sync-up-pending".into());
    }
    if report
        .native_status
        .as_ref()
        .is_some_and(native_sync_down_pending)
    {
        blockers.push("icloud-native-sync-down-pending".into());
    }
    if report.native_status.as_ref().is_some_and(native_pending_scan) {
        blockers.push("icloud-native-status-pending-scan".into());
    }
    if let Some(activity) = report.file_provider_activity.as_ref() {
        let no_progress = activity.no_progress_fetch_count > 0
            || activity.no_progress_create_count > 0;
        let materialization_failed = activity.materialization_failure_count > 0
            || activity.staged_item_missing_count > 0;
        if no_progress {
            blockers.push("icloud-file-provider-no-progress".into());
        }
        if materialization_failed {
            blockers.push("icloud-file-provider-materialization-failed".into());
        }
        if activity
            .notices
            .iter()
            .any(|notice| notice == "icloud-file-provider-item-locked-observed")
        {
            blockers.push("icloud-file-provider-item-locked".into());
        }
        if activity
            .notices
            .iter()
            .any(|notice| notice == "icloud-file-provider-stale-error-observed")
        {
            blockers.push("icloud-file-provider-stalled".into());
        }
        if activity.sync_excluded_filename_count > 0 {
            blockers.push("icloud-file-provider-filename-excluded".into());
        }
        if activity.sync_excluded_root_count > 0 {
            blockers.push("icloud-file-provider-root-excluded".into());
        }
        if activity.pending_indexable_count.is_some_and(|count| count > 0) {
            blockers.push("icloud-file-provider-indexing-pending".into());
        }
        if activity
            .notices
            .iter()
            .any(|notice| notice == "icloud-file-provider-disk-import-active")
        {
            blockers.push("icloud-file-provider-disk-import-active".into());
        }
        if !no_progress && !materialization_failed
            && (activity.active_upload_count > 0 || activity.active_download_count > 0)
        {
            blockers.push("icloud-file-provider-transfer-active".into());
        } else if !no_progress && !materialization_failed && activity.timed_out {
            blockers.push("icloud-file-provider-dump-timeout".into());
        } else if !no_progress && !materialization_failed && activity.output_truncated {
            blockers.push("icloud-file-provider-dump-output-truncated".into());
        } else if !no_progress && !materialization_failed && !activity.command_succeeded {
            blockers.push("icloud-file-provider-evidence-unavailable".into());
        }
    }
    blockers
}

fn validate_icloud_health(
    provider: CloudProvider,
    report: Option<&IcloudSyncHealthReport>,
) -> Result<(Option<IcloudNewCopyAdmissionSummary>, Option<bool>), String> {
    if provider != CloudProvider::Icloud {
        if report.is_some() {
            return Err("naruon-copy-readiness-icloud-evidence-unexpected".into());
        }
        return Ok((None, None));
    }
    let Some(report) = report else {
        return Ok((None, Some(false)));
    };
    if report.schema_version != ICLOUD_SYNC_HEALTH_SCHEMA_VERSION
        || report.provider != "icloud"
        || report.output_mode != "icloud-local-sync-health"
        || report.evidence_kind != "supplementary-local-cloud-docs-private-schema"
        || !report.paths_redacted
        || report.user_filenames_read
        || report.user_file_contents_read
        || report.remote_capacity_verified
        || report.provider_sync_attested
        || report.local_eviction_authorized
        || report.mutation_performed
        || report.database_sidecar_write_permitted
        || (!report.evidence_complete && report.database_snapshot_includes_wal)
    {
        return Err("naruon-copy-readiness-icloud-claim-invalid".into());
    }
    if let Some(native_status) = report.native_status.as_ref() {
        validate_native_status_evidence(native_status)
            .map_err(|_| "naruon-copy-readiness-icloud-native-status-invalid".to_string())?;
        if native_status.observed_at_ms != report.observed_at_ms {
            return Err("naruon-copy-readiness-icloud-native-status-time-mismatch".into());
        }
    }
    if let Some(activity) = report.file_provider_activity.as_ref() {
        validate_file_provider_activity_evidence(activity)
            .map_err(|_| "naruon-copy-readiness-icloud-file-provider-activity-invalid".to_string())?;
        if activity.observed_at_ms != report.observed_at_ms {
            return Err("naruon-copy-readiness-icloud-file-provider-activity-time-mismatch".into());
        }
    }
    let reported_blockers = expected_icloud_admission_blockers(report);
    let reported_state = if reported_blockers.is_empty() {
        "clear"
    } else {
        "blocked"
    };
    if report.new_copy_admission_state != reported_state
        || report.new_copy_admission_blockers != reported_blockers
        || report.upload_queue.scheduled_count
            != report
                .upload_queue
                .scheduled_waiting_count
                .checked_add(report.upload_queue.scheduled_active_count)
                .ok_or_else(|| "naruon-copy-readiness-icloud-count-overflow".to_string())?
        || report.upload_queue.scheduled_bytes
            != report
                .upload_queue
                .scheduled_waiting_bytes
                .checked_add(report.upload_queue.scheduled_active_bytes)
                .ok_or_else(|| "naruon-copy-readiness-icloud-bytes-overflow".to_string())?
        || report.upload_queue.item_error_count
            != report
                .upload_queue
                .item_error_octagon_not_signed_in_count
                .checked_add(report.upload_queue.item_error_unclassified_count)
                .ok_or_else(|| "naruon-copy-readiness-icloud-item-error-overflow".to_string())?
    {
        return Err("naruon-copy-readiness-icloud-shape-invalid".into());
    }
    let mut exported_blockers = reported_blockers;
    if !report.evidence_complete {
        exported_blockers.push("icloud-new-copy-admission-evidence-unavailable".into());
    }
    let exported_state = if exported_blockers.is_empty() {
        "clear"
    } else {
        "blocked"
    };
    let admission_met = exported_blockers.is_empty();
    Ok((
        Some(IcloudNewCopyAdmissionSummary {
            observed_at_ms: report.observed_at_ms,
            state: exported_state.into(),
            scheduled_waiting_count: report.upload_queue.scheduled_waiting_count,
            scheduled_waiting_bytes: report.upload_queue.scheduled_waiting_bytes,
            scheduled_active_count: report.upload_queue.scheduled_active_count,
            scheduled_active_bytes: report.upload_queue.scheduled_active_bytes,
            scheduled_count: report.upload_queue.scheduled_count,
            scheduled_bytes: report.upload_queue.scheduled_bytes,
            blocked_on_sync_up_count: report.upload_queue.blocked_on_sync_up_count,
            out_of_quota_count: report.upload_queue.out_of_quota_count,
            out_of_quota_bytes: report.upload_queue.out_of_quota_bytes,
            other_state_count: report.upload_queue.other_state_count,
            item_error_count: report.upload_queue.item_error_count,
            item_error_octagon_not_signed_in_count: report
                .upload_queue
                .item_error_octagon_not_signed_in_count,
            item_error_unclassified_count: report.upload_queue.item_error_unclassified_count,
            newest_item_error_timestamp_ms: report.upload_queue.newest_item_error_timestamp_ms,
            newest_item_error_age_ms: report
                .upload_queue
                .newest_item_error_timestamp_ms
                .and_then(|timestamp| report.observed_at_ms.checked_sub(timestamp)),
            blockers: exported_blockers,
            evidence_complete: report.evidence_complete,
            database_snapshot_includes_wal: report.database_snapshot_includes_wal,
            native_status: report.native_status.clone(),
            file_provider_activity: report.file_provider_activity.clone(),
        }),
        Some(admission_met),
    ))
}

fn canonical_fingerprint(envelope: &NaruonCloudCopyReadinessEnvelope) -> Result<String, String> {
    let mut unsigned = envelope.clone();
    unsigned.readiness_fingerprint_sha256.clear();
    let value = serde_json::to_value(&unsigned)
        .map_err(|_| "naruon-copy-readiness-json-invalid".to_string())?;
    let mut bytes = Vec::new();
    append_canonical_json(&value, &mut bytes)?;
    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(64);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in digest {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    Ok(encoded)
}

fn append_canonical_json(value: &serde_json::Value, output: &mut Vec<u8>) -> Result<(), String> {
    match value {
        serde_json::Value::Null => output.extend_from_slice(b"null"),
        serde_json::Value::Bool(value) => {
            output.extend_from_slice(if *value { b"true" } else { b"false" })
        }
        serde_json::Value::Number(value) => output.extend_from_slice(value.to_string().as_bytes()),
        serde_json::Value::String(value) => {
            output.extend_from_slice(
                serde_json::to_string(value)
                    .map_err(|_| "naruon-copy-readiness-json-invalid".to_string())?
                    .as_bytes(),
            );
        }
        serde_json::Value::Array(values) => {
            output.push(b'[');
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    output.push(b',');
                }
                append_canonical_json(value, output)?;
            }
            output.push(b']');
        }
        serde_json::Value::Object(values) => {
            output.push(b'{');
            let mut keys = values.keys().collect::<Vec<_>>();
            keys.sort_unstable();
            for (index, key) in keys.into_iter().enumerate() {
                if index > 0 {
                    output.push(b',');
                }
                output.extend_from_slice(
                    serde_json::to_string(key)
                        .map_err(|_| "naruon-copy-readiness-json-invalid".to_string())?
                        .as_bytes(),
                );
                output.push(b':');
                append_canonical_json(
                    values
                        .get(key)
                        .ok_or_else(|| "naruon-copy-readiness-json-invalid".to_string())?,
                    output,
                )?;
            }
            output.push(b'}');
        }
    }
    Ok(())
}

fn add_blocker(
    counts: &mut BTreeMap<String, CountBytes>,
    blocker: String,
    bytes: u64,
) -> Result<(), String> {
    if !is_reason_code(&blocker) {
        return Err("naruon-copy-readiness-blocker-invalid".into());
    }
    add_count_bytes(counts.entry(blocker).or_default(), bytes)
}

fn pre_copy_evidence_is_complete(cohort: Option<&PreCopyEvidenceCohort>) -> bool {
    let Some(cohort) = cohort else {
        return false;
    };
    if cohort.schema_version != crate::cloud::PRE_COPY_EVIDENCE_COHORT_SCHEMA_VERSION {
        return false;
    }
    let recomputed = crate::cloud::compare_pre_copy_evidence(cohort.observations.clone());
    recomputed.observed_at_ms == cohort.observed_at_ms
        && recomputed.complete == cohort.complete
        && recomputed.blockers == cohort.blockers
        && recomputed.cohort_fingerprint == cohort.cohort_fingerprint
        && cohort.complete
        && cohort.blockers.is_empty()
}

pub fn export_naruon_cloud_copy_readiness(
    report: &CloudPlanReport,
    runtime: &ProviderClientRuntimeSnapshot,
    icloud_health: Option<&IcloudSyncHealthReport>,
) -> Result<NaruonCloudCopyReadinessEnvelope, String> {
    export_naruon_cloud_copy_readiness_with_global_sync(report, runtime, icloud_health, None)
}

pub fn export_naruon_cloud_copy_readiness_with_global_sync(
    report: &CloudPlanReport,
    runtime: &ProviderClientRuntimeSnapshot,
    icloud_health: Option<&IcloudSyncHealthReport>,
    provider_global_sync: Option<&ProviderGlobalSyncReport>,
) -> Result<NaruonCloudCopyReadinessEnvelope, String> {
    if report
        .notices
        .iter()
        .any(|notice| notice == "source-scan-incomplete")
    {
        return Err("naruon-copy-readiness-source-scan-incomplete".into());
    }
    provider_client_runtime::validate_provider_client_runtime_snapshot(runtime)?;
    if runtime.provider != report.cloud_root.provider {
        return Err("naruon-copy-readiness-runtime-provider-mismatch".into());
    }
    let capacity = naruon_capacity::export_naruon_cloud_capacity_assessment(report)?.capacity;
    let remote_capacity_verified =
        capacity.snapshot.evidence_kind != CapacityEvidenceKind::Unavailable;
    let (icloud_new_copy_admission, icloud_new_copy_admission_met) =
        validate_icloud_health(report.cloud_root.provider, icloud_health)?;
    let pre_copy_evidence_met = if report.cloud_root.provider == CloudProvider::Icloud {
        Some(pre_copy_evidence_is_complete(
            report.pre_copy_evidence.as_ref(),
        ))
    } else {
        None
    };
    let (provider_global_sync, provider_global_sync_blockers) =
        validate_provider_global_sync_input(report.cloud_root.provider, provider_global_sync)?;

    let mut planner_unblocked = CountBytes::default();
    let mut requires_human_review = CountBytes::default();
    let mut ready_without_new_review = CountBytes::default();
    let mut production_time_evidence = ProductionTimeEvidenceSummary::default();
    let mut candidate_blocker_counts = BTreeMap::<String, CountBytes>::new();

    for candidate in &report.candidates {
        add_count_bytes(
            production_summary_target(
                &mut production_time_evidence,
                &candidate.production_time_source,
            ),
            candidate.bytes,
        )?;
        if candidate.blocked_reason.is_none() {
            add_count_bytes(&mut planner_unblocked, candidate.bytes)?;
        }
        if candidate.requires_review {
            add_count_bytes(&mut requires_human_review, candidate.bytes)?;
        }

        let mut blockers = cloud_transfer::candidate_blockers(candidate, &report.cloud_root);
        if !runtime.copy_prerequisite_met {
            blockers.push(
                runtime
                    .blocker
                    .clone()
                    .unwrap_or_else(|| "provider-client-runtime-verification-required".into()),
            );
        }
        let candidate_capacity = provider_capacity::assess_capacity(
            capacity.snapshot.clone(),
            candidate.bytes,
            candidate.bytes,
            capacity.reserve_bytes,
        );
        if candidate_capacity.can_fit != Some(true) {
            if candidate_capacity.blockers.is_empty() {
                blockers.push("cloud-capacity-verification-required".into());
            } else {
                blockers.extend(candidate_capacity.blockers);
            }
        }
        if report.cloud_root.provider == CloudProvider::Icloud {
            match &icloud_new_copy_admission {
                Some(admission) => blockers.extend(admission.blockers.clone()),
                None => blockers.push("icloud-new-copy-admission-evidence-unavailable".into()),
            }
            if pre_copy_evidence_met != Some(true) {
                blockers.push(PRE_COPY_EVIDENCE_BLOCKER.into());
            }
        } else {
            blockers.extend(provider_global_sync_blockers.iter().cloned());
        }
        blockers.sort();
        blockers.dedup();
        if blockers.is_empty() {
            add_count_bytes(&mut ready_without_new_review, candidate.bytes)?;
        } else {
            for blocker in blockers {
                add_blocker(&mut candidate_blocker_counts, blocker, candidate.bytes)?;
            }
        }
    }

    let candidate_count = u64::try_from(report.candidates.len())
        .map_err(|_| "naruon-copy-readiness-count-overflow".to_string())?;
    let readiness_state = if candidate_count == 0 {
        CloudCopyReadinessState::NoCandidates
    } else if ready_without_new_review.count == 0 {
        CloudCopyReadinessState::Blocked
    } else if ready_without_new_review.count == candidate_count {
        CloudCopyReadinessState::ReadyWithoutNewReview
    } else {
        CloudCopyReadinessState::PartiallyReady
    };
    let mut envelope = NaruonCloudCopyReadinessEnvelope {
        schema_kind: NARUON_CLOUD_COPY_READINESS_SCHEMA_KIND.into(),
        schema_version: NARUON_CLOUD_COPY_READINESS_SCHEMA_VERSION,
        generated_at_ms: report
            .generated_at_ms
            .max(runtime.observed_at_ms)
            .max(capacity.snapshot.observed_at_ms)
            .max(
                icloud_new_copy_admission
                    .as_ref()
                    .map(|value| value.observed_at_ms)
                    .unwrap_or_default(),
            )
            .max(
                report
                    .pre_copy_evidence
                    .as_ref()
                    .map(|value| value.observed_at_ms)
                    .unwrap_or_default(),
            ),
        decision_batch_fingerprint_version: crate::cloud::CLOUD_DECISION_BATCH_FINGERPRINT_VERSION,
        decision_batch_fingerprint: crate::cloud::cloud_decision_batch_fingerprint(report),
        provider: report.cloud_root.provider,
        destination_account_scope: report.cloud_root.account_scope,
        source_selection_policy: report
            .source_selection_policy
            .ok_or_else(|| "naruon-copy-readiness-selection-policy-missing".to_string())?,
        candidate_count,
        candidate_bytes: report.candidate_bytes,
        potentially_reclaimable_bytes: report.potentially_reclaimable_bytes,
        planner_unblocked,
        requires_human_review,
        ready_without_new_review,
        readiness_state,
        production_time_evidence,
        candidate_blocker_counts,
        provider_runtime: runtime.clone(),
        capacity,
        icloud_new_copy_admission,
        pre_copy_evidence: report.pre_copy_evidence.clone(),
        provider_global_sync,
        provider_runtime_prerequisite_met: runtime.copy_prerequisite_met,
        remote_capacity_verified,
        icloud_new_copy_admission_met,
        pre_copy_evidence_met,
        human_review_decisions_applied: false,
        metadata_policy: vec![
            "embedded-metadata".into(),
            "explicit-filename-date".into(),
            "filesystem-created".into(),
            "filesystem-modified".into(),
        ],
        filename_dates_are_auxiliary: true,
        readiness_fingerprint_canonicalization: FINGERPRINT_CANONICALIZATION.into(),
        readiness_fingerprint_sha256: String::new(),
        local_paths_included: false,
        relative_names_included: false,
        raw_metadata_values_included: false,
        account_identifiers_included: false,
        provider_sync_attested: false,
        cloud_write_executed: false,
        source_eviction_authorized: false,
    };
    envelope.readiness_fingerprint_sha256 = canonical_fingerprint(&envelope)?;
    validate_naruon_cloud_copy_readiness(&envelope)?;
    Ok(envelope)
}

fn validate_provider_global_sync_input(
    provider: CloudProvider,
    report: Option<&ProviderGlobalSyncReport>,
) -> Result<(Option<ProviderGlobalSyncReport>, Vec<String>), String> {
    if provider == CloudProvider::Icloud {
        if report.is_some() {
            return Err("naruon-copy-readiness-provider-global-sync-icloud-invalid".into());
        }
        return Ok((None, Vec::new()));
    }
    let Some(report) = report else {
        return Ok((
            None,
            vec!["provider-global-sync-evidence-unavailable".into()],
        ));
    };
    if report.schema_version != provider_global_sync::PROVIDER_GLOBAL_SYNC_SCHEMA_VERSION
        || report.provider != provider
        || report.evidence_kind != "fileproviderctl-global-dump"
        || !report.evidence_complete
        || report
            .blockers
            .iter()
            .any(|blocker| !is_reason_code(blocker))
        || (report.state == ProviderGlobalSyncState::Clear && !report.blockers.is_empty())
        || (report.state == ProviderGlobalSyncState::Clear
            && (report.upload_progress_present
                || report.download_progress_present
                || report.pending_indexable_count.is_some_and(|count| count > 0)))
        || (report.state != ProviderGlobalSyncState::Clear && report.blockers.is_empty())
    {
        return Err("naruon-copy-readiness-provider-global-sync-invalid".into());
    }
    Ok((Some(report.clone()), report.blockers.clone()))
}

pub fn validate_naruon_cloud_copy_readiness(
    envelope: &NaruonCloudCopyReadinessEnvelope,
) -> Result<(), String> {
    if envelope.schema_kind != NARUON_CLOUD_COPY_READINESS_SCHEMA_KIND
        || envelope.schema_version != NARUON_CLOUD_COPY_READINESS_SCHEMA_VERSION
        || envelope.decision_batch_fingerprint_version
            != crate::cloud::CLOUD_DECISION_BATCH_FINGERPRINT_VERSION
        || !is_lower_hex_64(&envelope.decision_batch_fingerprint)
        || envelope.readiness_fingerprint_canonicalization != FINGERPRINT_CANONICALIZATION
    {
        return Err("naruon-copy-readiness-schema-invalid".into());
    }
    if envelope.local_paths_included
        || envelope.relative_names_included
        || envelope.raw_metadata_values_included
        || envelope.account_identifiers_included
        || envelope.provider_sync_attested
        || envelope.cloud_write_executed
        || envelope.source_eviction_authorized
        || envelope.human_review_decisions_applied
        || !envelope.filename_dates_are_auxiliary
        || envelope.metadata_policy
            != [
                "embedded-metadata",
                "explicit-filename-date",
                "filesystem-created",
                "filesystem-modified",
            ]
    {
        return Err("naruon-copy-readiness-policy-claim-invalid".into());
    }
    if envelope.source_selection_policy.min_size_bytes == 0
        || envelope.source_selection_policy.limit == 0
        || envelope.source_selection_policy.limit > 1_000
    {
        return Err("naruon-copy-readiness-selection-policy-invalid".into());
    }
    provider_client_runtime::validate_provider_client_runtime_snapshot(&envelope.provider_runtime)?;
    naruon_capacity::validate_cloud_capacity_assessment(&envelope.capacity)?;
    if envelope.provider_runtime.provider != envelope.provider
        || envelope.capacity.snapshot.provider != envelope.provider
        || envelope.provider_runtime_prerequisite_met
            != envelope.provider_runtime.copy_prerequisite_met
        || envelope.remote_capacity_verified
            != (envelope.capacity.snapshot.evidence_kind != CapacityEvidenceKind::Unavailable)
    {
        return Err("naruon-copy-readiness-provider-binding-invalid".into());
    }
    if envelope
        .capacity
        .snapshot
        .account_scope
        .is_some_and(|scope| scope != envelope.destination_account_scope)
        || envelope.capacity.requested_bytes != envelope.potentially_reclaimable_bytes
        || envelope.capacity.largest_candidate_bytes > envelope.candidate_bytes
        || envelope.generated_at_ms < envelope.provider_runtime.observed_at_ms
        || envelope.generated_at_ms < envelope.capacity.snapshot.observed_at_ms
    {
        return Err("naruon-copy-readiness-capacity-binding-invalid".into());
    }
    match (
        envelope.provider,
        envelope.icloud_new_copy_admission.as_ref(),
        envelope.icloud_new_copy_admission_met,
    ) {
        (CloudProvider::Icloud, Some(summary), Some(met)) => {
            validate_icloud_admission_summary(summary)?;
            if met != summary.blockers.is_empty()
                || envelope.generated_at_ms < summary.observed_at_ms
            {
                return Err("naruon-copy-readiness-icloud-binding-invalid".into());
            }
        }
        (CloudProvider::Icloud, None, Some(false)) => {}
        (CloudProvider::Icloud, _, _) => {
            return Err("naruon-copy-readiness-icloud-binding-invalid".into())
        }
        (_, None, None) => {}
        _ => return Err("naruon-copy-readiness-icloud-binding-invalid".into()),
    }
    match (
        envelope.provider,
        envelope.pre_copy_evidence.as_ref(),
        envelope.pre_copy_evidence_met,
    ) {
        (CloudProvider::Icloud, Some(cohort), Some(met)) => {
            let recomputed = crate::cloud::compare_pre_copy_evidence(cohort.observations.clone());
            if cohort.schema_version != crate::cloud::PRE_COPY_EVIDENCE_COHORT_SCHEMA_VERSION
                || recomputed.observed_at_ms != cohort.observed_at_ms
                || recomputed.complete != cohort.complete
                || recomputed.blockers != cohort.blockers
                || recomputed.cohort_fingerprint != cohort.cohort_fingerprint
                || met != pre_copy_evidence_is_complete(Some(cohort))
                || envelope.generated_at_ms < cohort.observed_at_ms
            {
                return Err("naruon-copy-readiness-pre-copy-evidence-invalid".into());
            }
        }
        (CloudProvider::Icloud, None, Some(false)) => {}
        (CloudProvider::Icloud, None, _) => {
            return Err("naruon-copy-readiness-pre-copy-evidence-invalid".into())
        }
        (_, None, None) => {}
        _ => return Err("naruon-copy-readiness-pre-copy-evidence-invalid".into()),
    }
    for blocker in envelope.candidate_blocker_counts.keys() {
        if !is_reason_code(blocker) {
            return Err("naruon-copy-readiness-blocker-invalid".into());
        }
    }
    if envelope.candidate_count > 1_000 || envelope.candidate_blocker_counts.len() > 128 {
        return Err("naruon-copy-readiness-bounds-invalid".into());
    }
    let production_parts = [
        &envelope.production_time_evidence.embedded_metadata,
        &envelope.production_time_evidence.explicit_filename_date,
        &envelope.production_time_evidence.filesystem_created,
        &envelope.production_time_evidence.filesystem_modified,
        &envelope.production_time_evidence.unclassified,
    ];
    let production_count = production_parts.iter().try_fold(0u64, |total, item| {
        total
            .checked_add(item.count)
            .ok_or_else(|| "naruon-copy-readiness-count-overflow".to_string())
    })?;
    let production_bytes = production_parts.iter().try_fold(0u64, |total, item| {
        total
            .checked_add(item.bytes)
            .ok_or_else(|| "naruon-copy-readiness-bytes-overflow".to_string())
    })?;
    if production_count != envelope.candidate_count
        || production_bytes != envelope.candidate_bytes
        || envelope.planner_unblocked.count > envelope.candidate_count
        || envelope.requires_human_review.count > envelope.candidate_count
        || envelope.ready_without_new_review.count > envelope.candidate_count
        || envelope.planner_unblocked.bytes > envelope.candidate_bytes
        || envelope.requires_human_review.bytes > envelope.candidate_bytes
        || envelope.ready_without_new_review.bytes > envelope.candidate_bytes
        || envelope.ready_without_new_review.count > envelope.planner_unblocked.count
        || envelope.ready_without_new_review.bytes > envelope.planner_unblocked.bytes
        || envelope.planner_unblocked.bytes != envelope.potentially_reclaimable_bytes
    {
        return Err("naruon-copy-readiness-aggregate-invalid".into());
    }
    if envelope.candidate_blocker_counts.values().any(|aggregate| {
        aggregate.count > envelope.candidate_count || aggregate.bytes > envelope.candidate_bytes
    }) {
        return Err("naruon-copy-readiness-blocker-aggregate-invalid".into());
    }
    match envelope.candidate_blocker_counts.get("review-required") {
        Some(aggregate) if aggregate != &envelope.requires_human_review => {
            return Err("naruon-copy-readiness-review-binding-invalid".into())
        }
        None if envelope.requires_human_review.count > 0 => {
            return Err("naruon-copy-readiness-review-binding-invalid".into())
        }
        Some(_) if envelope.requires_human_review.count == 0 => {
            return Err("naruon-copy-readiness-review-binding-invalid".into())
        }
        _ => {}
    }
    let planner_blocked = CountBytes {
        count: envelope
            .candidate_count
            .checked_sub(envelope.planner_unblocked.count)
            .ok_or_else(|| "naruon-copy-readiness-count-overflow".to_string())?,
        bytes: envelope
            .candidate_bytes
            .checked_sub(envelope.planner_unblocked.bytes)
            .ok_or_else(|| "naruon-copy-readiness-bytes-overflow".to_string())?,
    };
    match envelope.candidate_blocker_counts.get("planner-blocked") {
        Some(aggregate) if aggregate != &planner_blocked => {
            return Err("naruon-copy-readiness-planner-binding-invalid".into())
        }
        None if planner_blocked.count > 0 => {
            return Err("naruon-copy-readiness-planner-binding-invalid".into())
        }
        Some(_) if planner_blocked.count == 0 => {
            return Err("naruon-copy-readiness-planner-binding-invalid".into())
        }
        _ => {}
    }
    if envelope.candidate_count == 0 && !envelope.candidate_blocker_counts.is_empty() {
        return Err("naruon-copy-readiness-empty-blockers-invalid".into());
    }
    if envelope
        .ready_without_new_review
        .count
        .checked_add(envelope.requires_human_review.count)
        .is_none_or(|count| count > envelope.candidate_count)
        || envelope
            .ready_without_new_review
            .bytes
            .checked_add(envelope.requires_human_review.bytes)
            .is_none_or(|bytes| bytes > envelope.candidate_bytes)
    {
        return Err("naruon-copy-readiness-review-overlap-invalid".into());
    }
    if envelope.ready_without_new_review.count > 0 {
        if !envelope.provider_runtime_prerequisite_met
            || !envelope.remote_capacity_verified
            || envelope.icloud_new_copy_admission_met == Some(false)
            || envelope.pre_copy_evidence_met == Some(false)
        {
            return Err("naruon-copy-readiness-ready-gate-invalid".into());
        }
    }
    if envelope.candidate_count > 0 && !envelope.provider_runtime_prerequisite_met {
        let Some(blocker) = envelope.provider_runtime.blocker.as_ref() else {
            return Err("naruon-copy-readiness-runtime-binding-invalid".into());
        };
        if envelope.candidate_blocker_counts.get(blocker)
            != Some(&CountBytes {
                count: envelope.candidate_count,
                bytes: envelope.candidate_bytes,
            })
        {
            return Err("naruon-copy-readiness-runtime-binding-invalid".into());
        }
    } else if RUNTIME_BLOCKERS
        .iter()
        .any(|blocker| envelope.candidate_blocker_counts.contains_key(*blocker))
    {
        return Err("naruon-copy-readiness-runtime-binding-invalid".into());
    }
    if envelope.candidate_count > 0
        && envelope.capacity.snapshot.evidence_kind == CapacityEvidenceKind::Unavailable
    {
        let Some(blocker) = envelope.capacity.snapshot.unavailable_reason.as_ref() else {
            return Err("naruon-copy-readiness-capacity-binding-invalid".into());
        };
        if envelope.candidate_blocker_counts.get(blocker)
            != Some(&CountBytes {
                count: envelope.candidate_count,
                bytes: envelope.candidate_bytes,
            })
        {
            return Err("naruon-copy-readiness-capacity-binding-invalid".into());
        }
    }
    if envelope.candidate_count > 0 && envelope.provider == CloudProvider::Icloud {
        let expected = CountBytes {
            count: envelope.candidate_count,
            bytes: envelope.candidate_bytes,
        };
        if let Some(summary) = &envelope.icloud_new_copy_admission {
            for blocker in &summary.blockers {
                if envelope.candidate_blocker_counts.get(blocker) != Some(&expected) {
                    return Err("naruon-copy-readiness-icloud-binding-invalid".into());
                }
            }
            if ICLOUD_ADMISSION_BLOCKERS.iter().any(|blocker| {
                !summary.blockers.iter().any(|expected| expected == blocker)
                    && envelope.candidate_blocker_counts.contains_key(*blocker)
            }) {
                return Err("naruon-copy-readiness-icloud-binding-invalid".into());
            }
        } else if envelope
            .candidate_blocker_counts
            .get("icloud-new-copy-admission-evidence-unavailable")
            != Some(&expected)
        {
            return Err("naruon-copy-readiness-icloud-binding-invalid".into());
        }
        match envelope.pre_copy_evidence_met {
            Some(false)
                if envelope
                    .candidate_blocker_counts
                    .get(PRE_COPY_EVIDENCE_BLOCKER)
                    == Some(&expected) => {}
            Some(true)
                if envelope
                    .candidate_blocker_counts
                    .contains_key(PRE_COPY_EVIDENCE_BLOCKER) =>
            {
                return Err("naruon-copy-readiness-pre-copy-evidence-invalid".into())
            }
            Some(false) => return Err("naruon-copy-readiness-pre-copy-evidence-invalid".into()),
            Some(true) => {}
            None => return Err("naruon-copy-readiness-pre-copy-evidence-invalid".into()),
        }
    } else if ICLOUD_ADMISSION_BLOCKERS
        .iter()
        .any(|blocker| envelope.candidate_blocker_counts.contains_key(*blocker))
    {
        return Err("naruon-copy-readiness-icloud-binding-invalid".into());
    } else if envelope
        .candidate_blocker_counts
        .contains_key(PRE_COPY_EVIDENCE_BLOCKER)
    {
        return Err("naruon-copy-readiness-pre-copy-evidence-invalid".into());
    }
    let expected_provider_global_sync_blockers = validate_provider_global_sync_input(
        envelope.provider,
        envelope.provider_global_sync.as_ref(),
    )
    .map_err(|_| "naruon-copy-readiness-provider-global-sync-binding-invalid".to_string())?
    .1;
    if envelope.candidate_blocker_counts.keys().any(|blocker| {
        blocker.starts_with("provider-global-sync-")
            && !expected_provider_global_sync_blockers
                .iter()
                .any(|expected| expected == blocker)
    }) {
        return Err("naruon-copy-readiness-provider-global-sync-binding-invalid".into());
    }
    if envelope.candidate_count > 0 {
        let expected = CountBytes {
            count: envelope.candidate_count,
            bytes: envelope.candidate_bytes,
        };
        for blocker in expected_provider_global_sync_blockers {
            if envelope.candidate_blocker_counts.get(&blocker) != Some(&expected) {
                return Err("naruon-copy-readiness-provider-global-sync-binding-invalid".into());
            }
        }
    }
    let expected_state = if envelope.candidate_count == 0 {
        CloudCopyReadinessState::NoCandidates
    } else if envelope.ready_without_new_review.count == 0 {
        CloudCopyReadinessState::Blocked
    } else if envelope.ready_without_new_review.count == envelope.candidate_count {
        CloudCopyReadinessState::ReadyWithoutNewReview
    } else {
        CloudCopyReadinessState::PartiallyReady
    };
    if envelope.readiness_state != expected_state {
        return Err("naruon-copy-readiness-state-invalid".into());
    }
    if !is_lower_hex_64(&envelope.readiness_fingerprint_sha256)
        || envelope.readiness_fingerprint_sha256 != canonical_fingerprint(envelope)?
    {
        return Err("naruon-copy-readiness-fingerprint-invalid".into());
    }
    Ok(())
}

fn validate_icloud_admission_summary(
    summary: &IcloudNewCopyAdmissionSummary,
) -> Result<(), String> {
    let scheduled_count = summary
        .scheduled_waiting_count
        .checked_add(summary.scheduled_active_count)
        .ok_or_else(|| "naruon-copy-readiness-icloud-count-overflow".to_string())?;
    let scheduled_bytes = summary
        .scheduled_waiting_bytes
        .checked_add(summary.scheduled_active_bytes)
        .ok_or_else(|| "naruon-copy-readiness-icloud-bytes-overflow".to_string())?;
    let mut expected = Vec::new();
    if !summary.evidence_complete {
        expected.push("icloud-sync-health-evidence-incomplete".to_string());
    }
    if summary.scheduled_waiting_count > 0 {
        expected.push("icloud-upload-queue-nonempty".to_string());
    }
    if summary.scheduled_active_count > 0 {
        expected.push("icloud-upload-in-flight".to_string());
    }
    if summary.blocked_on_sync_up_count > 0 {
        expected.push("icloud-upload-blocked-on-sync-up".to_string());
    }
    if summary.out_of_quota_count > 0 {
        expected.push("icloud-upload-out-of-quota".to_string());
    }
    if summary.other_state_count > 0 {
        expected.push("icloud-upload-queue-state-unclassified".to_string());
    }
    if summary.item_error_count > 0 {
        expected.push("icloud-local-sync-item-error-present".to_string());
    }
    if summary
        .native_status
        .as_ref()
        .is_some_and(|status| !status.evidence_complete)
    {
        expected.push("icloud-native-status-evidence-incomplete".to_string());
    }
    if summary
        .native_status
        .as_ref()
        .is_some_and(|status| status.timed_out)
    {
        expected.push("icloud-native-status-command-timeout".to_string());
    }
    if summary.native_status.as_ref().is_some_and(native_sync_up_pending) {
        expected.push("icloud-native-sync-up-pending".to_string());
    }
    if summary
        .native_status
        .as_ref()
        .is_some_and(native_sync_down_pending)
    {
        expected.push("icloud-native-sync-down-pending".to_string());
    }
    if summary.native_status.as_ref().is_some_and(native_pending_scan) {
        expected.push("icloud-native-status-pending-scan".to_string());
    }
    if let Some(activity) = summary.file_provider_activity.as_ref() {
        let no_progress = activity.no_progress_fetch_count > 0
            || activity.no_progress_create_count > 0;
        let materialization_failed = activity.materialization_failure_count > 0
            || activity.staged_item_missing_count > 0;
        if no_progress {
            expected.push("icloud-file-provider-no-progress".to_string());
        }
        if materialization_failed {
            expected.push("icloud-file-provider-materialization-failed".to_string());
        }
        if activity
            .notices
            .iter()
            .any(|notice| notice == "icloud-file-provider-item-locked-observed")
        {
            expected.push("icloud-file-provider-item-locked".to_string());
        }
        if activity
            .notices
            .iter()
            .any(|notice| notice == "icloud-file-provider-stale-error-observed")
        {
            expected.push("icloud-file-provider-stalled".to_string());
        }
        if activity.sync_excluded_filename_count > 0 {
            expected.push("icloud-file-provider-filename-excluded".to_string());
        }
        if activity.sync_excluded_root_count > 0 {
            expected.push("icloud-file-provider-root-excluded".to_string());
        }
        if activity.pending_indexable_count.is_some_and(|count| count > 0) {
            expected.push("icloud-file-provider-indexing-pending".to_string());
        }
        if activity
            .notices
            .iter()
            .any(|notice| notice == FILE_PROVIDER_DISK_IMPORT_NOTICE)
        {
            expected.push(FILE_PROVIDER_DISK_IMPORT_NOTICE.to_string());
        }
        if !no_progress && !materialization_failed
            && (activity.active_upload_count > 0 || activity.active_download_count > 0)
        {
            expected.push("icloud-file-provider-transfer-active".to_string());
        } else if !no_progress && !materialization_failed && activity.timed_out {
            expected.push("icloud-file-provider-dump-timeout".to_string());
        } else if !no_progress && !materialization_failed && activity.output_truncated {
            expected.push("icloud-file-provider-dump-output-truncated".to_string());
        } else if !no_progress && !materialization_failed && !activity.command_succeeded {
            expected.push("icloud-file-provider-evidence-unavailable".to_string());
        }
    }
    if !summary.evidence_complete {
        expected.push("icloud-new-copy-admission-evidence-unavailable".to_string());
    }
    let expected_state = if expected.is_empty() {
        "clear"
    } else {
        "blocked"
    };
    if summary.scheduled_count != scheduled_count
        || summary.scheduled_bytes != scheduled_bytes
        || (!summary.evidence_complete && summary.database_snapshot_includes_wal)
        || (summary.scheduled_waiting_count == 0 && summary.scheduled_waiting_bytes != 0)
        || (summary.scheduled_active_count == 0 && summary.scheduled_active_bytes != 0)
        || (summary.out_of_quota_count == 0 && summary.out_of_quota_bytes != 0)
        || summary.item_error_count
            != summary
                .item_error_octagon_not_signed_in_count
                .checked_add(summary.item_error_unclassified_count)
                .ok_or_else(|| "naruon-copy-readiness-icloud-item-error-overflow".to_string())?
        || summary.newest_item_error_age_ms
            != summary
                .newest_item_error_timestamp_ms
                .and_then(|timestamp| summary.observed_at_ms.checked_sub(timestamp))
        || summary
            .native_status
            .as_ref()
            .is_some_and(|native_status| {
                validate_native_status_evidence(native_status).is_err()
                    || native_status.observed_at_ms != summary.observed_at_ms
            })
        || summary
            .file_provider_activity
            .as_ref()
            .is_some_and(|activity| {
                validate_file_provider_activity_evidence(activity).is_err()
                    || activity.observed_at_ms != summary.observed_at_ms
            })
        || summary.state != expected_state
        || summary.blockers != expected
    {
        return Err("naruon-copy-readiness-icloud-shape-invalid".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::{
        ArchiveKind, CloudAccountScope, CloudCandidate, CloudRoot, ExactDuplicateSummary,
        MetadataEvidence, PreCopyEvidenceObservation,
    };
    use crate::icloud_sync_health::{
        IcloudNativeStatusEvidence, IcloudUploadQueueSummary, ManagedDatabaseFileEvidence,
        ICLOUD_NATIVE_STATUS_SCHEMA_VERSION,
    };
    use crate::provider_capacity::{
        assess_capacity, unavailable_capacity, DEFAULT_CAPACITY_RESERVE_BYTES,
    };
    use crate::provider_client_runtime::assess_provider_client_runtime;

    fn resign(envelope: &mut NaruonCloudCopyReadinessEnvelope) {
        envelope.readiness_fingerprint_sha256 = canonical_fingerprint(envelope).unwrap();
    }

    fn candidate(source: &str, confidence: &str, review: bool) -> CloudCandidate {
        let mut candidate = CloudCandidate {
            metadata_fingerprint: "a".repeat(64),
            review_fingerprint: String::new(),
            src: "/private/source/report.pdf".into(),
            dst: "/private/cloud/DiskSage Archive/2026/07/document/report.pdf".into(),
            provider: CloudProvider::Onedrive,
            destination_account_scope: CloudAccountScope::Personal,
            kind: ArchiveKind::Document,
            bytes: 42,
            age_days: 100,
            created_ms: 1,
            modified_ms: 2,
            production_time_ms: 3,
            production_time_source: source.into(),
            production_time_confidence: confidence.into(),
            source_root: "/private/source".into(),
            relative_path: "report.pdf".into(),
            source_context: "downloads".into(),
            requires_review: review,
            review_reasons: if review {
                vec!["metadata-review-required".into()]
            } else {
                Vec::new()
            },
            content_title: Some("Private title".into()),
            content_authors: vec!["private@example.com".into()],
            content_context: vec!["private context".into()],
            duration_ms: None,
            dataset_profile: None,
            metadata_evidence: vec![MetadataEvidence {
                field: "production-date".into(),
                value: "private raw value".into(),
                source: source.into(),
                confidence: confidence.into(),
            }],
            blocked_reason: None,
        };
        candidate.review_fingerprint = crate::cloud::candidate_review_fingerprint(&candidate);
        candidate
    }

    fn report(provider: CloudProvider) -> CloudPlanReport {
        let scope = if provider == CloudProvider::GoogleDrive {
            crate::cloud::CloudAccountScope::Unknown
        } else {
            CloudAccountScope::Personal
        };
        let mut embedded = candidate("embedded:exiftool:CreateDate", "high", false);
        embedded.provider = provider;
        embedded.destination_account_scope = scope;
        embedded.review_fingerprint = crate::cloud::candidate_review_fingerprint(&embedded);
        let mut filesystem = candidate("filesystem:created", "low", true);
        filesystem.metadata_fingerprint = "b".repeat(64);
        filesystem.provider = provider;
        filesystem.destination_account_scope = scope;
        filesystem.review_fingerprint = crate::cloud::candidate_review_fingerprint(&filesystem);
        let snapshot = unavailable_capacity(provider, 10, "capacity-unavailable");
        CloudPlanReport {
            cloud_root: CloudRoot {
                id: "private-root-id".into(),
                provider,
                account_scope: scope,
                label: "Private root".into(),
                path: "/private/cloud".into(),
                readable: true,
                access_issue: None,
            },
            generated_at_ms: 20,
            source_selection_policy: Some(CloudPlanOptions {
                min_size_bytes: 90 * 1024 * 1024,
                min_age_days: 30,
                limit: 200,
            }),
            candidates: vec![embedded, filesystem],
            candidate_bytes: 84,
            potentially_reclaimable_bytes: 84,
            exact_duplicates: ExactDuplicateSummary::default(),
            capacity: Some(assess_capacity(
                snapshot,
                84,
                42,
                DEFAULT_CAPACITY_RESERVE_BYTES,
            )),
            local_volume: None,
            pre_copy_evidence: (provider == CloudProvider::Icloud).then(pre_copy_evidence),
            notices: Vec::new(),
        }
    }

    fn pre_copy_evidence() -> PreCopyEvidenceCohort {
        crate::cloud::compare_pre_copy_evidence(vec![
            PreCopyEvidenceObservation {
                stream: "icloud-sync-health-evidence".into(),
                observed_at_ms: 30,
                evidence_complete: true,
                fingerprint: "a".repeat(64),
            },
            PreCopyEvidenceObservation {
                stream: "provider-client-runtime-evidence".into(),
                observed_at_ms: 30,
                evidence_complete: true,
                fingerprint: "b".repeat(64),
            },
            PreCopyEvidenceObservation {
                stream: "volume-pressure-evidence".into(),
                observed_at_ms: 30,
                evidence_complete: true,
                fingerprint: "c".repeat(64),
            },
        ])
    }

    fn icloud_health(blocked: bool) -> IcloudSyncHealthReport {
        let queue = if blocked {
            IcloudUploadQueueSummary {
                scheduled_waiting_count: 2,
                scheduled_waiting_bytes: 20,
                scheduled_count: 2,
                scheduled_bytes: 20,
                item_error_count: 1,
                item_error_octagon_not_signed_in_count: 1,
                newest_item_error_timestamp_ms: Some(10),
                ..IcloudUploadQueueSummary::default()
            }
        } else {
            IcloudUploadQueueSummary::default()
        };
        let blockers = if blocked {
            vec![
                "icloud-upload-queue-nonempty".into(),
                "icloud-local-sync-item-error-present".into(),
            ]
        } else {
            Vec::new()
        };
        IcloudSyncHealthReport {
            schema_version: ICLOUD_SYNC_HEALTH_SCHEMA_VERSION,
            output_mode: "icloud-local-sync-health".into(),
            observed_at_ms: 30,
            admission_blocked_since_ms: None,
            provider: "icloud".into(),
            evidence_kind: "supplementary-local-cloud-docs-private-schema".into(),
            evidence_complete: true,
            database_snapshot_includes_wal: true,
            database_sidecar_write_permitted: false,
            managed_database_files: vec![ManagedDatabaseFileEvidence {
                role: "client.db".into(),
                present: true,
                logical_bytes: 1,
                allocated_bytes: 1,
                modified_ms: Some(1),
            }],
            managed_database_allocated_bytes: 1,
            upload_queue: queue,
            native_status: None,
            file_provider_activity: None,
            sync_backlog_present: blocked,
            new_copy_admission_state: if blocked {
                "blocked".into()
            } else {
                "clear".into()
            },
            new_copy_admission_blockers: blockers.clone(),
            blockers,
            notices: Vec::new(),
            paths_redacted: true,
            user_filenames_read: false,
            user_file_contents_read: false,
            remote_capacity_verified: false,
            provider_sync_attested: false,
            local_eviction_authorized: false,
            mutation_performed: false,
        }
    }

    fn native_sync_up_status() -> IcloudNativeStatusEvidence {
        IcloudNativeStatusEvidence {
            schema_version: ICLOUD_NATIVE_STATUS_SCHEMA_VERSION,
            observed_at_ms: 30,
            command_succeeded: false,
            timed_out: true,
            output_truncated: false,
            status_observed: true,
            evidence_complete: true,
            container_count: Some(1),
            client_state: Some("needs-sync".into()),
            server_state: Some("full-sync".into()),
            sync_state: Some("needs-sync-up".into()),
            last_sync_present: true,
            pending_scan_count: 0,
            notices: vec!["icloud-native-status-summary-observed".into()],
        }
    }

    #[test]
    fn partial_source_scan_never_exports_readiness() {
        let mut report = report(CloudProvider::Onedrive);
        report.notices.push("source-scan-incomplete".into());
        let runtime = assess_provider_client_runtime(
            CloudProvider::Onedrive,
            Some(b"OneDrive Sync Service\n"),
            25,
        );
        assert_eq!(
            export_naruon_cloud_copy_readiness(&report, &runtime, None).unwrap_err(),
            "naruon-copy-readiness-source-scan-incomplete"
        );
    }

    #[test]
    fn export_is_path_free_and_preserves_metadata_precedence_aggregates() {
        let onedrive_report = report(CloudProvider::Onedrive);
        let runtime = assess_provider_client_runtime(
            CloudProvider::Onedrive,
            Some(b"OneDrive Sync Service\n"),
            25,
        );
        let envelope =
            export_naruon_cloud_copy_readiness(&onedrive_report, &runtime, None).unwrap();

        assert_eq!(envelope.candidate_count, 2);
        assert_eq!(
            envelope.source_selection_policy,
            CloudPlanOptions {
                min_size_bytes: 90 * 1024 * 1024,
                min_age_days: 30,
                limit: 200,
            }
        );
        assert_eq!(
            envelope.production_time_evidence.embedded_metadata,
            CountBytes {
                count: 1,
                bytes: 42
            }
        );
        assert_eq!(
            envelope.production_time_evidence.filesystem_created.count,
            1
        );
        assert_eq!(
            envelope
                .production_time_evidence
                .explicit_filename_date
                .count,
            0
        );
        assert_eq!(envelope.readiness_state, CloudCopyReadinessState::Blocked);
        assert!(envelope
            .candidate_blocker_counts
            .contains_key("capacity-unavailable"));
        assert_eq!(envelope.readiness_fingerprint_sha256.len(), 64);
        assert!(validate_naruon_cloud_copy_readiness(&envelope).is_ok());

        let encoded = serde_json::to_string(&envelope).unwrap();
        for redacted in [
            "/private/",
            "report.pdf",
            "Private title",
            "private@example.com",
            "private raw value",
        ] {
            assert!(!encoded.contains(redacted));
        }
    }

    #[test]
    fn third_party_global_sync_is_bound_and_fails_closed() {
        let report = report(CloudProvider::Onedrive);
        let runtime = assess_provider_client_runtime(
            CloudProvider::Onedrive,
            Some(b"OneDrive Sync Service\n"),
            25,
        );
        let expected = CountBytes {
            count: report.candidates.len() as u64,
            bytes: report.candidate_bytes,
        };

        let missing = export_naruon_cloud_copy_readiness(&report, &runtime, None).unwrap();
        assert!(missing.provider_global_sync.is_none());
        assert_eq!(
            missing
                .candidate_blocker_counts
                .get("provider-global-sync-evidence-unavailable"),
            Some(&expected)
        );

        let blocked_sync = ProviderGlobalSyncReport {
            schema_version: provider_global_sync::PROVIDER_GLOBAL_SYNC_SCHEMA_VERSION,
            provider: CloudProvider::Onedrive,
            evidence_kind: "fileproviderctl-global-dump".into(),
            observed_at_ms: 1,
            admission_blocked_since_ms: None,
            evidence_complete: true,
            state: ProviderGlobalSyncState::Pending,
            upload_progress_present: true,
            download_progress_present: false,
            pending_indexable_count: Some(2),
            blockers: vec!["provider-global-sync-transfer-active".into()],
            notices: vec!["provider-global-sync-dump-read-only".into()],
        };
        let blocked = export_naruon_cloud_copy_readiness_with_global_sync(
            &report,
            &runtime,
            None,
            Some(&blocked_sync),
        )
        .unwrap();
        assert_eq!(blocked.provider_global_sync, Some(blocked_sync.clone()));
        assert_eq!(
            blocked
                .candidate_blocker_counts
                .get("provider-global-sync-transfer-active"),
            Some(&expected)
        );
        assert!(validate_naruon_cloud_copy_readiness(&blocked).is_ok());

        let clear_sync = ProviderGlobalSyncReport {
            state: ProviderGlobalSyncState::Clear,
            upload_progress_present: false,
            download_progress_present: false,
            pending_indexable_count: Some(0),
            blockers: Vec::new(),
            ..blocked_sync
        };
        let clear = export_naruon_cloud_copy_readiness_with_global_sync(
            &report,
            &runtime,
            None,
            Some(&clear_sync),
        )
        .unwrap();
        assert!(clear.provider_global_sync.is_some());
        assert!(!clear
            .candidate_blocker_counts
            .keys()
            .any(|key| key.starts_with("provider-global-sync-")));

        let mut wrong_provider = clear_sync;
        wrong_provider.provider = CloudProvider::GoogleDrive;
        assert_eq!(
            export_naruon_cloud_copy_readiness_with_global_sync(
                &report,
                &runtime,
                None,
                Some(&wrong_provider),
            )
            .unwrap_err(),
            "naruon-copy-readiness-provider-global-sync-invalid"
        );
    }

    #[test]
    fn modified_fallback_maps_to_filesystem_modified() {
        let mut report = report(CloudProvider::Onedrive);
        report.candidates[1].production_time_source = "filesystem:modified-fallback".into();
        report.candidates[1].review_fingerprint =
            crate::cloud::candidate_review_fingerprint(&report.candidates[1]);
        let runtime = assess_provider_client_runtime(
            CloudProvider::Onedrive,
            Some(b"OneDrive Sync Service\n"),
            25,
        );

        let envelope = export_naruon_cloud_copy_readiness(&report, &runtime, None).unwrap();

        assert_eq!(
            envelope.production_time_evidence.filesystem_modified,
            CountBytes {
                count: 1,
                bytes: 42
            }
        );
        assert_eq!(envelope.production_time_evidence.unclassified.count, 0);
    }

    #[test]
    fn icloud_queue_and_missing_evidence_both_fail_closed() {
        let report = report(CloudProvider::Icloud);
        let runtime = assess_provider_client_runtime(CloudProvider::Icloud, None, 25);
        let mut health = icloud_health(true);
        health.native_status = Some(native_sync_up_status());
        health
            .new_copy_admission_blockers
            .push("icloud-native-status-command-timeout".into());
        health
            .blockers
            .insert(0, "icloud-native-status-command-timeout".into());
        health
            .new_copy_admission_blockers
            .push("icloud-native-sync-up-pending".into());
        health
            .blockers
            .insert(0, "icloud-native-sync-up-pending".into());
        let blocked = export_naruon_cloud_copy_readiness(&report, &runtime, Some(&health)).unwrap();
        assert_eq!(blocked.icloud_new_copy_admission_met, Some(false));
        let admission = blocked.icloud_new_copy_admission.as_ref().unwrap();
        assert_eq!(admission.item_error_octagon_not_signed_in_count, 1);
        assert_eq!(admission.item_error_unclassified_count, 0);
        assert_eq!(admission.newest_item_error_timestamp_ms, Some(10));
        assert_eq!(admission.newest_item_error_age_ms, Some(20));
        assert_eq!(
            admission
                .native_status
                .as_ref()
                .and_then(|status| status.sync_state.as_deref()),
            Some("needs-sync-up")
        );
        assert!(blocked
            .candidate_blocker_counts
            .contains_key("icloud-upload-queue-nonempty"));

        let missing = export_naruon_cloud_copy_readiness(&report, &runtime, None).unwrap();
        assert_eq!(missing.icloud_new_copy_admission_met, Some(false));
        assert!(missing
            .candidate_blocker_counts
            .contains_key("icloud-new-copy-admission-evidence-unavailable"));

        let clear_health = icloud_health(false);
        let clear =
            export_naruon_cloud_copy_readiness(&report, &runtime, Some(&clear_health)).unwrap();
        assert_eq!(clear.icloud_new_copy_admission_met, Some(true));

        let mut incomplete_health = clear_health;
        incomplete_health.evidence_complete = false;
        incomplete_health.database_snapshot_includes_wal = false;
        incomplete_health.new_copy_admission_state = "blocked".into();
        incomplete_health.new_copy_admission_blockers =
            vec!["icloud-sync-health-evidence-incomplete".into()];
        let incomplete =
            export_naruon_cloud_copy_readiness(&report, &runtime, Some(&incomplete_health))
                .unwrap();
        assert_eq!(incomplete.icloud_new_copy_admission_met, Some(false));
        assert!(incomplete
            .candidate_blocker_counts
            .contains_key("icloud-sync-health-evidence-incomplete"));
    }

    #[test]
    fn native_sync_up_blocks_even_when_private_queue_is_quiet() {
        let report = report(CloudProvider::Icloud);
        let runtime = assess_provider_client_runtime(CloudProvider::Icloud, None, 25);
        let mut health = icloud_health(false);
        let mut native = native_sync_up_status();
        native.pending_scan_count = 1;
        health.native_status = Some(native);
        health.new_copy_admission_state = "blocked".into();
        health.new_copy_admission_blockers = vec![
            "icloud-native-status-command-timeout".into(),
            "icloud-native-sync-up-pending".into(),
            "icloud-native-status-pending-scan".into(),
        ];
        health.blockers = vec![
            "icloud-native-status-command-timeout".into(),
            "icloud-native-sync-up-pending".into(),
            "icloud-native-status-pending-scan".into(),
        ];

        let envelope =
            export_naruon_cloud_copy_readiness(&report, &runtime, Some(&health)).unwrap();
        let admission = envelope.icloud_new_copy_admission.as_ref().unwrap();
        assert_eq!(envelope.icloud_new_copy_admission_met, Some(false));
        assert_eq!(
            admission.blockers,
            vec![
                "icloud-native-status-command-timeout",
                "icloud-native-sync-up-pending",
                "icloud-native-status-pending-scan"
            ]
        );
        assert_eq!(
            admission
                .native_status
                .as_ref()
                .and_then(|status| status.sync_state.as_deref()),
            Some("needs-sync-up")
        );
        assert!(validate_naruon_cloud_copy_readiness(&envelope).is_ok());
    }

    #[test]
    fn missing_pre_copy_cohort_blocks_naruon_readiness() {
        let mut report = report(CloudProvider::Icloud);
        report.pre_copy_evidence = None;
        let runtime = assess_provider_client_runtime(CloudProvider::Icloud, None, 25);
        let envelope =
            export_naruon_cloud_copy_readiness(&report, &runtime, Some(&icloud_health(false)))
                .unwrap();
        assert_eq!(envelope.pre_copy_evidence_met, Some(false));
        assert!(envelope
            .candidate_blocker_counts
            .contains_key(PRE_COPY_EVIDENCE_BLOCKER));
        assert_eq!(envelope.readiness_state, CloudCopyReadinessState::Blocked);
        assert!(validate_naruon_cloud_copy_readiness(&envelope).is_ok());
    }

    #[test]
    fn native_sync_down_blocks_new_copy_admission() {
        let report = report(CloudProvider::Icloud);
        let runtime = assess_provider_client_runtime(CloudProvider::Icloud, None, 25);
        let mut health = icloud_health(false);
        let mut native = native_sync_up_status();
        native.sync_state = Some("needs-sync-down".into());
        health.native_status = Some(native);
        health.new_copy_admission_state = "blocked".into();
        health.new_copy_admission_blockers = vec![
            "icloud-native-status-command-timeout".into(),
            "icloud-native-sync-down-pending".into(),
        ];
        health.blockers = vec![
            "icloud-native-status-command-timeout".into(),
            "icloud-native-sync-down-pending".into(),
        ];

        let envelope =
            export_naruon_cloud_copy_readiness(&report, &runtime, Some(&health)).unwrap();
        let admission = envelope.icloud_new_copy_admission.as_ref().unwrap();
        assert_eq!(envelope.icloud_new_copy_admission_met, Some(false));
        assert_eq!(
            admission.blockers,
            vec![
                "icloud-native-status-command-timeout",
                "icloud-native-sync-down-pending"
            ]
        );
        assert!(validate_naruon_cloud_copy_readiness(&envelope).is_ok());
    }

    #[test]
    fn invalid_native_status_uses_copy_readiness_error_namespace() {
        let report = report(CloudProvider::Icloud);
        let runtime = assess_provider_client_runtime(CloudProvider::Icloud, None, 25);
        let mut health = icloud_health(false);
        let mut native = native_sync_up_status();
        native.notices.clear();
        health.native_status = Some(native);

        assert_eq!(
            export_naruon_cloud_copy_readiness(&report, &runtime, Some(&health)).unwrap_err(),
            "naruon-copy-readiness-icloud-native-status-invalid"
        );
    }

    #[test]
    fn incomplete_icloud_snapshot_never_authorizes_new_copy() {
        let report = report(CloudProvider::Icloud);
        let runtime = assess_provider_client_runtime(CloudProvider::Icloud, None, 25);
        let mut health = icloud_health(false);
        health.evidence_complete = false;
        health.database_snapshot_includes_wal = false;
        health.new_copy_admission_state = "blocked".into();
        health.new_copy_admission_blockers = vec!["icloud-sync-health-evidence-incomplete".into()];

        let envelope =
            export_naruon_cloud_copy_readiness(&report, &runtime, Some(&health)).unwrap();
        let admission = envelope.icloud_new_copy_admission.as_ref().unwrap();

        assert_eq!(envelope.icloud_new_copy_admission_met, Some(false));
        assert_eq!(admission.state, "blocked");
        assert_eq!(
            admission.blockers,
            vec![
                "icloud-sync-health-evidence-incomplete",
                "icloud-new-copy-admission-evidence-unavailable",
            ]
        );
        assert!(!admission.evidence_complete);
        assert!(!admission.database_snapshot_includes_wal);
        assert!(envelope
            .candidate_blocker_counts
            .contains_key("icloud-new-copy-admission-evidence-unavailable"));
    }

    #[test]
    fn complete_consistent_icloud_snapshot_can_clear_with_or_without_a_wal_file() {
        let report = report(CloudProvider::Icloud);
        let runtime = assess_provider_client_runtime(CloudProvider::Icloud, None, 25);
        for database_snapshot_includes_wal in [true, false] {
            let mut health = icloud_health(false);
            health.evidence_complete = true;
            health.database_snapshot_includes_wal = database_snapshot_includes_wal;

            let envelope =
                export_naruon_cloud_copy_readiness(&report, &runtime, Some(&health)).unwrap();
            let admission = envelope.icloud_new_copy_admission.as_ref().unwrap();

            assert_eq!(envelope.icloud_new_copy_admission_met, Some(true));
            assert_eq!(admission.state, "clear");
            assert!(admission.blockers.is_empty());
            assert!(admission.evidence_complete);
            assert_eq!(
                admission.database_snapshot_includes_wal,
                database_snapshot_includes_wal
            );
            assert!(!envelope
                .candidate_blocker_counts
                .contains_key("icloud-new-copy-admission-evidence-unavailable"));
        }
    }

    #[test]
    fn export_rejects_provider_switches_and_fingerprint_mutation() {
        let onedrive_report = report(CloudProvider::Onedrive);
        let wrong_runtime =
            assess_provider_client_runtime(CloudProvider::GoogleDrive, Some(b"Google Drive\n"), 25);
        assert_eq!(
            export_naruon_cloud_copy_readiness(&onedrive_report, &wrong_runtime, None,)
                .unwrap_err(),
            "naruon-copy-readiness-runtime-provider-mismatch"
        );

        let runtime =
            assess_provider_client_runtime(CloudProvider::Onedrive, Some(b"OneDrive\n"), 25);
        let mut envelope =
            export_naruon_cloud_copy_readiness(&onedrive_report, &runtime, None).unwrap();
        envelope.candidate_bytes += 1;
        assert_eq!(
            validate_naruon_cloud_copy_readiness(&envelope).unwrap_err(),
            "naruon-copy-readiness-aggregate-invalid"
        );

        let mut forged =
            export_naruon_cloud_copy_readiness(&onedrive_report, &runtime, None).unwrap();
        forged.remote_capacity_verified = true;
        resign(&mut forged);
        assert_eq!(
            validate_naruon_cloud_copy_readiness(&forged).unwrap_err(),
            "naruon-copy-readiness-provider-binding-invalid"
        );

        let mut forged_planner =
            export_naruon_cloud_copy_readiness(&onedrive_report, &runtime, None).unwrap();
        forged_planner.candidate_blocker_counts.insert(
            "planner-blocked".into(),
            CountBytes {
                count: 1,
                bytes: 42,
            },
        );
        resign(&mut forged_planner);
        assert_eq!(
            validate_naruon_cloud_copy_readiness(&forged_planner).unwrap_err(),
            "naruon-copy-readiness-planner-binding-invalid"
        );

        let mut forged_runtime_blocker =
            export_naruon_cloud_copy_readiness(&onedrive_report, &runtime, None).unwrap();
        forged_runtime_blocker.candidate_blocker_counts.insert(
            "provider-client-runtime-not-observed".into(),
            CountBytes {
                count: forged_runtime_blocker.candidate_count,
                bytes: forged_runtime_blocker.candidate_bytes,
            },
        );
        resign(&mut forged_runtime_blocker);
        assert_eq!(
            validate_naruon_cloud_copy_readiness(&forged_runtime_blocker).unwrap_err(),
            "naruon-copy-readiness-runtime-binding-invalid"
        );

        let mut forged_icloud_blocker =
            export_naruon_cloud_copy_readiness(&onedrive_report, &runtime, None).unwrap();
        forged_icloud_blocker.candidate_blocker_counts.insert(
            "icloud-file-provider-indexing-pending".into(),
            CountBytes {
                count: forged_icloud_blocker.candidate_count,
                bytes: forged_icloud_blocker.candidate_bytes,
            },
        );
        resign(&mut forged_icloud_blocker);
        assert_eq!(
            validate_naruon_cloud_copy_readiness(&forged_icloud_blocker).unwrap_err(),
            "naruon-copy-readiness-icloud-binding-invalid"
        );

        let icloud_report = report(CloudProvider::Icloud);
        let icloud_runtime = assess_provider_client_runtime(CloudProvider::Icloud, None, 25);
        let health = icloud_health(true);
        let mut forged_icloud =
            export_naruon_cloud_copy_readiness(&icloud_report, &icloud_runtime, Some(&health))
                .unwrap();
        forged_icloud
            .icloud_new_copy_admission
            .as_mut()
            .unwrap()
            .scheduled_waiting_count += 1;
        resign(&mut forged_icloud);
        assert_eq!(
            validate_naruon_cloud_copy_readiness(&forged_icloud).unwrap_err(),
            "naruon-copy-readiness-icloud-shape-invalid"
        );

        let mut forged_icloud_authority =
            export_naruon_cloud_copy_readiness(&icloud_report, &icloud_runtime, Some(&health))
                .unwrap();
        forged_icloud_authority
            .icloud_new_copy_admission
            .as_mut()
            .unwrap()
            .evidence_complete = false;
        resign(&mut forged_icloud_authority);
        assert_eq!(
            validate_naruon_cloud_copy_readiness(&forged_icloud_authority).unwrap_err(),
            "naruon-copy-readiness-icloud-shape-invalid"
        );
    }

    #[test]
    fn offline_reader_accepts_valid_envelope_and_rejects_untrusted_inputs() {
        let report = report(CloudProvider::Onedrive);
        let runtime = assess_provider_client_runtime(
            CloudProvider::Onedrive,
            Some(b"OneDrive Sync Service\n"),
            25,
        );
        let envelope = export_naruon_cloud_copy_readiness(&report, &runtime, None).unwrap();
        let directory = tempfile::tempdir().unwrap();
        let valid_path = directory.path().join("valid.json");
        std::fs::write(&valid_path, serde_json::to_vec(&envelope).unwrap()).unwrap();

        assert_eq!(
            read_and_validate_naruon_cloud_copy_readiness(&valid_path).unwrap(),
            envelope
        );
        assert_eq!(
            read_and_validate_naruon_cloud_copy_readiness(Path::new("relative.json")).unwrap_err(),
            "naruon-copy-readiness-input-path-not-absolute"
        );

        let mut unknown = serde_json::to_value(&envelope).unwrap();
        unknown["provider_runtime"]["unexpected"] = serde_json::json!(true);
        let unknown_path = directory.path().join("unknown.json");
        std::fs::write(&unknown_path, serde_json::to_vec(&unknown).unwrap()).unwrap();
        assert_eq!(
            read_and_validate_naruon_cloud_copy_readiness(&unknown_path).unwrap_err(),
            "naruon-copy-readiness-json-invalid"
        );

        let mut forged = envelope.clone();
        forged.readiness_fingerprint_sha256 = "0".repeat(64);
        let forged_path = directory.path().join("forged.json");
        std::fs::write(&forged_path, serde_json::to_vec(&forged).unwrap()).unwrap();
        assert_eq!(
            read_and_validate_naruon_cloud_copy_readiness(&forged_path).unwrap_err(),
            "naruon-copy-readiness-fingerprint-invalid"
        );

        let oversized_path = directory.path().join("oversized.json");
        std::fs::write(
            &oversized_path,
            vec![b' '; (NARUON_CLOUD_COPY_READINESS_MAX_INPUT_BYTES + 1) as usize],
        )
        .unwrap();
        assert_eq!(
            read_and_validate_naruon_cloud_copy_readiness(&oversized_path).unwrap_err(),
            "naruon-copy-readiness-input-size-invalid"
        );

        #[cfg(unix)]
        {
            let symlink_path = directory.path().join("symlink.json");
            std::os::unix::fs::symlink(&valid_path, &symlink_path).unwrap();
            assert_eq!(
                read_and_validate_naruon_cloud_copy_readiness(&symlink_path).unwrap_err(),
                "naruon-copy-readiness-input-not-regular-file"
            );
        }
    }

    #[test]
    fn fingerprint_uses_recursive_lexicographic_json_keys() {
        let report = report(CloudProvider::Onedrive);
        let runtime = assess_provider_client_runtime(
            CloudProvider::Onedrive,
            Some(b"OneDrive Sync Service\n"),
            25,
        );
        let envelope = export_naruon_cloud_copy_readiness(&report, &runtime, None).unwrap();
        let mut unsigned = envelope.clone();
        unsigned.readiness_fingerprint_sha256.clear();
        let mut canonical = Vec::new();
        append_canonical_json(&serde_json::to_value(&unsigned).unwrap(), &mut canonical).unwrap();
        let encoded = String::from_utf8(canonical).unwrap();

        assert!(encoded.starts_with("{\"account_identifiers_included\":"));
        assert!(
            encoded.find("\"capacity\":").unwrap() < encoded.find("\"provider_runtime\":").unwrap()
        );
        let expected = Sha256::digest(encoded.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(envelope.readiness_fingerprint_sha256, expected);
        assert_eq!(
            envelope.readiness_fingerprint_sha256,
            "6a69022601c4fc41c9e42360618e7e31728d8b9cf7757f15791832974f8e67bd"
        );
    }
}
