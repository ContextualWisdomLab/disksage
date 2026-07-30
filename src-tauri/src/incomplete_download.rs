#[cfg(not(coverage))]
use crate::cloud::probe_content_metadata_for_audit;
use crate::cloud::ContentMetadata;
use crate::cloud_local_eviction::{observe_path_active_use, ActiveUseEvidence};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use unicode_normalization::UnicodeNormalization;

pub const INCOMPLETE_DOWNLOAD_AUDIT_VERSION: u32 = 1;
pub const DEFAULT_MAX_ENTRIES: usize = 200_000;
pub const DEFAULT_STALE_AFTER_DAYS: u64 = 30;
pub const MAX_STALE_AFTER_DAYS: u64 = 3_650;
const MAX_DEPTH: usize = 64;
const TYPE_PROBE_BYTES: usize = 16 * 1024;
const MAX_RECORDED_ISSUE_KINDS: usize = 32;
const DAY_MS: u64 = 86_400_000;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "kebab-case")]
pub enum IncompleteDownloadState {
    Active,
    ActiveUseEvidenceIncomplete,
    RecentIdle,
    StaleIdleRecoveryCandidate,
    StaleIdleReview,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncompleteDownloadAuditItem {
    pub candidate_fingerprint: String,
    pub relative_path: String,
    pub logical_bytes: u64,
    pub allocated_bytes: u64,
    pub filesystem_created_ms: u64,
    pub filesystem_modified_ms: u64,
    pub modified_age_days: u64,
    pub staleness_basis: String,
    pub state: IncompleteDownloadState,
    pub active_use: ActiveUseEvidence,
    pub evidence_complete: bool,
    pub evidence_issues: Vec<String>,
    pub detected_mime_type: Option<String>,
    pub detected_extension: Option<String>,
    pub structural_zip_candidate_count: u64,
    pub structural_zip_candidates: Vec<String>,
    pub structural_zip_recoverable_bytes: u64,
    pub whole_file_structurally_complete_zip: bool,
    pub zip_eocd_count: u64,
    pub zip_eocd_offsets: Vec<String>,
    pub download_acquired_dates: Vec<String>,
    pub download_agents: Vec<String>,
    pub download_origin_hosts: Vec<String>,
    pub production_time_evidence_present: bool,
    pub final_sibling_relative_path: Option<String>,
    pub final_sibling_exists: bool,
    pub final_sibling_bytes: Option<u64>,
    pub recovery_candidate: bool,
    pub partial_content_recovery_possible: bool,
    pub requires_human_review: bool,
    pub automatic_discard_allowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncompleteDownloadAuditReport {
    pub schema_version: u32,
    pub observed_at_ms: u64,
    pub source_root: String,
    pub source_scope_fingerprint: String,
    pub stale_after_days: u64,
    pub evidence_complete: bool,
    pub entries_seen: usize,
    pub issue_counts: BTreeMap<String, u64>,
    pub file_count: usize,
    pub logical_bytes: u64,
    pub allocated_bytes: u64,
    pub active_count: usize,
    pub active_bytes: u64,
    pub evidence_incomplete_count: usize,
    pub evidence_incomplete_bytes: u64,
    pub recent_idle_count: usize,
    pub recent_idle_bytes: u64,
    pub stale_idle_count: usize,
    pub stale_idle_bytes: u64,
    pub recovery_candidate_count: usize,
    pub recovery_candidate_bytes: u64,
    pub structural_zip_candidate_item_count: usize,
    pub structural_zip_recoverable_bytes: u64,
    pub whole_file_structurally_complete_zip_count: usize,
    pub whole_file_structurally_complete_zip_bytes: u64,
    pub detected_type_count: usize,
    pub acquisition_date_evidence_count: usize,
    pub production_time_evidence_count: usize,
    pub final_sibling_count: usize,
    pub discard_review_bytes: u64,
    pub audit_fingerprint: String,
    pub mutation_performed: bool,
    pub items: Vec<IncompleteDownloadAuditItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncompleteDownloadItemSummary {
    pub candidate_fingerprint: String,
    pub logical_bytes: u64,
    pub allocated_bytes: u64,
    pub modified_age_days: u64,
    pub staleness_basis: String,
    pub state: IncompleteDownloadState,
    pub active_use_evidence_complete: bool,
    pub active: bool,
    pub evidence_complete: bool,
    pub evidence_issue_count: usize,
    pub detected_mime_type: Option<String>,
    pub detected_extension: Option<String>,
    pub structural_zip_candidate_count: u64,
    pub structural_zip_recoverable_bytes: u64,
    pub whole_file_structurally_complete_zip: bool,
    pub zip_eocd_count: u64,
    pub download_acquisition_date_evidence_present: bool,
    pub production_time_evidence_present: bool,
    pub final_sibling_exists: bool,
    pub recovery_candidate: bool,
    pub partial_content_recovery_possible: bool,
    pub requires_human_review: bool,
    pub automatic_discard_allowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncompleteDownloadAuditSummary {
    pub schema_version: u32,
    pub output_mode: String,
    pub observed_at_ms: u64,
    pub source_scope_fingerprint: String,
    pub stale_after_days: u64,
    pub evidence_complete: bool,
    pub entries_seen: usize,
    pub issue_counts: BTreeMap<String, u64>,
    pub file_count: usize,
    pub logical_bytes: u64,
    pub allocated_bytes: u64,
    pub active_count: usize,
    pub active_bytes: u64,
    pub evidence_incomplete_count: usize,
    pub evidence_incomplete_bytes: u64,
    pub recent_idle_count: usize,
    pub recent_idle_bytes: u64,
    pub stale_idle_count: usize,
    pub stale_idle_bytes: u64,
    pub recovery_candidate_count: usize,
    pub recovery_candidate_bytes: u64,
    pub structural_zip_candidate_item_count: usize,
    pub structural_zip_recoverable_bytes: u64,
    pub whole_file_structurally_complete_zip_count: usize,
    pub whole_file_structurally_complete_zip_bytes: u64,
    pub detected_type_count: usize,
    pub acquisition_date_evidence_count: usize,
    pub production_time_evidence_count: usize,
    pub final_sibling_count: usize,
    pub discard_review_bytes: u64,
    pub audit_fingerprint: String,
    pub mutation_performed: bool,
    pub human_discard_approval_required: bool,
    pub automatic_discard_allowed: bool,
    pub notices: Vec<String>,
    pub redacted_from_summary: Vec<String>,
    pub items: Vec<IncompleteDownloadItemSummary>,
}

fn normalized(value: &str) -> String {
    value.nfc().collect()
}

fn valid_relative_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
}

fn incomplete_download_name(path: &Path) -> bool {
    path.file_name()
        .map(|name| {
            name.to_string_lossy()
                .to_ascii_lowercase()
                .ends_with(".crdownload")
        })
        .unwrap_or(false)
}

fn final_sibling_path(path: &Path) -> Option<PathBuf> {
    let name = path.file_name()?.to_string_lossy();
    let suffix = ".crdownload";
    if !name.to_ascii_lowercase().ends_with(suffix) || name.len() <= suffix.len() {
        return None;
    }
    Some(path.with_file_name(&name[..name.len() - suffix.len()]))
}

fn source_scope_fingerprint(source_root: &str, stale_after_days: u64) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-incomplete-download-source-scope-v1\0");
    hasher.update(normalized(source_root).as_bytes());
    hasher.update(&[0]);
    hasher.update(&stale_after_days.to_le_bytes());
    hasher.finalize().to_hex().to_string()
}

fn metadata_values(metadata: &ContentMetadata, field: &str) -> Vec<String> {
    let mut values = metadata
        .evidence
        .iter()
        .filter(|evidence| evidence.field == field)
        .map(|evidence| normalized(&evidence.value))
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values
}

fn metadata_u64(metadata: &ContentMetadata, field: &str) -> u64 {
    metadata_values(metadata, field)
        .first()
        .and_then(|value| value.parse().ok())
        .unwrap_or_default()
}

fn metadata_probe_issues(metadata: &ContentMetadata) -> Vec<String> {
    metadata_values(metadata, "metadata-probe-warning")
}

fn structural_zip_range(value: &str) -> Option<(u64, u64)> {
    let mut start = None;
    let mut end = None;
    for field in value.split(';') {
        let (key, value) = field.split_once('=')?;
        match key {
            "start" => start = value.parse().ok(),
            "end" => end = value.parse().ok(),
            _ => {}
        }
    }
    let (start, end) = (start?, end?);
    (end > start).then_some((start, end))
}

fn structural_zip_range_summary(values: &[String], logical_bytes: u64) -> (u64, bool) {
    let mut ranges = values
        .iter()
        .filter_map(|value| structural_zip_range(value))
        .filter(|(_, end)| *end <= logical_bytes)
        .collect::<Vec<_>>();
    ranges.sort_unstable();
    let whole_file = ranges
        .iter()
        .any(|(start, end)| *start == 0 && *end == logical_bytes);
    let mut merged = Vec::<(u64, u64)>::new();
    for (start, end) in ranges {
        if let Some((_, previous_end)) = merged.last_mut() {
            if start <= *previous_end {
                *previous_end = (*previous_end).max(end);
                continue;
            }
        }
        merged.push((start, end));
    }
    let recoverable_bytes = merged.iter().fold(0u64, |total, (start, end)| {
        total.saturating_add(end.saturating_sub(*start))
    });
    (recoverable_bytes, whole_file)
}

fn detect_content_type_from_bytes(bytes: &[u8]) -> (Option<String>, Option<String>) {
    infer::get(bytes)
        .map(|kind| {
            (
                Some(kind.mime_type().to_string()),
                Some(kind.extension().to_string()),
            )
        })
        .unwrap_or((None, None))
}

#[cfg(not(coverage))]
fn detect_content_type(path: &Path) -> Result<(Option<String>, Option<String>), String> {
    let mut file =
        std::fs::File::open(path).map_err(|_| "magic-type-probe-open-failed".to_string())?;
    let mut bytes = vec![0u8; TYPE_PROBE_BYTES];
    let read = file
        .read(&mut bytes)
        .map_err(|_| "magic-type-probe-read-failed".to_string())?;
    bytes.truncate(read);
    Ok(detect_content_type_from_bytes(&bytes))
}

#[cfg(unix)]
fn allocated_bytes(metadata: &std::fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    metadata.blocks().saturating_mul(512)
}

#[cfg(not(unix))]
fn allocated_bytes(_metadata: &std::fs::Metadata) -> u64 {
    0
}

fn system_time_ms(value: std::io::Result<std::time::SystemTime>) -> u64 {
    value
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .unwrap_or_default()
}

fn classify_state(
    active_use: &ActiveUseEvidence,
    modified_age_days: u64,
    stale_after_days: u64,
    recovery_candidate: bool,
) -> IncompleteDownloadState {
    if !active_use.evidence_complete {
        IncompleteDownloadState::ActiveUseEvidenceIncomplete
    } else if active_use.active {
        IncompleteDownloadState::Active
    } else if modified_age_days < stale_after_days {
        IncompleteDownloadState::RecentIdle
    } else if recovery_candidate {
        IncompleteDownloadState::StaleIdleRecoveryCandidate
    } else {
        IncompleteDownloadState::StaleIdleReview
    }
}

#[allow(clippy::too_many_arguments)]
fn candidate_fingerprint(
    source_root: &str,
    relative_path: &str,
    logical_bytes: u64,
    allocated_bytes: u64,
    filesystem_created_ms: u64,
    filesystem_modified_ms: u64,
    state: IncompleteDownloadState,
    active_use: &ActiveUseEvidence,
    evidence_issues: &[String],
    detected_mime_type: Option<&str>,
    detected_extension: Option<&str>,
    structural_zip_candidate_count: u64,
    structural_zip_candidates: &[String],
    zip_eocd_count: u64,
    zip_eocd_offsets: &[String],
    download_acquired_dates: &[String],
    download_agents: &[String],
    download_origin_hosts: &[String],
    production_time_evidence_present: bool,
    final_sibling_relative_path: Option<&str>,
    final_sibling_bytes: Option<u64>,
) -> String {
    let mut hasher = blake3::Hasher::new();
    // Bind stable source evidence and the policy-relevant state transition, not the derived
    // whole-day age. `filesystem_modified_ms` already binds source drift, while `state` changes
    // when the configured staleness threshold is crossed. Hashing `modified_age_days` would make
    // an otherwise identical approved plan expire every day after it was already stale.
    hasher.update(b"disksage-incomplete-download-candidate-v2\0");
    for value in [
        source_root,
        relative_path,
        active_use.method.as_str(),
        detected_mime_type.unwrap_or(""),
        detected_extension.unwrap_or(""),
        final_sibling_relative_path.unwrap_or(""),
    ] {
        hasher.update(normalized(value).as_bytes());
        hasher.update(&[0]);
    }
    for value in [
        logical_bytes,
        allocated_bytes,
        filesystem_created_ms,
        filesystem_modified_ms,
        structural_zip_candidate_count,
        zip_eocd_count,
        final_sibling_bytes.unwrap_or_default(),
    ] {
        hasher.update(&value.to_le_bytes());
    }
    hasher.update(&[state as u8]);
    for value in [
        active_use.evidence_complete,
        active_use.active,
        active_use.results_truncated,
        production_time_evidence_present,
        final_sibling_bytes.is_some(),
    ] {
        hasher.update(&[u8::from(value)]);
    }
    for pid in &active_use.observed_pids {
        hasher.update(&pid.to_le_bytes());
    }
    for values in [
        evidence_issues,
        structural_zip_candidates,
        zip_eocd_offsets,
        download_acquired_dates,
        download_agents,
        download_origin_hosts,
    ] {
        for value in values {
            hasher.update(normalized(value).as_bytes());
            hasher.update(&[0]);
        }
        hasher.update(&[0xff]);
    }
    hasher.finalize().to_hex().to_string()
}

#[allow(clippy::too_many_arguments)]
fn build_item(
    source_root: &str,
    relative_path: String,
    logical_bytes: u64,
    allocated_bytes: u64,
    filesystem_created_ms: u64,
    filesystem_modified_ms: u64,
    observed_at_ms: u64,
    stale_after_days: u64,
    active_use: ActiveUseEvidence,
    mut evidence_issues: Vec<String>,
    detected_mime_type: Option<String>,
    detected_extension: Option<String>,
    structural_zip_candidates: Vec<String>,
    zip_eocd_count: u64,
    zip_eocd_offsets: Vec<String>,
    download_acquired_dates: Vec<String>,
    download_agents: Vec<String>,
    download_origin_hosts: Vec<String>,
    production_time_evidence_present: bool,
    final_sibling_relative_path: Option<String>,
    final_sibling_bytes: Option<u64>,
) -> IncompleteDownloadAuditItem {
    evidence_issues.sort();
    evidence_issues.dedup();
    let structural_zip_candidate_count = structural_zip_candidates.len() as u64;
    let (structural_zip_recoverable_bytes, whole_file_structurally_complete_zip) =
        structural_zip_range_summary(&structural_zip_candidates, logical_bytes);
    let modified_age_days = observed_at_ms
        .saturating_sub(filesystem_modified_ms)
        .checked_div(DAY_MS)
        .unwrap_or_default();
    let final_sibling_exists = final_sibling_bytes.is_some();
    let recovery_candidate =
        detected_mime_type.is_some() || structural_zip_candidate_count > 0 || final_sibling_exists;
    let state = classify_state(
        &active_use,
        modified_age_days,
        stale_after_days,
        recovery_candidate,
    );
    let evidence_complete = active_use.evidence_complete && evidence_issues.is_empty();
    let fingerprint = candidate_fingerprint(
        source_root,
        &relative_path,
        logical_bytes,
        allocated_bytes,
        filesystem_created_ms,
        filesystem_modified_ms,
        state,
        &active_use,
        &evidence_issues,
        detected_mime_type.as_deref(),
        detected_extension.as_deref(),
        structural_zip_candidate_count,
        &structural_zip_candidates,
        zip_eocd_count,
        &zip_eocd_offsets,
        &download_acquired_dates,
        &download_agents,
        &download_origin_hosts,
        production_time_evidence_present,
        final_sibling_relative_path.as_deref(),
        final_sibling_bytes,
    );
    IncompleteDownloadAuditItem {
        candidate_fingerprint: fingerprint,
        relative_path,
        logical_bytes,
        allocated_bytes,
        filesystem_created_ms,
        filesystem_modified_ms,
        modified_age_days,
        staleness_basis: "filesystem-modified-age-not-production-date".into(),
        state,
        active_use,
        evidence_complete,
        evidence_issues,
        detected_mime_type,
        detected_extension,
        structural_zip_candidate_count,
        structural_zip_candidates,
        structural_zip_recoverable_bytes,
        whole_file_structurally_complete_zip,
        zip_eocd_count,
        zip_eocd_offsets,
        download_acquired_dates,
        download_agents,
        download_origin_hosts,
        production_time_evidence_present,
        final_sibling_relative_path,
        final_sibling_exists,
        final_sibling_bytes,
        recovery_candidate,
        partial_content_recovery_possible: logical_bytes > 0,
        requires_human_review: true,
        automatic_discard_allowed: false,
    }
}

#[cfg(not(coverage))]
fn observe_item(
    canonical_root: &Path,
    path: &Path,
    observed_at_ms: u64,
    stale_after_days: u64,
) -> Result<IncompleteDownloadAuditItem, String> {
    let relative = path
        .strip_prefix(canonical_root)
        .map_err(|_| "incomplete-download-relative-path-failed".to_string())?;
    if !valid_relative_path(relative) {
        return Err("incomplete-download-relative-path-unsafe".into());
    }
    let before = std::fs::symlink_metadata(path)
        .map_err(|_| "incomplete-download-metadata-failed".to_string())?;
    if !before.is_file() || before.file_type().is_symlink() {
        return Err("incomplete-download-file-unsafe".into());
    }
    let logical_bytes = before.len();
    let filesystem_created_ms = system_time_ms(before.created());
    let filesystem_modified_ms = system_time_ms(before.modified());
    if filesystem_modified_ms == 0 {
        return Err("incomplete-download-modified-time-unavailable".into());
    }

    let content_metadata = probe_content_metadata_for_audit(path);
    let mut evidence_issues = metadata_probe_issues(&content_metadata);
    let (detected_mime_type, detected_extension) = match detect_content_type(path) {
        Ok(result) => result,
        Err(error) => {
            evidence_issues.push(error);
            (None, None)
        }
    };
    let active_use = observe_path_active_use(path);
    let sibling = final_sibling_path(path);
    let (final_sibling_relative_path, final_sibling_bytes) = match sibling {
        Some(sibling) => match std::fs::symlink_metadata(&sibling) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                let relative = sibling
                    .strip_prefix(canonical_root)
                    .ok()
                    .filter(|relative| valid_relative_path(relative))
                    .map(|relative| normalized(&relative.to_string_lossy()));
                (relative, Some(metadata.len()))
            }
            Ok(_) => {
                evidence_issues.push("final-sibling-unsafe".into());
                (None, None)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (None, None),
            Err(_) => {
                evidence_issues.push("final-sibling-metadata-failed".into());
                (None, None)
            }
        },
        None => {
            evidence_issues.push("final-sibling-name-unavailable".into());
            (None, None)
        }
    };
    let after = std::fs::symlink_metadata(path)
        .map_err(|_| "incomplete-download-post-metadata-failed".to_string())?;
    let after_modified_ms = system_time_ms(after.modified());
    if after.len() != logical_bytes || after_modified_ms != filesystem_modified_ms {
        evidence_issues.push("source-changed-during-audit".into());
    }

    Ok(build_item(
        &canonical_root.to_string_lossy(),
        normalized(&relative.to_string_lossy()),
        logical_bytes,
        allocated_bytes(&before),
        filesystem_created_ms,
        filesystem_modified_ms,
        observed_at_ms,
        stale_after_days,
        active_use,
        evidence_issues,
        detected_mime_type,
        detected_extension,
        metadata_values(
            &content_metadata,
            "incomplete-download-structural-zip-candidate",
        ),
        metadata_u64(
            &content_metadata,
            "incomplete-download-embedded-zip-eocd-count",
        ),
        metadata_values(
            &content_metadata,
            "incomplete-download-embedded-zip-eocd-offset",
        ),
        metadata_values(&content_metadata, "download-acquired-date"),
        metadata_values(&content_metadata, "download-agent"),
        metadata_values(&content_metadata, "download-origin-host"),
        content_metadata.production_time_ms.is_some(),
        final_sibling_relative_path,
        final_sibling_bytes,
    ))
}

fn increment_issue(issues: &mut BTreeMap<String, u64>, reason: &str) {
    if issues.contains_key(reason) || issues.len() < MAX_RECORDED_ISSUE_KINDS {
        *issues.entry(reason.to_string()).or_default() += 1;
    } else {
        *issues
            .entry("additional-issue-kinds-truncated".into())
            .or_default() += 1;
    }
}

fn audit_fingerprint(
    source_root: &str,
    stale_after_days: u64,
    evidence_complete: bool,
    entries_seen: usize,
    issue_counts: &BTreeMap<String, u64>,
    items: &[IncompleteDownloadAuditItem],
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-incomplete-download-audit-v1\0");
    hasher.update(normalized(source_root).as_bytes());
    hasher.update(&[0, u8::from(evidence_complete)]);
    hasher.update(&stale_after_days.to_le_bytes());
    hasher.update(&(entries_seen as u64).to_le_bytes());
    for (reason, count) in issue_counts {
        hasher.update(reason.as_bytes());
        hasher.update(&[0]);
        hasher.update(&count.to_le_bytes());
    }
    for item in items {
        hasher.update(item.candidate_fingerprint.as_bytes());
        hasher.update(&[0]);
    }
    hasher.finalize().to_hex().to_string()
}

pub(crate) fn incomplete_download_audit_integrity_valid(
    report: &IncompleteDownloadAuditReport,
) -> bool {
    if report.schema_version != INCOMPLETE_DOWNLOAD_AUDIT_VERSION
        || report.mutation_performed
        || report.source_scope_fingerprint
            != source_scope_fingerprint(&report.source_root, report.stale_after_days)
        || report.file_count != report.items.len()
        || report.logical_bytes
            != report
                .items
                .iter()
                .fold(0u64, |total, item| total.saturating_add(item.logical_bytes))
        || report.allocated_bytes
            != report.items.iter().fold(0u64, |total, item| {
                total.saturating_add(item.allocated_bytes)
            })
        || report
            .items
            .windows(2)
            .any(|items| items[0].candidate_fingerprint >= items[1].candidate_fingerprint)
        || report.items.iter().any(|item| {
            candidate_fingerprint(
                &report.source_root,
                &item.relative_path,
                item.logical_bytes,
                item.allocated_bytes,
                item.filesystem_created_ms,
                item.filesystem_modified_ms,
                item.state,
                &item.active_use,
                &item.evidence_issues,
                item.detected_mime_type.as_deref(),
                item.detected_extension.as_deref(),
                item.structural_zip_candidate_count,
                &item.structural_zip_candidates,
                item.zip_eocd_count,
                &item.zip_eocd_offsets,
                &item.download_acquired_dates,
                &item.download_agents,
                &item.download_origin_hosts,
                item.production_time_evidence_present,
                item.final_sibling_relative_path.as_deref(),
                item.final_sibling_bytes,
            ) != item.candidate_fingerprint
        })
    {
        return false;
    }

    audit_fingerprint(
        &report.source_root,
        report.stale_after_days,
        report.evidence_complete,
        report.entries_seen,
        &report.issue_counts,
        &report.items,
    ) == report.audit_fingerprint
}

fn build_report(
    source_root: String,
    observed_at_ms: u64,
    stale_after_days: u64,
    entries_seen: usize,
    mut evidence_complete: bool,
    issue_counts: BTreeMap<String, u64>,
    mut items: Vec<IncompleteDownloadAuditItem>,
) -> IncompleteDownloadAuditReport {
    items.sort_by(|left, right| left.candidate_fingerprint.cmp(&right.candidate_fingerprint));
    if items.iter().any(|item| !item.evidence_complete) {
        evidence_complete = false;
    }
    let logical_bytes = items
        .iter()
        .fold(0u64, |total, item| total.saturating_add(item.logical_bytes));
    let allocated_bytes = items.iter().fold(0u64, |total, item| {
        total.saturating_add(item.allocated_bytes)
    });
    let totals = |predicate: fn(&IncompleteDownloadAuditItem) -> bool| {
        let selected = items
            .iter()
            .filter(|item| predicate(item))
            .collect::<Vec<_>>();
        (
            selected.len(),
            selected
                .iter()
                .fold(0u64, |total, item| total.saturating_add(item.logical_bytes)),
        )
    };
    let (active_count, active_bytes) = totals(|item| item.state == IncompleteDownloadState::Active);
    let (evidence_incomplete_count, evidence_incomplete_bytes) =
        totals(|item| item.state == IncompleteDownloadState::ActiveUseEvidenceIncomplete);
    let (recent_idle_count, recent_idle_bytes) =
        totals(|item| item.state == IncompleteDownloadState::RecentIdle);
    let (stale_idle_count, stale_idle_bytes) = totals(|item| {
        matches!(
            item.state,
            IncompleteDownloadState::StaleIdleRecoveryCandidate
                | IncompleteDownloadState::StaleIdleReview
        )
    });
    let (recovery_candidate_count, recovery_candidate_bytes) =
        totals(|item| item.recovery_candidate);
    let (structural_zip_candidate_item_count, _) =
        totals(|item| item.structural_zip_candidate_count > 0);
    let structural_zip_recoverable_bytes = items.iter().fold(0u64, |total, item| {
        total.saturating_add(item.structural_zip_recoverable_bytes)
    });
    let (whole_file_structurally_complete_zip_count, whole_file_structurally_complete_zip_bytes) =
        totals(|item| item.whole_file_structurally_complete_zip);
    let fingerprint = audit_fingerprint(
        &source_root,
        stale_after_days,
        evidence_complete,
        entries_seen,
        &issue_counts,
        &items,
    );

    IncompleteDownloadAuditReport {
        schema_version: INCOMPLETE_DOWNLOAD_AUDIT_VERSION,
        observed_at_ms,
        source_scope_fingerprint: source_scope_fingerprint(&source_root, stale_after_days),
        source_root,
        stale_after_days,
        evidence_complete,
        entries_seen,
        issue_counts,
        file_count: items.len(),
        logical_bytes,
        allocated_bytes,
        active_count,
        active_bytes,
        evidence_incomplete_count,
        evidence_incomplete_bytes,
        recent_idle_count,
        recent_idle_bytes,
        stale_idle_count,
        stale_idle_bytes,
        recovery_candidate_count,
        recovery_candidate_bytes,
        structural_zip_candidate_item_count,
        structural_zip_recoverable_bytes,
        whole_file_structurally_complete_zip_count,
        whole_file_structurally_complete_zip_bytes,
        detected_type_count: items
            .iter()
            .filter(|item| item.detected_mime_type.is_some())
            .count(),
        acquisition_date_evidence_count: items
            .iter()
            .filter(|item| !item.download_acquired_dates.is_empty())
            .count(),
        production_time_evidence_count: items
            .iter()
            .filter(|item| item.production_time_evidence_present)
            .count(),
        final_sibling_count: items
            .iter()
            .filter(|item| item.final_sibling_exists)
            .count(),
        discard_review_bytes: stale_idle_bytes,
        audit_fingerprint: fingerprint,
        mutation_performed: false,
        items,
    }
}

/// Recursively audit Chromium-style incomplete downloads without following symlinks or mutating
/// any file. Modified age is used only to assess download staleness, never as production time.
#[cfg(not(coverage))]
pub fn collect_incomplete_download_audit(
    source_root: &Path,
    observed_at_ms: u64,
    max_entries: usize,
    stale_after_days: u64,
) -> Result<IncompleteDownloadAuditReport, String> {
    if !source_root.is_absolute() {
        return Err("incomplete-download-audit-root-must-be-absolute".into());
    }
    if !(1..=MAX_STALE_AFTER_DAYS).contains(&stale_after_days) {
        return Err("incomplete-download-stale-days-out-of-range".into());
    }
    let canonical_root = std::fs::canonicalize(source_root)
        .map_err(|_| "incomplete-download-audit-root-unavailable".to_string())?;
    let root_metadata = std::fs::symlink_metadata(&canonical_root)
        .map_err(|_| "incomplete-download-audit-root-unavailable".to_string())?;
    if !root_metadata.is_dir() || root_metadata.file_type().is_symlink() {
        return Err("incomplete-download-audit-root-unsafe".into());
    }
    let max_entries = max_entries.clamp(1, DEFAULT_MAX_ENTRIES);
    let mut evidence_complete = true;
    let mut issue_counts = BTreeMap::new();
    let mut entries_seen = 0usize;
    let mut items = Vec::new();
    let mut pending = vec![(canonical_root.clone(), 0usize)];

    while let Some((directory, depth)) = pending.pop() {
        let entries = match std::fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(_) => {
                evidence_complete = false;
                increment_issue(&mut issue_counts, "directory-read-failed");
                continue;
            }
        };
        let mut entries = entries.collect::<Vec<_>>();
        entries.sort_by(|left, right| {
            let left = left.as_ref().ok().map(|entry| entry.file_name());
            let right = right.as_ref().ok().map(|entry| entry.file_name());
            left.cmp(&right)
        });
        for entry in entries {
            if entries_seen >= max_entries {
                evidence_complete = false;
                increment_issue(&mut issue_counts, "entry-limit-reached");
                pending.clear();
                break;
            }
            entries_seen += 1;
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => {
                    evidence_complete = false;
                    increment_issue(&mut issue_counts, "directory-entry-read-failed");
                    continue;
                }
            };
            let file_type = match entry.file_type() {
                Ok(file_type) => file_type,
                Err(_) => {
                    evidence_complete = false;
                    increment_issue(&mut issue_counts, "file-type-read-failed");
                    continue;
                }
            };
            if file_type.is_symlink() {
                continue;
            }
            let path = entry.path();
            if file_type.is_dir() {
                if depth >= MAX_DEPTH {
                    evidence_complete = false;
                    increment_issue(&mut issue_counts, "depth-limit-reached");
                } else {
                    pending.push((path, depth + 1));
                }
                continue;
            }
            if !file_type.is_file() || !incomplete_download_name(&path) {
                continue;
            }
            match observe_item(&canonical_root, &path, observed_at_ms, stale_after_days) {
                Ok(item) => items.push(item),
                Err(reason) => {
                    evidence_complete = false;
                    increment_issue(&mut issue_counts, &reason);
                }
            }
        }
    }

    Ok(build_report(
        canonical_root.to_string_lossy().into_owned(),
        observed_at_ms,
        stale_after_days,
        entries_seen,
        evidence_complete,
        issue_counts,
        items,
    ))
}

pub fn summarize_incomplete_download_audit(
    report: &IncompleteDownloadAuditReport,
) -> IncompleteDownloadAuditSummary {
    IncompleteDownloadAuditSummary {
        schema_version: report.schema_version,
        output_mode: "incomplete-download-audit-summary".into(),
        observed_at_ms: report.observed_at_ms,
        source_scope_fingerprint: report.source_scope_fingerprint.clone(),
        stale_after_days: report.stale_after_days,
        evidence_complete: report.evidence_complete,
        entries_seen: report.entries_seen,
        issue_counts: report.issue_counts.clone(),
        file_count: report.file_count,
        logical_bytes: report.logical_bytes,
        allocated_bytes: report.allocated_bytes,
        active_count: report.active_count,
        active_bytes: report.active_bytes,
        evidence_incomplete_count: report.evidence_incomplete_count,
        evidence_incomplete_bytes: report.evidence_incomplete_bytes,
        recent_idle_count: report.recent_idle_count,
        recent_idle_bytes: report.recent_idle_bytes,
        stale_idle_count: report.stale_idle_count,
        stale_idle_bytes: report.stale_idle_bytes,
        recovery_candidate_count: report.recovery_candidate_count,
        recovery_candidate_bytes: report.recovery_candidate_bytes,
        structural_zip_candidate_item_count: report.structural_zip_candidate_item_count,
        structural_zip_recoverable_bytes: report.structural_zip_recoverable_bytes,
        whole_file_structurally_complete_zip_count: report
            .whole_file_structurally_complete_zip_count,
        whole_file_structurally_complete_zip_bytes: report
            .whole_file_structurally_complete_zip_bytes,
        detected_type_count: report.detected_type_count,
        acquisition_date_evidence_count: report.acquisition_date_evidence_count,
        production_time_evidence_count: report.production_time_evidence_count,
        final_sibling_count: report.final_sibling_count,
        discard_review_bytes: report.discard_review_bytes,
        audit_fingerprint: report.audit_fingerprint.clone(),
        mutation_performed: false,
        human_discard_approval_required: report.stale_idle_count > 0,
        automatic_discard_allowed: false,
        notices: vec![
            "read-only-dry-run".into(),
            "download-acquisition-date-is-not-production-date".into(),
            "filename-date-is-not-used-as-production-evidence".into(),
            "filesystem-modified-age-is-only-a-staleness-signal".into(),
            "magic-type-detection-does-not-prove-payload-completeness".into(),
            "whole-file-zip-structure-does-not-prove-every-entry-crc".into(),
            "partial-content-recovery-may-still-be-possible".into(),
            "fresh-audit-and-explicit-human-approval-required-before-discard".into(),
        ],
        redacted_from_summary: vec![
            "absolute-source-root".into(),
            "relative-file-path".into(),
            "filesystem-created-time".into(),
            "filesystem-modified-time".into(),
            "download-acquisition-date".into(),
            "download-agent".into(),
            "download-origin-host".into(),
            "active-process-identifiers".into(),
            "final-sibling-relative-path".into(),
        ],
        items: report
            .items
            .iter()
            .map(|item| IncompleteDownloadItemSummary {
                candidate_fingerprint: item.candidate_fingerprint.clone(),
                logical_bytes: item.logical_bytes,
                allocated_bytes: item.allocated_bytes,
                modified_age_days: item.modified_age_days,
                staleness_basis: item.staleness_basis.clone(),
                state: item.state,
                active_use_evidence_complete: item.active_use.evidence_complete,
                active: item.active_use.active,
                evidence_complete: item.evidence_complete,
                evidence_issue_count: item.evidence_issues.len(),
                detected_mime_type: item.detected_mime_type.clone(),
                detected_extension: item.detected_extension.clone(),
                structural_zip_candidate_count: item.structural_zip_candidate_count,
                structural_zip_recoverable_bytes: item.structural_zip_recoverable_bytes,
                whole_file_structurally_complete_zip: item.whole_file_structurally_complete_zip,
                zip_eocd_count: item.zip_eocd_count,
                download_acquisition_date_evidence_present: !item
                    .download_acquired_dates
                    .is_empty(),
                production_time_evidence_present: item.production_time_evidence_present,
                final_sibling_exists: item.final_sibling_exists,
                recovery_candidate: item.recovery_candidate,
                partial_content_recovery_possible: item.partial_content_recovery_possible,
                requires_human_review: true,
                automatic_discard_allowed: false,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn active_use(complete: bool, active: bool) -> ActiveUseEvidence {
        ActiveUseEvidence {
            method: "test".into(),
            evidence_complete: complete,
            active,
            observed_pids: active.then_some(42).into_iter().collect(),
            results_truncated: false,
            error: (!complete).then(|| "incomplete".into()),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn item(
        relative_path: &str,
        logical_bytes: u64,
        age_days: u64,
        active: ActiveUseEvidence,
        detected_mime_type: Option<&str>,
        structural_zip_candidate_count: u64,
    ) -> IncompleteDownloadAuditItem {
        build_item(
            "/source",
            relative_path.into(),
            logical_bytes,
            logical_bytes,
            10,
            20,
            20 + age_days * DAY_MS,
            DEFAULT_STALE_AFTER_DAYS,
            active,
            Vec::new(),
            detected_mime_type.map(str::to_string),
            detected_mime_type.map(|_| "bin".into()),
            (0..structural_zip_candidate_count)
                .map(|index| format!("start={index};end={};entries=1", index + 1))
                .collect(),
            u64::from(structural_zip_candidate_count > 0),
            (0..structural_zip_candidate_count)
                .map(|index| index.to_string())
                .collect(),
            vec!["2026-01-01".into()],
            vec!["Browser".into()],
            vec!["example.invalid".into()],
            false,
            None,
            None,
        )
    }

    #[test]
    fn detects_magic_type_from_bounded_bytes() {
        let mut png = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        png.resize(64, 0);
        assert_eq!(
            detect_content_type_from_bytes(&png),
            (Some("image/png".into()), Some("png".into()))
        );
        assert_eq!(detect_content_type_from_bytes(b"unknown"), (None, None));
    }

    #[test]
    fn structural_zip_ranges_are_bounded_merged_and_detect_whole_file() {
        let values = vec![
            "start=0;end=100;entries=2;central-directory-bytes=10".into(),
            "start=20;end=40;entries=1;central-directory-bytes=5".into(),
            "start=200;end=300;entries=1;central-directory-bytes=5".into(),
            "start=400;end=600;entries=1;central-directory-bytes=5".into(),
        ];
        assert_eq!(structural_zip_range_summary(&values, 500), (200, false));
        assert_eq!(
            structural_zip_range_summary(
                &["start=0;end=100;entries=2;central-directory-bytes=10".into()],
                100
            ),
            (100, true)
        );
        assert_eq!(structural_zip_range("start=10;end=10;entries=1"), None);
    }

    #[test]
    fn active_and_incomplete_evidence_fail_closed_before_age() {
        assert_eq!(
            classify_state(&active_use(true, true), 100, 30, false),
            IncompleteDownloadState::Active
        );
        assert_eq!(
            classify_state(&active_use(false, false), 100, 30, false),
            IncompleteDownloadState::ActiveUseEvidenceIncomplete
        );
    }

    #[test]
    fn recent_stale_and_recovery_states_are_distinct() {
        assert_eq!(
            classify_state(&active_use(true, false), 8, 30, true),
            IncompleteDownloadState::RecentIdle
        );
        assert_eq!(
            classify_state(&active_use(true, false), 70, 30, true),
            IncompleteDownloadState::StaleIdleRecoveryCandidate
        );
        assert_eq!(
            classify_state(&active_use(true, false), 70, 30, false),
            IncompleteDownloadState::StaleIdleReview
        );
    }

    #[test]
    fn report_totals_stale_review_without_granting_discard() {
        let report = build_report(
            "/source".into(),
            100 * DAY_MS,
            30,
            2,
            true,
            BTreeMap::new(),
            vec![
                item("old.crdownload", 100, 70, active_use(true, false), None, 0),
                item(
                    "recent.crdownload",
                    20,
                    8,
                    active_use(true, false),
                    Some("image/png"),
                    0,
                ),
            ],
        );
        assert_eq!(report.file_count, 2);
        assert_eq!(report.stale_idle_count, 1);
        assert_eq!(report.stale_idle_bytes, 100);
        assert_eq!(report.discard_review_bytes, 100);
        assert_eq!(report.structural_zip_candidate_item_count, 0);
        assert_eq!(report.structural_zip_recoverable_bytes, 0);
        assert!(report
            .items
            .iter()
            .all(|item| { item.requires_human_review && !item.automatic_discard_allowed }));
    }

    #[test]
    fn public_summary_redacts_paths_acquisition_context_and_pids() {
        let private = item(
            "private/client.crdownload",
            100,
            70,
            active_use(true, true),
            Some("application/zip"),
            1,
        );
        let report = build_report(
            "/source".into(),
            100 * DAY_MS,
            30,
            1,
            true,
            BTreeMap::new(),
            vec![private.clone()],
        );
        let encoded = serde_json::to_string(&summarize_incomplete_download_audit(&report)).unwrap();
        for sensitive in [
            "/source",
            "private",
            "client",
            "2026-01-01",
            "Browser",
            "example.invalid",
            "\"observed_pids\"",
            "\"filesystem_modified_ms\"",
        ] {
            assert!(!encoded.contains(sensitive), "{sensitive}");
        }
        assert!(encoded.contains(&private.candidate_fingerprint));
    }

    #[test]
    fn candidate_fingerprint_binds_active_and_content_state() {
        let idle = item("same.crdownload", 100, 70, active_use(true, false), None, 0);
        let active = item("same.crdownload", 100, 70, active_use(true, true), None, 0);
        let typed = item(
            "same.crdownload",
            100,
            70,
            active_use(true, false),
            Some("image/png"),
            0,
        );
        assert_ne!(idle.candidate_fingerprint, active.candidate_fingerprint);
        assert_ne!(idle.candidate_fingerprint, typed.candidate_fingerprint);
    }

    #[test]
    fn candidate_fingerprint_ignores_age_drift_within_state_but_binds_threshold_crossing() {
        let build_at = |observed_at_ms, filesystem_modified_ms| {
            build_item(
                "/source",
                "same.crdownload".into(),
                100,
                100,
                10,
                filesystem_modified_ms,
                observed_at_ms,
                DEFAULT_STALE_AFTER_DAYS,
                active_use(true, false),
                Vec::new(),
                Some("image/png".into()),
                Some("png".into()),
                Vec::new(),
                0,
                Vec::new(),
                vec!["2026-01-01".into()],
                vec!["Browser".into()],
                vec!["example.invalid".into()],
                false,
                None,
                None,
            )
        };

        let modified_ms = 20;
        let stale_day_70 = build_at(modified_ms + 70 * DAY_MS, modified_ms);
        let stale_day_71 = build_at(modified_ms + 71 * DAY_MS, modified_ms);
        assert_eq!(stale_day_70.modified_age_days, 70);
        assert_eq!(stale_day_71.modified_age_days, 71);
        assert_eq!(stale_day_70.state, stale_day_71.state);
        assert_eq!(
            stale_day_70.candidate_fingerprint,
            stale_day_71.candidate_fingerprint
        );

        let recent_day_29 = build_at(modified_ms + 29 * DAY_MS, modified_ms);
        let stale_day_30 = build_at(modified_ms + 30 * DAY_MS, modified_ms);
        assert_ne!(recent_day_29.state, stale_day_30.state);
        assert_ne!(
            recent_day_29.candidate_fingerprint,
            stale_day_30.candidate_fingerprint
        );

        let changed_modified_ms = modified_ms + 1;
        let changed_source = build_at(changed_modified_ms + 70 * DAY_MS, changed_modified_ms);
        assert_ne!(
            stale_day_70.candidate_fingerprint,
            changed_source.candidate_fingerprint
        );
    }

    #[test]
    fn recognizes_case_insensitive_suffix_and_derives_sibling() {
        assert!(incomplete_download_name(Path::new("a.CRDOWNLOAD")));
        assert!(!incomplete_download_name(Path::new("a.download")));
        assert_eq!(
            final_sibling_path(Path::new("/a/report.zip.crdownload")),
            Some(PathBuf::from("/a/report.zip"))
        );
    }

    #[test]
    fn issue_collection_is_bounded() {
        let mut issues = BTreeMap::new();
        for index in 0..100 {
            increment_issue(&mut issues, &format!("issue-{index}"));
        }
        assert!(issues.len() <= MAX_RECORDED_ISSUE_KINDS + 1);
        assert!(issues.contains_key("additional-issue-kinds-truncated"));
    }
}
