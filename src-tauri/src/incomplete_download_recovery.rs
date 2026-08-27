use crate::cloud_local_eviction::observe_path_active_use;
use crate::incomplete_download::{
    IncompleteDownloadAuditItem, IncompleteDownloadAuditReport, IncompleteDownloadState,
};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};

pub const INCOMPLETE_DOWNLOAD_RECOVERY_VERSION: u32 = 1;
pub const DEFAULT_MAX_ZIP_ENTRIES: usize = 10_000;
pub const DEFAULT_MAX_ZIP_TOTAL_UNCOMPRESSED_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub const DEFAULT_MAX_ZIP_SINGLE_UNCOMPRESSED_BYTES: u64 = 512 * 1024 * 1024;
pub const DEFAULT_MAX_PNG_OUTPUT_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_ZIP_ENTRIES: usize = 100_000;
pub const MAX_ZIP_TOTAL_UNCOMPRESSED_BYTES: u64 = 8 * 1024 * 1024 * 1024;
pub const MAX_ZIP_SINGLE_UNCOMPRESSED_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub const MAX_PNG_OUTPUT_BYTES: u64 = 512 * 1024 * 1024;
const IO_BUFFER_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryValidationLimits {
    pub max_zip_entries: usize,
    pub max_zip_total_uncompressed_bytes: u64,
    pub max_zip_single_uncompressed_bytes: u64,
    pub max_png_output_bytes: u64,
}

impl Default for RecoveryValidationLimits {
    fn default() -> Self {
        Self {
            max_zip_entries: DEFAULT_MAX_ZIP_ENTRIES,
            max_zip_total_uncompressed_bytes: DEFAULT_MAX_ZIP_TOTAL_UNCOMPRESSED_BYTES,
            max_zip_single_uncompressed_bytes: DEFAULT_MAX_ZIP_SINGLE_UNCOMPRESSED_BYTES,
            max_png_output_bytes: DEFAULT_MAX_PNG_OUTPUT_BYTES,
        }
    }
}

impl RecoveryValidationLimits {
    fn validate(self) -> Result<Self, String> {
        if self.max_zip_entries == 0 || self.max_zip_entries > MAX_ZIP_ENTRIES {
            return Err("recovery-validation-zip-entry-limit-out-of-range".into());
        }
        if self.max_zip_total_uncompressed_bytes == 0
            || self.max_zip_total_uncompressed_bytes > MAX_ZIP_TOTAL_UNCOMPRESSED_BYTES
        {
            return Err("recovery-validation-zip-total-limit-out-of-range".into());
        }
        if self.max_zip_single_uncompressed_bytes == 0
            || self.max_zip_single_uncompressed_bytes > MAX_ZIP_SINGLE_UNCOMPRESSED_BYTES
            || self.max_zip_single_uncompressed_bytes > self.max_zip_total_uncompressed_bytes
        {
            return Err("recovery-validation-zip-single-limit-out-of-range".into());
        }
        if self.max_png_output_bytes == 0 || self.max_png_output_bytes > MAX_PNG_OUTPUT_BYTES {
            return Err("recovery-validation-png-output-limit-out-of-range".into());
        }
        Ok(self)
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "kebab-case")]
pub enum RecoveryValidationKind {
    PngFullFile,
    ZipWholeFile,
    ZipEmbeddedRange,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "kebab-case")]
pub enum ContentValidationStatus {
    Validated,
    Invalid,
    LimitExceeded,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "kebab-case")]
pub enum RecoveryItemStatus {
    FullyValidated,
    PartiallyValidated,
    Invalid,
    LimitExceeded,
    Unsupported,
    SkippedActive,
    SkippedEvidenceIncomplete,
    ChangedDuringValidation,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContentValidation {
    pub kind: RecoveryValidationKind,
    pub status: ContentValidationStatus,
    pub range_start: u64,
    pub range_end: u64,
    pub span_bytes: u64,
    pub entry_count: usize,
    pub declared_uncompressed_bytes: u64,
    pub validated_uncompressed_bytes: u64,
    pub decoded_frame_count: u32,
    pub decoded_output_bytes: u64,
    pub reason_code: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncompleteDownloadRecoveryItem {
    pub candidate_fingerprint: String,
    pub relative_path: String,
    pub logical_bytes: u64,
    pub state: IncompleteDownloadState,
    pub detected_mime_type: Option<String>,
    pub status: RecoveryItemStatus,
    pub evidence_complete: bool,
    pub validation_stable: bool,
    pub validated_recoverable_bytes: u64,
    pub fully_validated_file: bool,
    pub validations: Vec<ContentValidation>,
    pub reason_codes: Vec<String>,
    pub requires_human_recovery_action: bool,
    pub automatic_rename_allowed: bool,
    pub automatic_discard_allowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncompleteDownloadRecoveryReport {
    pub schema_version: u32,
    pub observed_at_ms: u64,
    pub source_root: String,
    pub source_scope_fingerprint: String,
    pub audit_fingerprint: String,
    pub limits: RecoveryValidationLimits,
    pub audit_evidence_complete: bool,
    pub evidence_complete: bool,
    pub issue_counts: BTreeMap<String, u64>,
    pub candidate_count: usize,
    pub candidate_bytes: u64,
    pub fully_validated_file_count: usize,
    pub fully_validated_file_bytes: u64,
    pub partially_validated_file_count: usize,
    pub validated_recoverable_bytes: u64,
    pub invalid_count: usize,
    pub limit_exceeded_count: usize,
    pub unsupported_count: usize,
    pub skipped_count: usize,
    pub validation_fingerprint: String,
    pub mutation_performed: bool,
    pub items: Vec<IncompleteDownloadRecoveryItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContentValidationSummary {
    pub kind: RecoveryValidationKind,
    pub status: ContentValidationStatus,
    pub span_bytes: u64,
    pub entry_count: usize,
    pub declared_uncompressed_bytes: u64,
    pub validated_uncompressed_bytes: u64,
    pub decoded_frame_count: u32,
    pub decoded_output_bytes: u64,
    pub reason_code: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncompleteDownloadRecoveryItemSummary {
    pub candidate_fingerprint: String,
    pub logical_bytes: u64,
    pub state: IncompleteDownloadState,
    pub detected_mime_type: Option<String>,
    pub status: RecoveryItemStatus,
    pub evidence_complete: bool,
    pub validation_stable: bool,
    pub validated_recoverable_bytes: u64,
    pub fully_validated_file: bool,
    pub validations: Vec<ContentValidationSummary>,
    pub reason_codes: Vec<String>,
    pub requires_human_recovery_action: bool,
    pub automatic_rename_allowed: bool,
    pub automatic_discard_allowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncompleteDownloadRecoverySummary {
    pub schema_version: u32,
    pub output_mode: String,
    pub observed_at_ms: u64,
    pub source_scope_fingerprint: String,
    pub audit_fingerprint: String,
    pub limits: RecoveryValidationLimits,
    pub audit_evidence_complete: bool,
    pub evidence_complete: bool,
    pub issue_counts: BTreeMap<String, u64>,
    pub candidate_count: usize,
    pub candidate_bytes: u64,
    pub fully_validated_file_count: usize,
    pub fully_validated_file_bytes: u64,
    pub partially_validated_file_count: usize,
    pub validated_recoverable_bytes: u64,
    pub invalid_count: usize,
    pub limit_exceeded_count: usize,
    pub unsupported_count: usize,
    pub skipped_count: usize,
    pub validation_fingerprint: String,
    pub mutation_performed: bool,
    pub human_recovery_action_required: bool,
    pub automatic_rename_allowed: bool,
    pub automatic_discard_allowed: bool,
    pub notices: Vec<String>,
    pub redacted_from_summary: Vec<String>,
    pub items: Vec<IncompleteDownloadRecoveryItemSummary>,
}

#[derive(Debug)]
struct FileRangeReader {
    file: File,
    start: u64,
    len: u64,
    position: u64,
}

impl FileRangeReader {
    fn new(mut file: File, start: u64, end: u64) -> Result<Self, String> {
        let len = end
            .checked_sub(start)
            .filter(|len| *len > 0)
            .ok_or_else(|| "zip-range-invalid".to_string())?;
        let file_len = file
            .metadata()
            .map_err(|_| "zip-range-metadata-failed".to_string())?
            .len();
        if end > file_len {
            return Err("zip-range-outside-file".into());
        }
        file.seek(SeekFrom::Start(start))
            .map_err(|_| "zip-range-seek-failed".to_string())?;
        Ok(Self {
            file,
            start,
            len,
            position: 0,
        })
    }
}

impl Read for FileRangeReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if self.position >= self.len || buffer.is_empty() {
            return Ok(0);
        }
        self.file
            .seek(SeekFrom::Start(self.start + self.position))?;
        let remaining = self.len - self.position;
        let remaining = usize::try_from(remaining).unwrap_or(usize::MAX);
        let bounded_len = buffer.len().min(remaining);
        let count = self.file.read(&mut buffer[..bounded_len])?;
        self.position = self.position.saturating_add(count as u64);
        Ok(count)
    }
}

impl Seek for FileRangeReader {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        let next = match position {
            SeekFrom::Start(offset) => i128::from(offset),
            SeekFrom::Current(offset) => i128::from(self.position) + i128::from(offset),
            SeekFrom::End(offset) => i128::from(self.len) + i128::from(offset),
        };
        if next < 0 || next > i128::from(self.len) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "seek outside bounded file range",
            ));
        }
        self.position = next as u64;
        Ok(self.position)
    }
}

fn system_time_ms(time: std::time::SystemTime) -> u64 {
    time.duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn parse_structural_zip_range(value: &str) -> Option<(u64, u64, usize)> {
    let mut start = None;
    let mut end = None;
    let mut entries = None;
    for field in value.split(';') {
        let (key, value) = field.split_once('=')?;
        match key {
            "start" => start = value.parse().ok(),
            "end" => end = value.parse().ok(),
            "entries" => entries = value.parse().ok(),
            _ => {}
        }
    }
    let (start, end, entries) = (start?, end?, entries?);
    (end > start).then_some((start, end, entries))
}

fn safe_candidate_path(canonical_root: &Path, relative_path: &str) -> Result<PathBuf, String> {
    let relative = Path::new(relative_path);
    if relative.as_os_str().is_empty()
        || relative.is_absolute()
        || !relative
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err("recovery-candidate-relative-path-unsafe".into());
    }
    let mut current = canonical_root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(value) = component else {
            return Err("recovery-candidate-relative-path-unsafe".into());
        };
        current.push(value);
        let metadata = std::fs::symlink_metadata(&current)
            .map_err(|_| "recovery-candidate-unavailable".to_string())?;
        if metadata.file_type().is_symlink() {
            return Err("recovery-candidate-symlink-rejected".into());
        }
    }
    let canonical = std::fs::canonicalize(&current)
        .map_err(|_| "recovery-candidate-unavailable".to_string())?;
    if !canonical.starts_with(canonical_root) {
        return Err("recovery-candidate-outside-root".into());
    }
    Ok(canonical)
}

fn content_validation(
    kind: RecoveryValidationKind,
    status: ContentValidationStatus,
    start: u64,
    end: u64,
    reason_code: &str,
) -> ContentValidation {
    ContentValidation {
        kind,
        status,
        range_start: start,
        range_end: end,
        span_bytes: end.saturating_sub(start),
        entry_count: 0,
        declared_uncompressed_bytes: 0,
        validated_uncompressed_bytes: 0,
        decoded_frame_count: 0,
        decoded_output_bytes: 0,
        reason_code: reason_code.into(),
    }
}

fn validate_png(path: &Path, logical_bytes: u64, limit: u64) -> ContentValidation {
    let mut result = content_validation(
        RecoveryValidationKind::PngFullFile,
        ContentValidationStatus::Invalid,
        0,
        logical_bytes,
        "png-decode-failed",
    );
    let file = match File::open(path) {
        Ok(file) => file,
        Err(_) => {
            result.reason_code = "png-open-failed".into();
            return result;
        }
    };
    let decoder = png::Decoder::new(std::io::BufReader::new(file));
    let mut reader = match decoder.read_info() {
        Ok(reader) => reader,
        Err(_) => return result,
    };
    let worst_case_output_bytes = u64::from(reader.info().width)
        .checked_mul(u64::from(reader.info().height))
        .and_then(|pixels| pixels.checked_mul(8));
    if worst_case_output_bytes.is_none_or(|bytes| bytes == 0 || bytes > limit) {
        result.status = ContentValidationStatus::LimitExceeded;
        result.reason_code = "png-dimension-output-limit-exceeded".into();
        result.decoded_output_bytes = worst_case_output_bytes.unwrap_or(u64::MAX);
        return result;
    }
    let output_bytes = reader
        .output_buffer_size()
        .and_then(|bytes| u64::try_from(bytes).ok())
        .unwrap_or(u64::MAX);
    if output_bytes == 0 || output_bytes > limit {
        result.status = ContentValidationStatus::LimitExceeded;
        result.reason_code = "png-output-limit-exceeded".into();
        result.decoded_output_bytes = output_bytes;
        return result;
    }
    let frame_count = reader
        .info()
        .animation_control
        .as_ref()
        .map(|control| control.num_frames)
        .unwrap_or(1);
    let mut buffer = Vec::new();
    if buffer.try_reserve_exact(output_bytes as usize).is_err() {
        result.status = ContentValidationStatus::LimitExceeded;
        result.reason_code = "png-output-allocation-failed".into();
        result.decoded_output_bytes = output_bytes;
        return result;
    }
    buffer.resize(output_bytes as usize, 0);
    let mut decoded_output_bytes = 0u64;
    for _ in 0..frame_count {
        match reader.next_frame(&mut buffer) {
            Ok(frame) => {
                decoded_output_bytes =
                    decoded_output_bytes.saturating_add(frame.buffer_size() as u64);
                if decoded_output_bytes > limit {
                    result.status = ContentValidationStatus::LimitExceeded;
                    result.reason_code = "png-total-frame-output-limit-exceeded".into();
                    result.decoded_frame_count = frame_count;
                    result.decoded_output_bytes = decoded_output_bytes;
                    return result;
                }
            }
            Err(_) => return result,
        }
    }
    result.status = ContentValidationStatus::Validated;
    result.reason_code = "png-all-declared-frames-decoded".into();
    result.decoded_frame_count = frame_count;
    result.decoded_output_bytes = decoded_output_bytes;
    result
}

fn validate_zip_range(
    path: &Path,
    start: u64,
    end: u64,
    expected_entries: usize,
    logical_bytes: u64,
    limits: RecoveryValidationLimits,
) -> ContentValidation {
    let kind = if start == 0 && end == logical_bytes {
        RecoveryValidationKind::ZipWholeFile
    } else {
        RecoveryValidationKind::ZipEmbeddedRange
    };
    let mut result = content_validation(
        kind,
        ContentValidationStatus::Invalid,
        start,
        end,
        "zip-open-or-central-directory-failed",
    );
    result.entry_count = expected_entries;
    if expected_entries > limits.max_zip_entries {
        result.status = ContentValidationStatus::LimitExceeded;
        result.reason_code = "zip-structural-entry-limit-exceeded".into();
        return result;
    }
    let file = match File::open(path) {
        Ok(file) => file,
        Err(_) => {
            result.reason_code = "zip-open-failed".into();
            return result;
        }
    };
    let reader = match FileRangeReader::new(file, start, end) {
        Ok(reader) => reader,
        Err(reason) => {
            result.reason_code = reason;
            return result;
        }
    };
    let mut archive = match zip::ZipArchive::new(reader) {
        Ok(archive) => archive,
        Err(_) => return result,
    };
    result.entry_count = archive.len();
    if archive.len() > limits.max_zip_entries {
        result.status = ContentValidationStatus::LimitExceeded;
        result.reason_code = "zip-entry-limit-exceeded".into();
        return result;
    }

    let mut declared_total = 0u64;
    for index in 0..archive.len() {
        let file = match archive.by_index(index) {
            Ok(file) => file,
            Err(_) => {
                result.reason_code = "zip-entry-open-failed".into();
                return result;
            }
        };
        if file.size() > limits.max_zip_single_uncompressed_bytes {
            result.status = ContentValidationStatus::LimitExceeded;
            result.reason_code = "zip-single-entry-limit-exceeded".into();
            result.declared_uncompressed_bytes = file.size();
            return result;
        }
        declared_total = match declared_total.checked_add(file.size()) {
            Some(total) => total,
            None => {
                result.status = ContentValidationStatus::LimitExceeded;
                result.reason_code = "zip-total-size-overflow".into();
                return result;
            }
        };
        if declared_total > limits.max_zip_total_uncompressed_bytes {
            result.status = ContentValidationStatus::LimitExceeded;
            result.reason_code = "zip-total-uncompressed-limit-exceeded".into();
            result.declared_uncompressed_bytes = declared_total;
            return result;
        }
    }
    result.declared_uncompressed_bytes = declared_total;

    let mut validated_total = 0u64;
    let mut buffer = vec![0u8; IO_BUFFER_BYTES];
    for index in 0..archive.len() {
        let mut file = match archive.by_index(index) {
            Ok(file) => file,
            Err(_) => {
                result.reason_code = "zip-entry-open-failed".into();
                return result;
            }
        };
        loop {
            let count = match file.read(&mut buffer) {
                Ok(count) => count,
                Err(_) => {
                    result.reason_code = "zip-entry-read-or-crc-failed".into();
                    result.validated_uncompressed_bytes = validated_total;
                    return result;
                }
            };
            if count == 0 {
                break;
            }
            validated_total = validated_total.saturating_add(count as u64);
            if validated_total > limits.max_zip_total_uncompressed_bytes {
                result.status = ContentValidationStatus::LimitExceeded;
                result.reason_code = "zip-observed-output-limit-exceeded".into();
                result.validated_uncompressed_bytes = validated_total;
                return result;
            }
        }
    }
    result.status = ContentValidationStatus::Validated;
    result.reason_code = "zip-all-entries-read-to-eof-with-crc".into();
    result.validated_uncompressed_bytes = validated_total;
    result
}

fn increment_issue(issues: &mut BTreeMap<String, u64>, reason: &str) {
    *issues.entry(reason.into()).or_default() += 1;
}

fn validated_range_union(validations: &[ContentValidation], logical_bytes: u64) -> u64 {
    let mut ranges = validations
        .iter()
        .filter(|validation| validation.status == ContentValidationStatus::Validated)
        .map(|validation| (validation.range_start, validation.range_end))
        .filter(|(start, end)| start < end && *end <= logical_bytes)
        .collect::<Vec<_>>();
    ranges.sort_unstable();
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
    merged.iter().fold(0u64, |total, (start, end)| {
        total.saturating_add(end.saturating_sub(*start))
    })
}

fn skipped_item(
    item: &IncompleteDownloadAuditItem,
    status: RecoveryItemStatus,
    reason: &str,
) -> IncompleteDownloadRecoveryItem {
    IncompleteDownloadRecoveryItem {
        candidate_fingerprint: item.candidate_fingerprint.clone(),
        relative_path: item.relative_path.clone(),
        logical_bytes: item.logical_bytes,
        state: item.state,
        detected_mime_type: item.detected_mime_type.clone(),
        status,
        evidence_complete: false,
        validation_stable: false,
        validated_recoverable_bytes: 0,
        fully_validated_file: false,
        validations: Vec::new(),
        reason_codes: vec![reason.into()],
        requires_human_recovery_action: true,
        automatic_rename_allowed: false,
        automatic_discard_allowed: false,
    }
}

fn validate_item(
    canonical_root: &Path,
    item: &IncompleteDownloadAuditItem,
    limits: RecoveryValidationLimits,
) -> IncompleteDownloadRecoveryItem {
    if !item.evidence_complete || !item.active_use.evidence_complete {
        return skipped_item(
            item,
            RecoveryItemStatus::SkippedEvidenceIncomplete,
            "audit-item-evidence-incomplete",
        );
    }
    if item.active_use.active {
        return skipped_item(item, RecoveryItemStatus::SkippedActive, "audit-item-active");
    }
    let path = match safe_candidate_path(canonical_root, &item.relative_path) {
        Ok(path) => path,
        Err(reason) => {
            return skipped_item(item, RecoveryItemStatus::SkippedEvidenceIncomplete, &reason)
        }
    };
    let before = match std::fs::symlink_metadata(&path) {
        Ok(metadata)
            if metadata.is_file()
                && !metadata.file_type().is_symlink()
                && metadata.len() == item.logical_bytes
                && metadata.modified().map(system_time_ms).unwrap_or_default()
                    == item.filesystem_modified_ms =>
        {
            metadata
        }
        _ => {
            return skipped_item(
                item,
                RecoveryItemStatus::ChangedDuringValidation,
                "candidate-changed-since-audit",
            )
        }
    };
    let active_before = observe_path_active_use(&path);
    if !active_before.evidence_complete {
        return skipped_item(
            item,
            RecoveryItemStatus::SkippedEvidenceIncomplete,
            "pre-validation-active-use-evidence-incomplete",
        );
    }
    if active_before.active {
        return skipped_item(
            item,
            RecoveryItemStatus::SkippedActive,
            "candidate-active-before-validation",
        );
    }

    let mut validations = Vec::new();
    if item.detected_mime_type.as_deref() == Some("image/png") {
        validations.push(validate_png(
            &path,
            item.logical_bytes,
            limits.max_png_output_bytes,
        ));
    }
    let mut zip_ranges = item
        .structural_zip_candidates
        .iter()
        .filter_map(|value| parse_structural_zip_range(value))
        .filter(|(start, end, _)| start < end && *end <= item.logical_bytes)
        .collect::<Vec<_>>();
    zip_ranges.sort_unstable();
    zip_ranges.dedup();
    for (start, end, expected_entries) in zip_ranges {
        validations.push(validate_zip_range(
            &path,
            start,
            end,
            expected_entries,
            item.logical_bytes,
            limits,
        ));
    }

    let active_after = observe_path_active_use(&path);
    let after = std::fs::symlink_metadata(&path).ok();
    let stable = active_after.evidence_complete
        && !active_after.active
        && after.as_ref().is_some_and(|metadata| {
            metadata.is_file()
                && !metadata.file_type().is_symlink()
                && metadata.len() == before.len()
                && metadata.modified().ok() == before.modified().ok()
        });
    if !stable {
        let mut skipped = skipped_item(
            item,
            RecoveryItemStatus::ChangedDuringValidation,
            if !active_after.evidence_complete {
                "post-validation-active-use-evidence-incomplete"
            } else if active_after.active {
                "candidate-active-after-validation"
            } else {
                "candidate-changed-during-validation"
            },
        );
        skipped.validations = validations;
        return skipped;
    }

    let fully_validated_file = validations.iter().any(|validation| {
        validation.status == ContentValidationStatus::Validated
            && validation.range_start == 0
            && validation.range_end == item.logical_bytes
    });
    let validated_recoverable_bytes = validated_range_union(&validations, item.logical_bytes);
    let any_valid = validations
        .iter()
        .any(|validation| validation.status == ContentValidationStatus::Validated);
    let any_invalid = validations
        .iter()
        .any(|validation| validation.status == ContentValidationStatus::Invalid);
    let any_limit = validations
        .iter()
        .any(|validation| validation.status == ContentValidationStatus::LimitExceeded);
    let status = if fully_validated_file {
        RecoveryItemStatus::FullyValidated
    } else if any_valid {
        RecoveryItemStatus::PartiallyValidated
    } else if any_limit {
        RecoveryItemStatus::LimitExceeded
    } else if any_invalid {
        RecoveryItemStatus::Invalid
    } else {
        RecoveryItemStatus::Unsupported
    };
    let mut reason_codes = validations
        .iter()
        .map(|validation| validation.reason_code.clone())
        .collect::<Vec<_>>();
    if reason_codes.is_empty() {
        reason_codes.push("no-supported-full-file-or-structural-range-validator".into());
    }
    reason_codes.sort();
    reason_codes.dedup();
    IncompleteDownloadRecoveryItem {
        candidate_fingerprint: item.candidate_fingerprint.clone(),
        relative_path: item.relative_path.clone(),
        logical_bytes: item.logical_bytes,
        state: item.state,
        detected_mime_type: item.detected_mime_type.clone(),
        status,
        evidence_complete: true,
        validation_stable: true,
        validated_recoverable_bytes,
        fully_validated_file,
        validations,
        reason_codes,
        requires_human_recovery_action: true,
        automatic_rename_allowed: false,
        automatic_discard_allowed: false,
    }
}

fn validation_fingerprint(
    audit: &IncompleteDownloadAuditReport,
    limits: RecoveryValidationLimits,
    evidence_complete: bool,
    issues: &BTreeMap<String, u64>,
    items: &[IncompleteDownloadRecoveryItem],
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-incomplete-download-recovery-validation-v1\0");
    hasher.update(audit.source_scope_fingerprint.as_bytes());
    hasher.update(&[0]);
    hasher.update(audit.audit_fingerprint.as_bytes());
    hasher.update(&[u8::from(evidence_complete)]);
    hasher.update(&(limits.max_zip_entries as u64).to_le_bytes());
    hasher.update(&limits.max_zip_total_uncompressed_bytes.to_le_bytes());
    hasher.update(&limits.max_zip_single_uncompressed_bytes.to_le_bytes());
    hasher.update(&limits.max_png_output_bytes.to_le_bytes());
    for (reason, count) in issues {
        hasher.update(reason.as_bytes());
        hasher.update(&[0]);
        hasher.update(&count.to_le_bytes());
    }
    for item in items {
        hasher.update(item.candidate_fingerprint.as_bytes());
        hasher.update(&[0]);
        hasher.update(format!("{:?}", item.status).as_bytes());
        hasher.update(&item.validated_recoverable_bytes.to_le_bytes());
        hasher.update(&[
            u8::from(item.evidence_complete),
            u8::from(item.validation_stable),
            u8::from(item.fully_validated_file),
        ]);
        for validation in &item.validations {
            hasher.update(format!("{:?}", validation.kind).as_bytes());
            hasher.update(format!("{:?}", validation.status).as_bytes());
            hasher.update(&validation.range_start.to_le_bytes());
            hasher.update(&validation.range_end.to_le_bytes());
            hasher.update(&(validation.entry_count as u64).to_le_bytes());
            hasher.update(&validation.declared_uncompressed_bytes.to_le_bytes());
            hasher.update(&validation.validated_uncompressed_bytes.to_le_bytes());
            hasher.update(&validation.decoded_frame_count.to_le_bytes());
            hasher.update(&validation.decoded_output_bytes.to_le_bytes());
            hasher.update(validation.reason_code.as_bytes());
            hasher.update(&[0]);
        }
    }
    hasher.finalize().to_hex().to_string()
}

pub(crate) fn incomplete_download_recovery_integrity_valid(
    audit: &IncompleteDownloadAuditReport,
    report: &IncompleteDownloadRecoveryReport,
) -> bool {
    if report.schema_version != INCOMPLETE_DOWNLOAD_RECOVERY_VERSION
        || report.mutation_performed
        || report.source_root != audit.source_root
        || report.source_scope_fingerprint != audit.source_scope_fingerprint
        || report.audit_fingerprint != audit.audit_fingerprint
        || report.audit_evidence_complete != audit.evidence_complete
        || report
            .items
            .windows(2)
            .any(|items| items[0].candidate_fingerprint >= items[1].candidate_fingerprint)
    {
        return false;
    }

    let audit_candidates = audit
        .items
        .iter()
        .filter(|item| item.recovery_candidate)
        .collect::<Vec<_>>();
    if audit_candidates.len() != report.items.len()
        || audit_candidates
            .iter()
            .zip(&report.items)
            .any(|(audit_item, recovery_item)| {
                audit_item.candidate_fingerprint != recovery_item.candidate_fingerprint
                    || audit_item.relative_path != recovery_item.relative_path
                    || audit_item.logical_bytes != recovery_item.logical_bytes
                    || audit_item.state != recovery_item.state
                    || audit_item.detected_mime_type != recovery_item.detected_mime_type
            })
    {
        return false;
    }

    let totals = |status: RecoveryItemStatus| {
        let items = report.items.iter().filter(|item| item.status == status);
        (
            items.clone().count(),
            items.fold(0u64, |total, item| total.saturating_add(item.logical_bytes)),
        )
    };
    let (fully_validated_file_count, fully_validated_file_bytes) =
        totals(RecoveryItemStatus::FullyValidated);
    let (partially_validated_file_count, _) = totals(RecoveryItemStatus::PartiallyValidated);
    let (invalid_count, _) = totals(RecoveryItemStatus::Invalid);
    let (limit_exceeded_count, _) = totals(RecoveryItemStatus::LimitExceeded);
    let (unsupported_count, _) = totals(RecoveryItemStatus::Unsupported);
    let skipped_count = report
        .items
        .iter()
        .filter(|item| {
            matches!(
                item.status,
                RecoveryItemStatus::SkippedActive
                    | RecoveryItemStatus::SkippedEvidenceIncomplete
                    | RecoveryItemStatus::ChangedDuringValidation
            )
        })
        .count();
    let evidence_complete = audit.evidence_complete
        && report
            .items
            .iter()
            .all(|item| item.evidence_complete && item.validation_stable);

    report.candidate_count == report.items.len()
        && report.candidate_bytes
            == report
                .items
                .iter()
                .fold(0u64, |total, item| total.saturating_add(item.logical_bytes))
        && report.fully_validated_file_count == fully_validated_file_count
        && report.fully_validated_file_bytes == fully_validated_file_bytes
        && report.partially_validated_file_count == partially_validated_file_count
        && report.validated_recoverable_bytes
            == report.items.iter().fold(0u64, |total, item| {
                total.saturating_add(item.validated_recoverable_bytes)
            })
        && report.invalid_count == invalid_count
        && report.limit_exceeded_count == limit_exceeded_count
        && report.unsupported_count == unsupported_count
        && report.skipped_count == skipped_count
        && report.evidence_complete == evidence_complete
        && validation_fingerprint(
            audit,
            report.limits,
            report.evidence_complete,
            &report.issue_counts,
            &report.items,
        ) == report.validation_fingerprint
}

/// Validate recoverable content in a fresh incomplete-download audit without writing, extracting,
/// renaming, or deleting any candidate. ZIP entries are streamed to EOF so the zip crate's default
/// CRC reader checks each entry. PNG frames are decoded within an explicit output-memory bound.
pub fn validate_incomplete_download_recovery(
    source_root: &Path,
    audit: &IncompleteDownloadAuditReport,
    observed_at_ms: u64,
    limits: RecoveryValidationLimits,
) -> Result<IncompleteDownloadRecoveryReport, String> {
    let limits = limits.validate()?;
    if !source_root.is_absolute() {
        return Err("recovery-validation-root-must-be-absolute".into());
    }
    let canonical_root = std::fs::canonicalize(source_root)
        .map_err(|_| "recovery-validation-root-unavailable".to_string())?;
    let root_metadata = std::fs::symlink_metadata(&canonical_root)
        .map_err(|_| "recovery-validation-root-unavailable".to_string())?;
    if !root_metadata.is_dir() || root_metadata.file_type().is_symlink() {
        return Err("recovery-validation-root-unsafe".into());
    }
    if audit.source_root != canonical_root.to_string_lossy() {
        return Err("recovery-validation-audit-root-mismatch".into());
    }
    if audit.mutation_performed {
        return Err("recovery-validation-rejects-mutating-audit".into());
    }

    let mut items = audit
        .items
        .iter()
        .filter(|item| item.recovery_candidate)
        .map(|item| validate_item(&canonical_root, item, limits))
        .collect::<Vec<_>>();
    items.sort_by(|left, right| left.candidate_fingerprint.cmp(&right.candidate_fingerprint));
    let mut issues = BTreeMap::new();
    for item in &items {
        if !item.evidence_complete || !item.validation_stable {
            for reason in &item.reason_codes {
                increment_issue(&mut issues, reason);
            }
        }
    }
    let evidence_complete = audit.evidence_complete
        && items
            .iter()
            .all(|item| item.evidence_complete && item.validation_stable);
    let totals = |predicate: &dyn Fn(&IncompleteDownloadRecoveryItem) -> bool| {
        let selected = items.iter().filter(|item| predicate(item));
        (
            selected.clone().count(),
            selected.fold(0u64, |total, item| total.saturating_add(item.logical_bytes)),
        )
    };
    let (fully_validated_file_count, fully_validated_file_bytes) =
        totals(&|item| item.status == RecoveryItemStatus::FullyValidated);
    let (partially_validated_file_count, _) =
        totals(&|item| item.status == RecoveryItemStatus::PartiallyValidated);
    let (invalid_count, _) = totals(&|item| item.status == RecoveryItemStatus::Invalid);
    let (limit_exceeded_count, _) =
        totals(&|item| item.status == RecoveryItemStatus::LimitExceeded);
    let (unsupported_count, _) = totals(&|item| item.status == RecoveryItemStatus::Unsupported);
    let (skipped_count, _) = totals(&|item| {
        matches!(
            item.status,
            RecoveryItemStatus::SkippedActive
                | RecoveryItemStatus::SkippedEvidenceIncomplete
                | RecoveryItemStatus::ChangedDuringValidation
        )
    });
    let candidate_bytes = items
        .iter()
        .fold(0u64, |total, item| total.saturating_add(item.logical_bytes));
    let validated_recoverable_bytes = items.iter().fold(0u64, |total, item| {
        total.saturating_add(item.validated_recoverable_bytes)
    });
    let fingerprint = validation_fingerprint(audit, limits, evidence_complete, &issues, &items);
    Ok(IncompleteDownloadRecoveryReport {
        schema_version: INCOMPLETE_DOWNLOAD_RECOVERY_VERSION,
        observed_at_ms,
        source_root: canonical_root.to_string_lossy().into_owned(),
        source_scope_fingerprint: audit.source_scope_fingerprint.clone(),
        audit_fingerprint: audit.audit_fingerprint.clone(),
        limits,
        audit_evidence_complete: audit.evidence_complete,
        evidence_complete,
        issue_counts: issues,
        candidate_count: items.len(),
        candidate_bytes,
        fully_validated_file_count,
        fully_validated_file_bytes,
        partially_validated_file_count,
        validated_recoverable_bytes,
        invalid_count,
        limit_exceeded_count,
        unsupported_count,
        skipped_count,
        validation_fingerprint: fingerprint,
        mutation_performed: false,
        items,
    })
}

pub fn summarize_incomplete_download_recovery(
    report: &IncompleteDownloadRecoveryReport,
) -> IncompleteDownloadRecoverySummary {
    IncompleteDownloadRecoverySummary {
        schema_version: report.schema_version,
        output_mode: "incomplete-download-recovery-validation-summary".into(),
        observed_at_ms: report.observed_at_ms,
        source_scope_fingerprint: report.source_scope_fingerprint.clone(),
        audit_fingerprint: report.audit_fingerprint.clone(),
        limits: report.limits,
        audit_evidence_complete: report.audit_evidence_complete,
        evidence_complete: report.evidence_complete,
        issue_counts: report.issue_counts.clone(),
        candidate_count: report.candidate_count,
        candidate_bytes: report.candidate_bytes,
        fully_validated_file_count: report.fully_validated_file_count,
        fully_validated_file_bytes: report.fully_validated_file_bytes,
        partially_validated_file_count: report.partially_validated_file_count,
        validated_recoverable_bytes: report.validated_recoverable_bytes,
        invalid_count: report.invalid_count,
        limit_exceeded_count: report.limit_exceeded_count,
        unsupported_count: report.unsupported_count,
        skipped_count: report.skipped_count,
        validation_fingerprint: report.validation_fingerprint.clone(),
        mutation_performed: false,
        human_recovery_action_required: report.candidate_count > 0,
        automatic_rename_allowed: false,
        automatic_discard_allowed: false,
        notices: vec![
            "read-only-no-extraction-no-rename-no-discard".into(),
            "zip-entries-are-read-to-eof-with-default-crc-validation".into(),
            "png-decode-output-is-memory-bounded".into(),
            "successful-validation-does-not-authorize-automatic-rename".into(),
            "successful-validation-does-not-authorize-discard".into(),
            "fresh-audit-and-human-recovery-destination-review-required".into(),
            "download-acquisition-and-filesystem-times-are-not-production-time".into(),
            "filename-date-is-not-production-evidence".into(),
        ],
        redacted_from_summary: vec![
            "absolute-source-root".into(),
            "relative-file-path".into(),
            "zip-range-offsets".into(),
            "entry-paths".into(),
            "active-process-identifiers".into(),
        ],
        items: report
            .items
            .iter()
            .map(|item| IncompleteDownloadRecoveryItemSummary {
                candidate_fingerprint: item.candidate_fingerprint.clone(),
                logical_bytes: item.logical_bytes,
                state: item.state,
                detected_mime_type: item.detected_mime_type.clone(),
                status: item.status,
                evidence_complete: item.evidence_complete,
                validation_stable: item.validation_stable,
                validated_recoverable_bytes: item.validated_recoverable_bytes,
                fully_validated_file: item.fully_validated_file,
                validations: item
                    .validations
                    .iter()
                    .map(|validation| ContentValidationSummary {
                        kind: validation.kind,
                        status: validation.status,
                        span_bytes: validation.span_bytes,
                        entry_count: validation.entry_count,
                        declared_uncompressed_bytes: validation.declared_uncompressed_bytes,
                        validated_uncompressed_bytes: validation.validated_uncompressed_bytes,
                        decoded_frame_count: validation.decoded_frame_count,
                        decoded_output_bytes: validation.decoded_output_bytes,
                        reason_code: validation.reason_code.clone(),
                    })
                    .collect(),
                reason_codes: item.reason_codes.clone(),
                requires_human_recovery_action: item.requires_human_recovery_action,
                automatic_rename_allowed: false,
                automatic_discard_allowed: false,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_zip(path: &Path, prefix: &[u8], payload: &[u8]) -> (u64, u64) {
        let mut bytes = Vec::new();
        {
            let cursor = std::io::Cursor::new(&mut bytes);
            let mut writer = zip::ZipWriter::new(cursor);
            writer
                .start_file(
                    "payload.bin",
                    zip::write::SimpleFileOptions::default()
                        .compression_method(zip::CompressionMethod::Stored),
                )
                .unwrap();
            writer.write_all(payload).unwrap();
            writer.finish().unwrap();
        }
        let start = prefix.len() as u64;
        let end = start + bytes.len() as u64;
        let mut output = File::create(path).unwrap();
        output.write_all(prefix).unwrap();
        output.write_all(&bytes).unwrap();
        output.write_all(b"suffix").unwrap();
        (start, end)
    }

    #[test]
    fn bounded_zip_range_reads_all_entries_and_checks_crc() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("range.bin");
        let (start, end) = write_zip(&path, b"prefix", b"payload");
        let validation = validate_zip_range(
            &path,
            start,
            end,
            1,
            std::fs::metadata(&path).unwrap().len(),
            RecoveryValidationLimits::default(),
        );
        assert_eq!(validation.status, ContentValidationStatus::Validated);
        assert_eq!(validation.kind, RecoveryValidationKind::ZipEmbeddedRange);
        assert_eq!(validation.entry_count, 1);
        assert_eq!(validation.validated_uncompressed_bytes, 7);
    }

    #[test]
    fn corrupted_zip_payload_fails_crc_validation() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("archive.zip");
        let (start, end) = write_zip(&path, b"", b"payload");
        let mut bytes = std::fs::read(&path).unwrap();
        let name_len = u16::from_le_bytes([bytes[26], bytes[27]]) as usize;
        let extra_len = u16::from_le_bytes([bytes[28], bytes[29]]) as usize;
        let payload_offset = 30 + name_len + extra_len;
        bytes[payload_offset] ^= 0xff;
        std::fs::write(&path, bytes).unwrap();
        let validation = validate_zip_range(
            &path,
            start,
            end,
            1,
            end,
            RecoveryValidationLimits::default(),
        );
        assert_eq!(validation.status, ContentValidationStatus::Invalid);
        assert_eq!(validation.reason_code, "zip-entry-read-or-crc-failed");
    }

    #[test]
    fn zip_declared_output_limit_is_fail_closed() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("archive.zip");
        let (_, end) = write_zip(&path, b"", b"payload");
        let validation = validate_zip_range(
            &path,
            0,
            end,
            1,
            end,
            RecoveryValidationLimits {
                max_zip_total_uncompressed_bytes: 4,
                max_zip_single_uncompressed_bytes: 4,
                ..RecoveryValidationLimits::default()
            },
        );
        assert_eq!(validation.status, ContentValidationStatus::LimitExceeded);
    }

    #[test]
    fn png_decode_is_bounded_and_validates_all_declared_frames() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("image.png");
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&[1, 2, 3, 255]).unwrap();
        }
        std::fs::write(&path, &bytes).unwrap();
        let validation = validate_png(&path, bytes.len() as u64, 1024);
        assert_eq!(validation.status, ContentValidationStatus::Validated);
        assert_eq!(validation.decoded_frame_count, 1);
        assert_eq!(validation.decoded_output_bytes, 4);
        let limited = validate_png(&path, bytes.len() as u64, 3);
        assert_eq!(limited.status, ContentValidationStatus::LimitExceeded);
    }

    #[test]
    fn public_summary_redacts_paths_and_zip_offsets() {
        let report = IncompleteDownloadRecoveryReport {
            schema_version: 1,
            observed_at_ms: 1,
            source_root: "/private/downloads".into(),
            source_scope_fingerprint: "scope".into(),
            audit_fingerprint: "audit".into(),
            limits: RecoveryValidationLimits::default(),
            audit_evidence_complete: true,
            evidence_complete: true,
            issue_counts: BTreeMap::new(),
            candidate_count: 1,
            candidate_bytes: 10,
            fully_validated_file_count: 0,
            fully_validated_file_bytes: 0,
            partially_validated_file_count: 1,
            validated_recoverable_bytes: 5,
            invalid_count: 0,
            limit_exceeded_count: 0,
            unsupported_count: 0,
            skipped_count: 0,
            validation_fingerprint: "validation".into(),
            mutation_performed: false,
            items: vec![IncompleteDownloadRecoveryItem {
                candidate_fingerprint: "candidate".into(),
                relative_path: "private/client.crdownload".into(),
                logical_bytes: 10,
                state: IncompleteDownloadState::StaleIdleRecoveryCandidate,
                detected_mime_type: None,
                status: RecoveryItemStatus::PartiallyValidated,
                evidence_complete: true,
                validation_stable: true,
                validated_recoverable_bytes: 5,
                fully_validated_file: false,
                validations: vec![content_validation(
                    RecoveryValidationKind::ZipEmbeddedRange,
                    ContentValidationStatus::Validated,
                    2,
                    7,
                    "zip-all-entries-read-to-eof-with-crc",
                )],
                reason_codes: vec!["zip-all-entries-read-to-eof-with-crc".into()],
                requires_human_recovery_action: true,
                automatic_rename_allowed: false,
                automatic_discard_allowed: false,
            }],
        };
        let encoded =
            serde_json::to_string(&summarize_incomplete_download_recovery(&report)).unwrap();
        for sensitive in [
            "/private",
            "downloads",
            "client",
            "range_start",
            "range_end",
        ] {
            assert!(!encoded.contains(sensitive), "{sensitive}");
        }
        assert!(encoded.contains("candidate"));
        assert!(encoded.contains("\"span_bytes\":5"));
    }
}
