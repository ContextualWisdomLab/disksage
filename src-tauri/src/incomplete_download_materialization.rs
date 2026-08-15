//! Read-only lineage planning for materializing validated incomplete-download content.
//!
//! A plan binds fresh audit and recovery fingerprints to exact source byte ranges and content
//! digests. It never selects a destination, writes output, renames a source, or authorizes discard.

use crate::cloud_local_eviction::observe_path_active_use;
use crate::content_digest::{ContentDigests, ContentHasher};
use crate::incomplete_download::{
    incomplete_download_audit_integrity_valid, IncompleteDownloadAuditItem,
    IncompleteDownloadAuditReport,
};
use crate::incomplete_download_recovery::{
    incomplete_download_recovery_integrity_valid, ContentValidationStatus,
    IncompleteDownloadRecoveryReport, RecoveryItemStatus, RecoveryValidationKind,
};
use std::collections::BTreeSet;
use std::fs::{File, Metadata};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};

pub const INCOMPLETE_DOWNLOAD_MATERIALIZATION_VERSION: u32 = 1;
const IO_BUFFER_BYTES: usize = 64 * 1024;

#[cfg(all(test, not(coverage)))]
pub(crate) static MATERIALIZATION_ACTIVE_USE_TEST_LOCK: std::sync::Mutex<()> =
    std::sync::Mutex::new(());

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "kebab-case")]
pub enum MaterializationUnitKind {
    FullPngFile,
    FullZipFile,
    EmbeddedZipRange,
}

impl MaterializationUnitKind {
    fn token(self) -> &'static str {
        match self {
            Self::FullPngFile => "full-png-file",
            Self::FullZipFile => "full-zip-file",
            Self::EmbeddedZipRange => "embedded-zip-range",
        }
    }

    fn output(self) -> (&'static str, &'static str) {
        match self {
            Self::FullPngFile => ("image/png", "png"),
            Self::FullZipFile | Self::EmbeddedZipRange => ("application/zip", "zip"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncompleteDownloadMaterializationUnit {
    pub candidate_fingerprint: String,
    pub source_relative_path: String,
    pub source_logical_bytes: u64,
    pub source_filesystem_created_ms: u64,
    pub source_filesystem_modified_ms: u64,
    pub kind: MaterializationUnitKind,
    pub range_start: u64,
    pub range_end: u64,
    pub output_bytes: u64,
    pub output_mime_type: String,
    pub output_extension: String,
    pub content_digests: ContentDigests,
    pub unit_fingerprint: String,
    pub suggested_filename: String,
    pub active_use_evidence_complete: bool,
    pub source_active: bool,
    pub source_stable: bool,
    pub destination_selected: bool,
    pub requires_human_destination_review: bool,
    pub approval_issued: bool,
    pub write_performed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncompleteDownloadMaterializationReport {
    pub schema_version: u32,
    pub observed_at_ms: u64,
    pub source_root: String,
    pub source_scope_fingerprint: String,
    pub audit_fingerprint: String,
    pub validation_fingerprint: String,
    pub evidence_complete: bool,
    pub source_file_count: usize,
    pub unit_count: usize,
    pub full_file_unit_count: usize,
    pub embedded_zip_range_unit_count: usize,
    pub planned_output_bytes: u64,
    pub plan_fingerprint: String,
    pub destination_selected: bool,
    pub requires_human_destination_review: bool,
    pub exact_materialization_approval_available: bool,
    pub approval_issued: bool,
    pub mutation_performed: bool,
    pub units: Vec<IncompleteDownloadMaterializationUnit>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncompleteDownloadMaterializationSummary {
    pub schema_version: u32,
    pub output_mode: String,
    pub observed_at_ms: u64,
    pub source_scope_fingerprint: String,
    pub audit_fingerprint: String,
    pub validation_fingerprint: String,
    pub evidence_complete: bool,
    pub source_file_count: usize,
    pub unit_count: usize,
    pub full_file_unit_count: usize,
    pub embedded_zip_range_unit_count: usize,
    pub planned_output_bytes: u64,
    pub plan_fingerprint: String,
    pub destination_selected: bool,
    pub requires_human_destination_review: bool,
    pub exact_materialization_approval_available: bool,
    pub approval_issued: bool,
    pub mutation_performed: bool,
    pub production_time_assigned: bool,
    pub filename_date_used_as_production_time: bool,
    pub filesystem_times_used_only_for_source_stability: bool,
    pub content_digest_algorithms: Vec<String>,
    pub notices: Vec<String>,
    pub redacted_from_summary: Vec<String>,
}

fn valid_hex64(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn system_time_ms(value: std::io::Result<std::time::SystemTime>) -> u64 {
    value
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .unwrap_or_default()
}

fn source_metadata_matches(metadata: &Metadata, item: &IncompleteDownloadAuditItem) -> bool {
    metadata.is_file()
        && !metadata.file_type().is_symlink()
        && metadata.len() == item.logical_bytes
        && system_time_ms(metadata.created()) == item.filesystem_created_ms
        && system_time_ms(metadata.modified()) == item.filesystem_modified_ms
}

fn safe_candidate_path(canonical_root: &Path, relative_path: &str) -> Result<PathBuf, String> {
    let relative = Path::new(relative_path);
    if relative.as_os_str().is_empty()
        || relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err("materialization-source-relative-path-unsafe".into());
    }
    let mut current = canonical_root.to_path_buf();
    for component in relative.components() {
        if let Component::Normal(value) = component {
            current.push(value);
            let metadata = std::fs::symlink_metadata(&current)
                .map_err(|_| "materialization-source-component-unavailable".to_string())?;
            if metadata.file_type().is_symlink() {
                return Err("materialization-source-symlink-component".into());
            }
        }
    }
    let canonical = std::fs::canonicalize(&current)
        .map_err(|_| "materialization-source-unavailable".to_string())?;
    if !canonical.starts_with(canonical_root) {
        return Err("materialization-source-outside-root".into());
    }
    Ok(canonical)
}

fn digest_range(path: &Path, start: u64, end: u64) -> Result<ContentDigests, String> {
    let span = end
        .checked_sub(start)
        .filter(|span| *span > 0)
        .ok_or_else(|| "materialization-range-invalid".to_string())?;
    let mut file =
        File::open(path).map_err(|_| "materialization-source-open-failed".to_string())?;
    let file_len = file
        .metadata()
        .map_err(|_| "materialization-source-metadata-failed".to_string())?
        .len();
    if end > file_len {
        return Err("materialization-range-outside-source".into());
    }
    file.seek(SeekFrom::Start(start))
        .map_err(|_| "materialization-source-seek-failed".to_string())?;
    let mut remaining = span;
    let mut buffer = vec![0u8; IO_BUFFER_BYTES];
    let mut hasher = ContentHasher::default();
    while remaining > 0 {
        let bounded = usize::try_from(remaining)
            .unwrap_or(usize::MAX)
            .min(buffer.len());
        let read = file
            .read(&mut buffer[..bounded])
            .map_err(|_| "materialization-source-read-failed".to_string())?;
        if read == 0 {
            return Err("materialization-source-ended-before-range".into());
        }
        hasher.update(&buffer[..read]);
        remaining -= read as u64;
    }
    Ok(hasher.finalize())
}

#[allow(clippy::too_many_arguments)]
fn unit_fingerprint(
    candidate_fingerprint: &str,
    source_relative_path: &str,
    source_logical_bytes: u64,
    source_filesystem_created_ms: u64,
    source_filesystem_modified_ms: u64,
    kind: MaterializationUnitKind,
    range_start: u64,
    range_end: u64,
    digests: &ContentDigests,
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-incomplete-download-materialization-unit-v1\0");
    for value in [
        candidate_fingerprint,
        source_relative_path,
        kind.token(),
        digests.blake3.as_str(),
        digests.sha256.as_str(),
        digests.quick_xor_base64.as_str(),
    ] {
        hasher.update(value.as_bytes());
        hasher.update(&[0]);
    }
    for value in [
        source_logical_bytes,
        source_filesystem_created_ms,
        source_filesystem_modified_ms,
        range_start,
        range_end,
    ] {
        hasher.update(&value.to_le_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

fn build_unit(
    path: &Path,
    audit_item: &IncompleteDownloadAuditItem,
    validation_kind: RecoveryValidationKind,
    range_start: u64,
    range_end: u64,
) -> Result<IncompleteDownloadMaterializationUnit, String> {
    let kind = match validation_kind {
        RecoveryValidationKind::PngFullFile
            if range_start == 0 && range_end == audit_item.logical_bytes =>
        {
            MaterializationUnitKind::FullPngFile
        }
        RecoveryValidationKind::ZipWholeFile
            if range_start == 0 && range_end == audit_item.logical_bytes =>
        {
            MaterializationUnitKind::FullZipFile
        }
        RecoveryValidationKind::ZipEmbeddedRange
            if !(range_start == 0 && range_end == audit_item.logical_bytes) =>
        {
            MaterializationUnitKind::EmbeddedZipRange
        }
        _ => return Err("materialization-validation-kind-range-mismatch".into()),
    };
    let output_bytes = range_end
        .checked_sub(range_start)
        .filter(|span| *span > 0)
        .ok_or_else(|| "materialization-range-invalid".to_string())?;
    let content_digests = digest_range(path, range_start, range_end)?;
    if !valid_hex64(&content_digests.blake3) || !valid_hex64(&content_digests.sha256) {
        return Err("materialization-content-digest-invalid".into());
    }
    let unit_fingerprint = unit_fingerprint(
        &audit_item.candidate_fingerprint,
        &audit_item.relative_path,
        audit_item.logical_bytes,
        audit_item.filesystem_created_ms,
        audit_item.filesystem_modified_ms,
        kind,
        range_start,
        range_end,
        &content_digests,
    );
    let (output_mime_type, output_extension) = kind.output();
    let suggested_filename = format!(
        "recovered-{}-{}.{}",
        &content_digests.sha256[..12],
        &unit_fingerprint[..12],
        output_extension
    );
    Ok(IncompleteDownloadMaterializationUnit {
        candidate_fingerprint: audit_item.candidate_fingerprint.clone(),
        source_relative_path: audit_item.relative_path.clone(),
        source_logical_bytes: audit_item.logical_bytes,
        source_filesystem_created_ms: audit_item.filesystem_created_ms,
        source_filesystem_modified_ms: audit_item.filesystem_modified_ms,
        kind,
        range_start,
        range_end,
        output_bytes,
        output_mime_type: output_mime_type.into(),
        output_extension: output_extension.into(),
        content_digests,
        unit_fingerprint,
        suggested_filename,
        active_use_evidence_complete: true,
        source_active: false,
        source_stable: true,
        destination_selected: false,
        requires_human_destination_review: true,
        approval_issued: false,
        write_performed: false,
    })
}

fn plan_fingerprint(
    source_scope_fingerprint: &str,
    audit_fingerprint: &str,
    validation_fingerprint: &str,
    evidence_complete: bool,
    units: &[IncompleteDownloadMaterializationUnit],
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-incomplete-download-materialization-plan-v1\0");
    for value in [
        source_scope_fingerprint,
        audit_fingerprint,
        validation_fingerprint,
    ] {
        hasher.update(value.as_bytes());
        hasher.update(&[0]);
    }
    hasher.update(&[u8::from(evidence_complete)]);
    for unit in units {
        hasher.update(unit.unit_fingerprint.as_bytes());
        hasher.update(&[0]);
    }
    hasher.finalize().to_hex().to_string()
}

/// Hash every validated, non-overlapping recoverable range from a fresh audit and recovery report.
///
/// This is a destination-independent planning step. It does not create a directory or file, does
/// not rename or discard a source, and deliberately cannot issue an execution approval.
#[cfg(not(coverage))]
pub fn plan_incomplete_download_materialization(
    source_root: &Path,
    audit: &IncompleteDownloadAuditReport,
    recovery: &IncompleteDownloadRecoveryReport,
    observed_at_ms: u64,
) -> Result<IncompleteDownloadMaterializationReport, String> {
    if !source_root.is_absolute() {
        return Err("materialization-root-must-be-absolute".into());
    }
    let supplied_root_metadata = std::fs::symlink_metadata(source_root)
        .map_err(|_| "materialization-root-unavailable".to_string())?;
    if !supplied_root_metadata.is_dir() || supplied_root_metadata.file_type().is_symlink() {
        return Err("materialization-root-unsafe".into());
    }
    let canonical_root = std::fs::canonicalize(source_root)
        .map_err(|_| "materialization-root-unavailable".to_string())?;
    let root_metadata = std::fs::symlink_metadata(&canonical_root)
        .map_err(|_| "materialization-root-unavailable".to_string())?;
    if !root_metadata.is_dir() || root_metadata.file_type().is_symlink() {
        return Err("materialization-root-unsafe".into());
    }
    if audit.source_root != canonical_root.to_string_lossy() {
        return Err("materialization-audit-root-mismatch".into());
    }
    if !incomplete_download_audit_integrity_valid(audit) {
        return Err("materialization-audit-integrity-invalid".into());
    }
    if !incomplete_download_recovery_integrity_valid(audit, recovery) {
        return Err("materialization-recovery-integrity-invalid".into());
    }
    if ![
        audit.source_scope_fingerprint.as_str(),
        audit.audit_fingerprint.as_str(),
        recovery.validation_fingerprint.as_str(),
    ]
    .iter()
    .all(|value| valid_hex64(value))
    {
        return Err("materialization-lineage-fingerprint-invalid".into());
    }
    if !audit.evidence_complete
        || !recovery.evidence_complete
        || recovery.invalid_count > 0
        || recovery.limit_exceeded_count > 0
        || recovery.unsupported_count > 0
        || recovery.skipped_count > 0
        || recovery.items.iter().any(|item| {
            !matches!(
                item.status,
                RecoveryItemStatus::FullyValidated | RecoveryItemStatus::PartiallyValidated
            )
        })
    {
        return Err("materialization-recovery-evidence-incomplete".into());
    }

    let mut units = Vec::new();
    let mut source_files = BTreeSet::new();
    for recovery_item in &recovery.items {
        let audit_item = audit
            .items
            .iter()
            .find(|item| item.candidate_fingerprint == recovery_item.candidate_fingerprint)
            .ok_or_else(|| "materialization-audit-item-missing".to_string())?;
        if !audit_item.evidence_complete
            || !audit_item.active_use.evidence_complete
            || audit_item.active_use.active
        {
            return Err("materialization-audit-item-not-idle".into());
        }
        let path = safe_candidate_path(&canonical_root, &audit_item.relative_path)?;
        let before = std::fs::symlink_metadata(&path)
            .map_err(|_| "materialization-source-metadata-failed".to_string())?;
        if !source_metadata_matches(&before, audit_item) {
            return Err("materialization-source-changed-since-audit".into());
        }
        let active_before = observe_path_active_use(&path);
        if !active_before.evidence_complete {
            return Err("materialization-pre-hash-active-use-evidence-incomplete".into());
        }
        if active_before.active {
            return Err("materialization-source-active-before-hash".into());
        }

        let mut validations = recovery_item
            .validations
            .iter()
            .filter(|validation| validation.status == ContentValidationStatus::Validated)
            .collect::<Vec<_>>();
        validations.sort_by_key(|validation| (validation.range_start, validation.range_end));
        if validations.is_empty()
            || validations.iter().any(|validation| {
                validation.range_start >= validation.range_end
                    || validation.range_end > audit_item.logical_bytes
                    || validation.span_bytes != validation.range_end - validation.range_start
            })
            || validations
                .windows(2)
                .any(|ranges| ranges[1].range_start < ranges[0].range_end)
        {
            return Err("materialization-validated-ranges-invalid-or-overlapping".into());
        }
        let item_output_bytes = validations.iter().fold(0u64, |total, validation| {
            total.saturating_add(validation.span_bytes)
        });
        if item_output_bytes != recovery_item.validated_recoverable_bytes {
            return Err("materialization-recoverable-byte-total-mismatch".into());
        }
        for validation in validations {
            units.push(build_unit(
                &path,
                audit_item,
                validation.kind,
                validation.range_start,
                validation.range_end,
            )?);
        }

        let active_after = observe_path_active_use(&path);
        if !active_after.evidence_complete {
            return Err("materialization-post-hash-active-use-evidence-incomplete".into());
        }
        if active_after.active {
            return Err("materialization-source-active-after-hash".into());
        }
        let after = std::fs::symlink_metadata(&path)
            .map_err(|_| "materialization-post-hash-metadata-failed".to_string())?;
        if !source_metadata_matches(&after, audit_item)
            || after.modified().ok() != before.modified().ok()
        {
            return Err("materialization-source-changed-during-hash".into());
        }
        source_files.insert(audit_item.candidate_fingerprint.clone());
    }

    units.sort_by(|left, right| {
        (
            &left.candidate_fingerprint,
            left.range_start,
            left.range_end,
            left.kind,
        )
            .cmp(&(
                &right.candidate_fingerprint,
                right.range_start,
                right.range_end,
                right.kind,
            ))
    });
    if units.is_empty()
        || units
            .windows(2)
            .any(|items| items[0].unit_fingerprint == items[1].unit_fingerprint)
    {
        return Err("materialization-unit-set-empty-or-duplicate".into());
    }
    let planned_output_bytes = units
        .iter()
        .fold(0u64, |total, unit| total.saturating_add(unit.output_bytes));
    if planned_output_bytes != recovery.validated_recoverable_bytes {
        return Err("materialization-plan-byte-total-mismatch".into());
    }
    let full_file_unit_count = units
        .iter()
        .filter(|unit| {
            matches!(
                unit.kind,
                MaterializationUnitKind::FullPngFile | MaterializationUnitKind::FullZipFile
            )
        })
        .count();
    let embedded_zip_range_unit_count = units
        .iter()
        .filter(|unit| unit.kind == MaterializationUnitKind::EmbeddedZipRange)
        .count();
    let evidence_complete = true;
    let fingerprint = plan_fingerprint(
        &audit.source_scope_fingerprint,
        &audit.audit_fingerprint,
        &recovery.validation_fingerprint,
        evidence_complete,
        &units,
    );

    Ok(IncompleteDownloadMaterializationReport {
        schema_version: INCOMPLETE_DOWNLOAD_MATERIALIZATION_VERSION,
        observed_at_ms,
        source_root: canonical_root.to_string_lossy().into_owned(),
        source_scope_fingerprint: audit.source_scope_fingerprint.clone(),
        audit_fingerprint: audit.audit_fingerprint.clone(),
        validation_fingerprint: recovery.validation_fingerprint.clone(),
        evidence_complete,
        source_file_count: source_files.len(),
        unit_count: units.len(),
        full_file_unit_count,
        embedded_zip_range_unit_count,
        planned_output_bytes,
        plan_fingerprint: fingerprint,
        destination_selected: false,
        requires_human_destination_review: true,
        exact_materialization_approval_available: false,
        approval_issued: false,
        mutation_performed: false,
        units,
    })
}

pub fn summarize_incomplete_download_materialization(
    report: &IncompleteDownloadMaterializationReport,
) -> IncompleteDownloadMaterializationSummary {
    IncompleteDownloadMaterializationSummary {
        schema_version: report.schema_version,
        output_mode: "incomplete-download-materialization-plan-summary".into(),
        observed_at_ms: report.observed_at_ms,
        source_scope_fingerprint: report.source_scope_fingerprint.clone(),
        audit_fingerprint: report.audit_fingerprint.clone(),
        validation_fingerprint: report.validation_fingerprint.clone(),
        evidence_complete: report.evidence_complete,
        source_file_count: report.source_file_count,
        unit_count: report.unit_count,
        full_file_unit_count: report.full_file_unit_count,
        embedded_zip_range_unit_count: report.embedded_zip_range_unit_count,
        planned_output_bytes: report.planned_output_bytes,
        plan_fingerprint: report.plan_fingerprint.clone(),
        destination_selected: false,
        requires_human_destination_review: true,
        exact_materialization_approval_available: false,
        approval_issued: false,
        mutation_performed: false,
        production_time_assigned: false,
        filename_date_used_as_production_time: false,
        filesystem_times_used_only_for_source_stability: true,
        content_digest_algorithms: vec!["blake3".into(), "sha256".into(), "quickxor".into()],
        notices: vec![
            "read-only-no-output-created".into(),
            "fresh-audit-recovery-and-content-digest-bound".into(),
            "validated-ranges-must-be-non-overlapping".into(),
            "suggested-filenames-are-content-addressed-not-production-dated".into(),
            "destination-selection-and-human-review-required".into(),
            "no-exact-materialization-approval-until-destination-is-bound".into(),
            "source-rename-discard-and-cloud-write-not-authorized".into(),
        ],
        redacted_from_summary: vec![
            "absolute-source-root".into(),
            "relative-source-path".into(),
            "range-offsets".into(),
            "content-digests".into(),
            "suggested-filenames".into(),
            "active-process-identifiers".into(),
        ],
    }
}

fn valid_quick_xor_base64(value: &str) -> bool {
    value.len() == 28
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'='))
}

pub(crate) fn incomplete_download_materialization_integrity_valid(
    report: &IncompleteDownloadMaterializationReport,
) -> bool {
    if report.schema_version != INCOMPLETE_DOWNLOAD_MATERIALIZATION_VERSION
        || !report.evidence_complete
        || report.destination_selected
        || !report.requires_human_destination_review
        || report.exact_materialization_approval_available
        || report.approval_issued
        || report.mutation_performed
        || ![
            report.source_scope_fingerprint.as_str(),
            report.audit_fingerprint.as_str(),
            report.validation_fingerprint.as_str(),
            report.plan_fingerprint.as_str(),
        ]
        .iter()
        .all(|value| valid_hex64(value))
        || report.unit_count != report.units.len()
        || report.units.is_empty()
    {
        return false;
    }

    let mut source_files = BTreeSet::new();
    let mut unit_fingerprints = BTreeSet::new();
    let mut suggested_filenames = BTreeSet::new();
    let mut previous: Option<&IncompleteDownloadMaterializationUnit> = None;
    for unit in &report.units {
        if let Some(left) = previous {
            let left_key = (
                &left.candidate_fingerprint,
                left.range_start,
                left.range_end,
                left.kind,
            );
            let right_key = (
                &unit.candidate_fingerprint,
                unit.range_start,
                unit.range_end,
                unit.kind,
            );
            if left_key >= right_key
                || (left.candidate_fingerprint == unit.candidate_fingerprint
                    && unit.range_start < left.range_end)
            {
                return false;
            }
        }
        previous = Some(unit);

        let relative_source = Path::new(&unit.source_relative_path);
        let (expected_mime, expected_extension) = unit.kind.output();
        let range_matches_kind = match unit.kind {
            MaterializationUnitKind::FullPngFile | MaterializationUnitKind::FullZipFile => {
                unit.range_start == 0 && unit.range_end == unit.source_logical_bytes
            }
            MaterializationUnitKind::EmbeddedZipRange => {
                !(unit.range_start == 0 && unit.range_end == unit.source_logical_bytes)
            }
        };
        if !valid_hex64(&unit.candidate_fingerprint)
            || relative_source.as_os_str().is_empty()
            || relative_source.is_absolute()
            || relative_source
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
            || !valid_hex64(&unit.content_digests.blake3)
            || !valid_hex64(&unit.content_digests.sha256)
            || !valid_quick_xor_base64(&unit.content_digests.quick_xor_base64)
            || unit.range_end.checked_sub(unit.range_start) != Some(unit.output_bytes)
            || unit.output_bytes == 0
            || unit.range_end > unit.source_logical_bytes
            || !range_matches_kind
            || unit.output_mime_type != expected_mime
            || unit.output_extension != expected_extension
            || !unit.active_use_evidence_complete
            || unit.source_active
            || !unit.source_stable
            || unit.destination_selected
            || !unit.requires_human_destination_review
            || unit.approval_issued
            || unit.write_performed
        {
            return false;
        }
        let expected_fingerprint = unit_fingerprint(
            &unit.candidate_fingerprint,
            &unit.source_relative_path,
            unit.source_logical_bytes,
            unit.source_filesystem_created_ms,
            unit.source_filesystem_modified_ms,
            unit.kind,
            unit.range_start,
            unit.range_end,
            &unit.content_digests,
        );
        let expected_name = format!(
            "recovered-{}-{}.{}",
            &unit.content_digests.sha256[..12],
            &expected_fingerprint[..12],
            expected_extension
        );
        if unit.unit_fingerprint != expected_fingerprint
            || unit.suggested_filename != expected_name
            || !unit_fingerprints.insert(unit.unit_fingerprint.as_str())
            || !suggested_filenames.insert(unit.suggested_filename.as_str())
        {
            return false;
        }
        source_files.insert(unit.candidate_fingerprint.as_str());
    }

    report.source_file_count == source_files.len()
        && report.full_file_unit_count
            == report
                .units
                .iter()
                .filter(|unit| {
                    matches!(
                        unit.kind,
                        MaterializationUnitKind::FullPngFile | MaterializationUnitKind::FullZipFile
                    )
                })
                .count()
        && report.embedded_zip_range_unit_count
            == report
                .units
                .iter()
                .filter(|unit| unit.kind == MaterializationUnitKind::EmbeddedZipRange)
                .count()
        && report.planned_output_bytes
            == report
                .units
                .iter()
                .fold(0u64, |total, unit| total.saturating_add(unit.output_bytes))
        && plan_fingerprint(
            &report.source_scope_fingerprint,
            &report.audit_fingerprint,
            &report.validation_fingerprint,
            report.evidence_complete,
            &report.units,
        ) == report.plan_fingerprint
}

#[cfg(all(test, not(coverage)))]
mod tests {
    use super::*;
    use crate::incomplete_download::{
        collect_incomplete_download_audit, DEFAULT_MAX_ENTRIES, DEFAULT_STALE_AFTER_DAYS,
    };
    use crate::incomplete_download_recovery::{
        validate_incomplete_download_recovery, RecoveryValidationLimits,
    };
    use std::io::Write;

    fn write_zip(path: &Path, payload: &[u8]) -> Vec<u8> {
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
        std::fs::write(path, &bytes).unwrap();
        bytes
    }

    fn fresh_reports(
        root: &Path,
    ) -> (
        IncompleteDownloadAuditReport,
        IncompleteDownloadRecoveryReport,
    ) {
        let observed_at_ms =
            system_time_ms(std::fs::metadata(root).unwrap().modified()) + 31 * 86_400_000;
        let audit = collect_incomplete_download_audit(
            root,
            observed_at_ms,
            DEFAULT_MAX_ENTRIES,
            DEFAULT_STALE_AFTER_DAYS,
        )
        .unwrap();
        let recovery = validate_incomplete_download_recovery(
            root,
            &audit,
            observed_at_ms + 1,
            RecoveryValidationLimits::default(),
        )
        .unwrap();
        (audit, recovery)
    }

    #[test]
    fn plans_content_addressed_full_zip_without_writes() {
        let _active_use_guard = MATERIALIZATION_ACTIVE_USE_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("whole.zip.crdownload");
        let bytes = write_zip(&path, b"validated payload");
        let (audit, recovery) = fresh_reports(temp.path());

        let report = plan_incomplete_download_materialization(
            temp.path(),
            &audit,
            &recovery,
            recovery.observed_at_ms + 1,
        )
        .unwrap();
        assert_eq!(report.source_file_count, 1);
        assert_eq!(report.unit_count, 1);
        assert_eq!(report.full_file_unit_count, 1);
        assert_eq!(report.embedded_zip_range_unit_count, 0);
        assert_eq!(report.planned_output_bytes, bytes.len() as u64);
        assert_eq!(report.units[0].kind, MaterializationUnitKind::FullZipFile);
        assert_eq!(
            report.units[0].content_digests,
            crate::content_digest::digest_bytes(&bytes)
        );
        assert!(report.units[0].suggested_filename.ends_with(".zip"));
        assert!(!report.destination_selected);
        assert!(!report.exact_materialization_approval_available);
        assert!(!report.mutation_performed);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);

        let summary = summarize_incomplete_download_materialization(&report);
        let encoded = serde_json::to_string(&summary).unwrap();
        assert!(!encoded.contains("whole.zip.crdownload"));
        assert!(!encoded.contains(&report.units[0].content_digests.sha256));
        assert!(!encoded.contains(&report.units[0].suggested_filename));
        assert!(!summary.production_time_assigned);
        assert!(!summary.filename_date_used_as_production_time);
    }

    #[test]
    fn rejects_tampered_lineage_reports() {
        let _active_use_guard = MATERIALIZATION_ACTIVE_USE_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp = tempfile::tempdir().unwrap();
        write_zip(
            &temp.path().join("tampered.zip.crdownload"),
            b"validated payload",
        );
        let (audit, mut recovery) = fresh_reports(temp.path());
        let replacement = if recovery.validation_fingerprint.starts_with('0') {
            "1"
        } else {
            "0"
        };
        recovery
            .validation_fingerprint
            .replace_range(..1, replacement);

        assert_eq!(
            plan_incomplete_download_materialization(
                temp.path(),
                &audit,
                &recovery,
                recovery.observed_at_ms + 1,
            )
            .unwrap_err(),
            "materialization-recovery-integrity-invalid"
        );
    }

    #[test]
    fn rejects_source_change_after_recovery_validation() {
        let _active_use_guard = MATERIALIZATION_ACTIVE_USE_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("changed.zip.crdownload");
        write_zip(&path, b"validated payload");
        let (audit, recovery) = fresh_reports(temp.path());
        std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"changed")
            .unwrap();

        assert_eq!(
            plan_incomplete_download_materialization(
                temp.path(),
                &audit,
                &recovery,
                recovery.observed_at_ms + 1,
            )
            .unwrap_err(),
            "materialization-source-changed-since-audit"
        );
    }
}
