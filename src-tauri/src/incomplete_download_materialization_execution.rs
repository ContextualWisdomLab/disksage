//! Approval-gated materialization of validated incomplete-download byte ranges.
//!
//! The executor requires an integrity-checked materialization report, a destination plan, an
//! attributed human approval for that exact plan, and fresh provider capacity evidence. Outputs
//! are staged with create-new temporary names, verified against the planned digests, and renamed
//! only after every unit passes. Any failure removes files created by this invocation. Sources are
//! never renamed, discarded, trashed, or deleted.

use crate::cloud::{CloudAccountScope, CloudProvider};
use crate::cloud_local_eviction::observe_path_active_use;
use crate::content_digest::{ContentDigests, ContentHasher};
use crate::incomplete_download_materialization::{
    incomplete_download_materialization_integrity_valid, IncompleteDownloadMaterializationReport,
    IncompleteDownloadMaterializationUnit, MaterializationUnitKind,
};
use crate::incomplete_download_materialization_destination::{
    incomplete_download_destination_plan_integrity_valid,
    validate_incomplete_download_destination_approval, IncompleteDownloadDestinationApproval,
    IncompleteDownloadDestinationPlan, MAX_CAPACITY_AGE_MS,
};
use crate::provider_capacity::{
    assess_capacity, CapacityEvidenceKind, CloudCapacityAssessment, CloudCapacitySnapshot,
    CAPACITY_SCHEMA_VERSION,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, Metadata};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};

pub const INCOMPLETE_DOWNLOAD_EXECUTION_VERSION: u32 = 1;
const IO_BUFFER_BYTES: usize = 1024 * 1024;
const MAX_RECEIPT_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncompleteDownloadMaterializedUnit {
    pub materialization_unit_fingerprint: String,
    pub kind: MaterializationUnitKind,
    pub source_relative_path: String,
    pub range_start: u64,
    pub range_end: u64,
    pub destination_relative_path: String,
    pub output_bytes: u64,
    pub content_digests: ContentDigests,
    pub source_stable: bool,
    pub output_verified: bool,
    pub write_performed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncompleteDownloadMaterializationReceipt {
    pub schema_version: u32,
    pub receipt_id: String,
    pub destination_plan_fingerprint: String,
    pub materialization_plan_fingerprint: String,
    pub approval_id: String,
    pub provider: CloudProvider,
    pub account_scope: CloudAccountScope,
    pub cloud_root: String,
    pub destination_subdirectory: String,
    pub executed_at_ms: u64,
    pub fresh_capacity: CloudCapacityAssessment,
    pub source_file_count: usize,
    pub unit_count: usize,
    pub materialized_bytes: u64,
    pub units: Vec<IncompleteDownloadMaterializedUnit>,
    pub all_outputs_verified: bool,
    pub provider_sync_confirmed: bool,
    pub source_eviction_authorized: bool,
    pub source_mutation_performed: bool,
    pub production_time_ms: Option<u64>,
    pub production_time_source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncompleteDownloadMaterializationReceiptSummary {
    pub schema_version: u32,
    pub output_mode: String,
    pub receipt_id: String,
    pub destination_plan_fingerprint: String,
    pub materialization_plan_fingerprint: String,
    pub approval_id: String,
    pub provider: CloudProvider,
    pub account_scope: CloudAccountScope,
    pub executed_at_ms: u64,
    pub capacity_evidence_kind: CapacityEvidenceKind,
    pub capacity_evidence_fingerprint: Option<String>,
    pub source_file_count: usize,
    pub unit_count: usize,
    pub materialized_bytes: u64,
    pub all_outputs_verified: bool,
    pub provider_sync_confirmed: bool,
    pub source_eviction_authorized: bool,
    pub source_mutation_performed: bool,
    pub production_time_assigned: bool,
    pub filename_date_used_as_production_time: bool,
    pub filesystem_times_used_only_for_source_stability: bool,
    pub paths_names_ranges_and_digests_redacted: bool,
    pub redacted_from_summary: Vec<String>,
}

fn valid_hex64(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_quick_xor_base64(value: &str) -> bool {
    value.len() == 28
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'='))
}

fn system_time_ms(value: std::io::Result<std::time::SystemTime>) -> u64 {
    value
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .unwrap_or_default()
}

fn source_metadata_matches(
    metadata: &Metadata,
    unit: &IncompleteDownloadMaterializationUnit,
) -> bool {
    metadata.is_file()
        && !metadata.file_type().is_symlink()
        && metadata.len() == unit.source_logical_bytes
        && system_time_ms(metadata.created()) == unit.source_filesystem_created_ms
        && system_time_ms(metadata.modified()) == unit.source_filesystem_modified_ms
}

fn safe_relative_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn validate_existing_directory_prefix(root: &Path, relative: &Path) -> Result<(), String> {
    let mut current = root.to_path_buf();
    let mut missing_parent = false;
    for component in relative.components() {
        let Component::Normal(value) = component else {
            return Err("materialization-execution-destination-path-unsafe".into());
        };
        current.push(value);
        if missing_parent {
            continue;
        }
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err("materialization-execution-destination-symlink-component".into())
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Err("materialization-execution-destination-parent-not-directory".into())
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => missing_parent = true,
            Err(_) => return Err("materialization-execution-destination-parent-unavailable".into()),
        }
    }
    Ok(())
}

fn missing_directories(root: &Path, relative: &Path) -> Result<Vec<PathBuf>, String> {
    let mut current = root.to_path_buf();
    let mut missing = Vec::new();
    for component in relative.components() {
        let Component::Normal(value) = component else {
            return Err("materialization-execution-destination-path-unsafe".into());
        };
        current.push(value);
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err("materialization-execution-destination-symlink-component".into())
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Err("materialization-execution-destination-parent-not-directory".into())
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing.push(current.clone())
            }
            Err(_) => return Err("materialization-execution-destination-parent-unavailable".into()),
        }
    }
    Ok(missing)
}

fn hash_file(path: &Path) -> Result<ContentDigests, String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| "materialization-execution-output-metadata-failed".to_string())?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("materialization-execution-output-not-regular".into());
    }
    let mut file =
        File::open(path).map_err(|_| "materialization-execution-output-open-failed".to_string())?;
    let mut hasher = ContentHasher::default();
    let mut buffer = vec![0u8; IO_BUFFER_BYTES];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| "materialization-execution-output-read-failed".to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize())
}

fn remove_file_if_created(path: &Path) {
    if std::fs::symlink_metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
    {
        let _ = std::fs::remove_file(path);
    }
}

fn rollback(created_files: &[PathBuf], created_directories: &[PathBuf]) {
    for path in created_files.iter().rev() {
        remove_file_if_created(path);
    }
    for path in created_directories.iter().rev() {
        let _ = std::fs::remove_dir(path);
    }
}

fn finalize_staging_create_new(staging: &Path, final_path: &Path) -> Result<(), String> {
    std::fs::hard_link(staging, final_path)
        .map_err(|_| "materialization-execution-finalize-create-new-failed".to_string())?;
    if std::fs::remove_file(staging).is_err() {
        remove_file_if_created(final_path);
        return Err("materialization-execution-staging-unlink-failed".into());
    }
    Ok(())
}

fn preflight_capacity(
    plan: &IncompleteDownloadDestinationPlan,
    snapshot: CloudCapacitySnapshot,
    executed_at_ms: u64,
) -> Result<CloudCapacityAssessment, String> {
    if snapshot.schema_version != CAPACITY_SCHEMA_VERSION
        || snapshot.provider != plan.provider
        || snapshot.account_scope != Some(plan.account_scope)
        || snapshot.observed_at_ms == 0
        || snapshot.observed_at_ms > executed_at_ms
        || executed_at_ms.saturating_sub(snapshot.observed_at_ms) > MAX_CAPACITY_AGE_MS
        || !snapshot
            .evidence_fingerprint
            .as_deref()
            .is_some_and(valid_hex64)
    {
        return Err("materialization-execution-capacity-evidence-invalid".into());
    }
    let largest = plan
        .units
        .iter()
        .map(|unit| unit.output_bytes)
        .max()
        .unwrap_or_default();
    let assessment = assess_capacity(
        snapshot,
        plan.planned_output_bytes,
        largest,
        plan.capacity.reserve_bytes,
    );
    if assessment.can_fit != Some(true) || !assessment.blockers.is_empty() {
        return Err("materialization-execution-capacity-insufficient".into());
    }
    Ok(assessment)
}

fn validate_lineage_binding(
    materialization: &IncompleteDownloadMaterializationReport,
    plan: &IncompleteDownloadDestinationPlan,
) -> Result<(), String> {
    if !incomplete_download_materialization_integrity_valid(materialization)
        || !incomplete_download_destination_plan_integrity_valid(plan)
        || materialization.plan_fingerprint != plan.materialization_plan_fingerprint
        || materialization.source_scope_fingerprint != plan.source_scope_fingerprint
        || materialization.audit_fingerprint != plan.audit_fingerprint
        || materialization.validation_fingerprint != plan.validation_fingerprint
        || materialization.source_file_count != plan.source_file_count
        || materialization.unit_count != plan.unit_count
        || materialization.planned_output_bytes != plan.planned_output_bytes
        || materialization.units.len() != plan.units.len()
    {
        return Err("materialization-execution-lineage-mismatch".into());
    }
    for (source, destination) in materialization.units.iter().zip(&plan.units) {
        let destination_relative = Path::new(&destination.destination_relative_path);
        if source.unit_fingerprint != destination.materialization_unit_fingerprint
            || source.kind != destination.kind
            || source.output_bytes != destination.output_bytes
            || destination_relative.file_name()
                != Some(std::ffi::OsStr::new(&source.suggested_filename))
            || destination.destination_exists
            || destination.write_performed
        {
            return Err("materialization-execution-unit-lineage-mismatch".into());
        }
    }
    Ok(())
}

fn receipt_id_for(receipt: &IncompleteDownloadMaterializationReceipt) -> Result<String, String> {
    let mut unsigned = receipt.clone();
    unsigned.receipt_id.clear();
    let encoded = serde_json::to_vec(&unsigned)
        .map_err(|_| "materialization-execution-receipt-json-invalid".to_string())?;
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-incomplete-download-materialization-receipt-v1\0");
    hasher.update(&encoded);
    Ok(hasher.finalize().to_hex().to_string())
}

pub fn incomplete_download_materialization_receipt_integrity_valid(
    receipt: &IncompleteDownloadMaterializationReceipt,
) -> bool {
    if receipt.schema_version != INCOMPLETE_DOWNLOAD_EXECUTION_VERSION
        || ![
            receipt.receipt_id.as_str(),
            receipt.destination_plan_fingerprint.as_str(),
            receipt.materialization_plan_fingerprint.as_str(),
            receipt.approval_id.as_str(),
        ]
        .iter()
        .all(|value| valid_hex64(value))
        || receipt.account_scope == CloudAccountScope::Unknown
        || !Path::new(&receipt.cloud_root).is_absolute()
        || !safe_relative_path(Path::new(&receipt.destination_subdirectory))
        || receipt.source_file_count == 0
        || receipt.unit_count != receipt.units.len()
        || receipt.units.is_empty()
        || receipt.materialized_bytes == 0
        || !receipt.all_outputs_verified
        || receipt.provider_sync_confirmed
        || receipt.source_eviction_authorized
        || receipt.source_mutation_performed
        || receipt.production_time_ms.is_some()
        || receipt.production_time_source.is_some()
        || receipt.fresh_capacity.can_fit != Some(true)
        || !receipt.fresh_capacity.blockers.is_empty()
        || receipt.fresh_capacity.snapshot.provider != receipt.provider
        || receipt.fresh_capacity.snapshot.account_scope != Some(receipt.account_scope)
        || receipt.fresh_capacity.snapshot.observed_at_ms > receipt.executed_at_ms
        || receipt
            .executed_at_ms
            .saturating_sub(receipt.fresh_capacity.snapshot.observed_at_ms)
            > MAX_CAPACITY_AGE_MS
        || !receipt
            .fresh_capacity
            .snapshot
            .evidence_fingerprint
            .as_deref()
            .is_some_and(valid_hex64)
        || receipt.fresh_capacity.requested_bytes != receipt.materialized_bytes
    {
        return false;
    }
    let mut fingerprints = BTreeSet::new();
    let mut sources = BTreeSet::new();
    let mut destinations = BTreeSet::new();
    let mut ranges_by_source: BTreeMap<&str, Vec<(u64, u64)>> = BTreeMap::new();
    let mut total = 0u64;
    let mut largest = 0u64;
    for unit in &receipt.units {
        let destination = Path::new(&unit.destination_relative_path);
        if !valid_hex64(&unit.materialization_unit_fingerprint)
            || !safe_relative_path(Path::new(&unit.source_relative_path))
            || !safe_relative_path(destination)
            || destination.parent() != Some(Path::new(&receipt.destination_subdirectory))
            || unit.range_end <= unit.range_start
            || unit.output_bytes != unit.range_end - unit.range_start
            || !valid_hex64(&unit.content_digests.blake3)
            || !valid_hex64(&unit.content_digests.sha256)
            || !valid_quick_xor_base64(&unit.content_digests.quick_xor_base64)
            || !unit.source_stable
            || !unit.output_verified
            || !unit.write_performed
            || !fingerprints.insert(unit.materialization_unit_fingerprint.as_str())
            || !destinations.insert(unit.destination_relative_path.as_str())
        {
            return false;
        }
        sources.insert(unit.source_relative_path.as_str());
        ranges_by_source
            .entry(unit.source_relative_path.as_str())
            .or_default()
            .push((unit.range_start, unit.range_end));
        total = total.saturating_add(unit.output_bytes);
        largest = largest.max(unit.output_bytes);
    }
    for ranges in ranges_by_source.values_mut() {
        ranges.sort_unstable();
        if ranges.windows(2).any(|pair| pair[1].0 < pair[0].1) {
            return false;
        }
    }
    let expected_capacity = assess_capacity(
        receipt.fresh_capacity.snapshot.clone(),
        receipt.materialized_bytes,
        largest,
        receipt.fresh_capacity.reserve_bytes,
    );
    total == receipt.materialized_bytes
        && sources.len() == receipt.source_file_count
        && expected_capacity == receipt.fresh_capacity
        && receipt_id_for(receipt).is_ok_and(|id| id == receipt.receipt_id)
}

fn write_immutable_receipt(
    receipt: &IncompleteDownloadMaterializationReceipt,
    receipt_dir: &Path,
    source_root: &Path,
    cloud_root: &Path,
) -> Result<PathBuf, String> {
    let encoded = serde_json::to_vec_pretty(receipt)
        .map_err(|_| "materialization-execution-receipt-json-invalid".to_string())?;
    if encoded.len() > MAX_RECEIPT_BYTES {
        return Err("materialization-execution-receipt-too-large".into());
    }
    if !receipt_dir.is_absolute()
        || receipt_dir
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err("materialization-execution-receipt-directory-unsafe".into());
    }
    let mut directory_created = false;
    match std::fs::symlink_metadata(receipt_dir) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
        Ok(_) => return Err("materialization-execution-receipt-directory-unsafe".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let parent = receipt_dir
                .parent()
                .filter(|path| !path.as_os_str().is_empty())
                .ok_or_else(|| {
                    "materialization-execution-receipt-directory-parent-missing".to_string()
                })?;
            let name = receipt_dir.file_name().ok_or_else(|| {
                "materialization-execution-receipt-directory-name-invalid".to_string()
            })?;
            if !matches!(
                Path::new(name).components().collect::<Vec<_>>().as_slice(),
                [Component::Normal(_)]
            ) {
                return Err("materialization-execution-receipt-directory-name-invalid".into());
            }
            let parent_metadata = std::fs::symlink_metadata(parent).map_err(|_| {
                "materialization-execution-receipt-directory-parent-unavailable".to_string()
            })?;
            if !parent_metadata.is_dir() || parent_metadata.file_type().is_symlink() {
                return Err("materialization-execution-receipt-directory-parent-unsafe".into());
            }
            let canonical_parent = std::fs::canonicalize(parent).map_err(|_| {
                "materialization-execution-receipt-directory-parent-unavailable".to_string()
            })?;
            if canonical_parent.starts_with(source_root) || canonical_parent.starts_with(cloud_root)
            {
                return Err("materialization-execution-receipt-directory-overlaps-data".into());
            }
            std::fs::create_dir(receipt_dir).map_err(|_| {
                "materialization-execution-receipt-directory-create-failed".to_string()
            })?;
            directory_created = true;
        }
        Err(_) => return Err("materialization-execution-receipt-directory-unavailable".into()),
    }
    let metadata = std::fs::symlink_metadata(receipt_dir)
        .map_err(|_| "materialization-execution-receipt-directory-unavailable".to_string())?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        if directory_created {
            let _ = std::fs::remove_dir(receipt_dir);
        }
        return Err("materialization-execution-receipt-directory-unsafe".into());
    }
    let canonical = match std::fs::canonicalize(receipt_dir) {
        Ok(path) => path,
        Err(_) => {
            if directory_created {
                let _ = std::fs::remove_dir(receipt_dir);
            }
            return Err("materialization-execution-receipt-directory-unavailable".into());
        }
    };
    if canonical.starts_with(source_root) || canonical.starts_with(cloud_root) {
        if directory_created {
            let _ = std::fs::remove_dir(receipt_dir);
        }
        return Err("materialization-execution-receipt-directory-overlaps-data".into());
    }
    let path = canonical.join(format!("{}.json", receipt.receipt_id));
    #[cfg(unix)]
    let file_result = {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o400)
            .open(&path)
    };
    #[cfg(not(unix))]
    let file_result = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path);
    let mut file = match file_result {
        Ok(file) => file,
        Err(_) => {
            if directory_created {
                let _ = std::fs::remove_dir(receipt_dir);
            }
            return Err("materialization-execution-receipt-create-failed".into());
        }
    };
    let result = (|| -> Result<(), String> {
        file.write_all(&encoded)
            .and_then(|_| file.sync_all())
            .map_err(|_| "materialization-execution-receipt-write-failed".to_string())?;
        let receipt_metadata = file
            .metadata()
            .map_err(|_| "materialization-execution-receipt-metadata-failed".to_string())?;
        if !receipt_metadata.is_file() || receipt_metadata.file_type().is_symlink() {
            return Err("materialization-execution-receipt-file-unsafe".into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if receipt_metadata.permissions().mode() & 0o777 != 0o400 {
                return Err("materialization-execution-receipt-permissions-invalid".into());
            }
        }
        #[cfg(not(unix))]
        {
            let mut permissions = receipt_metadata.permissions();
            permissions.set_readonly(true);
            std::fs::set_permissions(&path, permissions)
                .map_err(|_| "materialization-execution-receipt-permissions-failed".to_string())?;
        }
        #[cfg(unix)]
        File::open(&canonical)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| "materialization-execution-receipt-directory-sync-failed".to_string())?;
        Ok(())
    })();
    if let Err(error) = result {
        drop(file);
        remove_file_if_created(&path);
        if directory_created {
            let _ = std::fs::remove_dir(receipt_dir);
        }
        return Err(error);
    }
    Ok(path)
}

fn copy_range_to_staging(
    source_path: &Path,
    source_unit: &IncompleteDownloadMaterializationUnit,
    staging_path: &Path,
) -> Result<(), String> {
    let before = std::fs::symlink_metadata(source_path)
        .map_err(|_| "materialization-execution-source-unavailable".to_string())?;
    if !source_metadata_matches(&before, source_unit) {
        return Err("materialization-execution-source-changed".into());
    }
    let active_before = observe_path_active_use(source_path);
    if !active_before.evidence_complete || active_before.active {
        return Err("materialization-execution-source-active-or-unverified".into());
    }

    let mut source = File::open(source_path)
        .map_err(|_| "materialization-execution-source-open-failed".to_string())?;
    source
        .seek(SeekFrom::Start(source_unit.range_start))
        .map_err(|_| "materialization-execution-source-seek-failed".to_string())?;
    let mut destination = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(staging_path)
        .map_err(|_| "materialization-execution-staging-create-failed".to_string())?;
    let copy_result = (|| -> Result<ContentDigests, String> {
        let mut remaining = source_unit.output_bytes;
        let mut hasher = ContentHasher::default();
        let mut buffer = vec![0u8; IO_BUFFER_BYTES];
        while remaining > 0 {
            let request = usize::try_from(remaining.min(buffer.len() as u64))
                .map_err(|_| "materialization-execution-range-too-large".to_string())?;
            let read = source
                .read(&mut buffer[..request])
                .map_err(|_| "materialization-execution-source-read-failed".to_string())?;
            if read == 0 {
                return Err("materialization-execution-source-short-read".into());
            }
            destination
                .write_all(&buffer[..read])
                .map_err(|_| "materialization-execution-staging-write-failed".to_string())?;
            hasher.update(&buffer[..read]);
            remaining -= read as u64;
        }
        destination
            .sync_all()
            .map_err(|_| "materialization-execution-staging-sync-failed".to_string())?;
        Ok(hasher.finalize())
    })();
    drop(destination);
    drop(source);
    let streamed = match copy_result {
        Ok(digests) => digests,
        Err(error) => {
            remove_file_if_created(staging_path);
            return Err(error);
        }
    };
    let active_after = observe_path_active_use(source_path);
    let after = std::fs::symlink_metadata(source_path)
        .map_err(|_| "materialization-execution-source-unavailable".to_string())?;
    let staging_metadata = std::fs::symlink_metadata(staging_path)
        .map_err(|_| "materialization-execution-staging-unavailable".to_string())?;
    let verified = active_after.evidence_complete
        && !active_after.active
        && source_metadata_matches(&after, source_unit)
        && staging_metadata.is_file()
        && !staging_metadata.file_type().is_symlink()
        && staging_metadata.len() == source_unit.output_bytes
        && streamed == source_unit.content_digests
        && hash_file(staging_path)? == source_unit.content_digests;
    if !verified {
        remove_file_if_created(staging_path);
        return Err("materialization-execution-copy-verification-failed".into());
    }
    Ok(())
}

pub fn execute_incomplete_download_materialization(
    source_root: &Path,
    materialization: &IncompleteDownloadMaterializationReport,
    plan: &IncompleteDownloadDestinationPlan,
    approval: &IncompleteDownloadDestinationApproval,
    confirmed_plan_fingerprint: &str,
    fresh_capacity_snapshot: CloudCapacitySnapshot,
    receipt_dir: &Path,
    executed_at_ms: u64,
) -> Result<(IncompleteDownloadMaterializationReceipt, PathBuf), String> {
    validate_lineage_binding(materialization, plan)?;
    validate_incomplete_download_destination_approval(plan, approval, confirmed_plan_fingerprint)?;
    if executed_at_ms < approval.approved_at_ms {
        return Err("materialization-execution-predates-approval".into());
    }
    let fresh_capacity = preflight_capacity(plan, fresh_capacity_snapshot, executed_at_ms)?;

    let source_root_metadata = std::fs::symlink_metadata(source_root)
        .map_err(|_| "materialization-execution-source-root-unavailable".to_string())?;
    if !source_root_metadata.is_dir() || source_root_metadata.file_type().is_symlink() {
        return Err("materialization-execution-source-root-unsafe".into());
    }
    let canonical_source_root = std::fs::canonicalize(source_root)
        .map_err(|_| "materialization-execution-source-root-unavailable".to_string())?;
    let cloud_root = Path::new(&plan.cloud_root);
    let cloud_root_metadata = std::fs::symlink_metadata(cloud_root)
        .map_err(|_| "materialization-execution-cloud-root-unavailable".to_string())?;
    if !cloud_root_metadata.is_dir() || cloud_root_metadata.file_type().is_symlink() {
        return Err("materialization-execution-cloud-root-unsafe".into());
    }
    let canonical_cloud_root = std::fs::canonicalize(cloud_root)
        .map_err(|_| "materialization-execution-cloud-root-unavailable".to_string())?;
    if canonical_cloud_root.to_string_lossy() != plan.cloud_root {
        return Err("materialization-execution-cloud-root-changed".into());
    }
    let destination_directory_relative = Path::new(&plan.destination_subdirectory);
    validate_existing_directory_prefix(&canonical_cloud_root, destination_directory_relative)?;
    let created_directories =
        missing_directories(&canonical_cloud_root, destination_directory_relative)?;

    let mut source_paths = Vec::with_capacity(materialization.units.len());
    let mut final_paths = Vec::with_capacity(plan.units.len());
    let mut staging_paths = Vec::with_capacity(plan.units.len());
    for (index, (source_unit, destination_unit)) in
        materialization.units.iter().zip(&plan.units).enumerate()
    {
        let source_relative = Path::new(&source_unit.source_relative_path);
        if !safe_relative_path(source_relative) {
            return Err("materialization-execution-source-relative-path-unsafe".into());
        }
        let source_path = canonical_source_root.join(source_relative);
        let source_metadata = std::fs::symlink_metadata(&source_path)
            .map_err(|_| "materialization-execution-source-unavailable".to_string())?;
        if !source_metadata_matches(&source_metadata, source_unit) {
            return Err("materialization-execution-source-changed".into());
        }
        let canonical_source = std::fs::canonicalize(&source_path)
            .map_err(|_| "materialization-execution-source-unavailable".to_string())?;
        if !canonical_source.starts_with(&canonical_source_root)
            || canonical_source.starts_with(&canonical_cloud_root)
        {
            return Err("materialization-execution-source-path-unsafe".into());
        }
        let active = observe_path_active_use(&source_path);
        if !active.evidence_complete || active.active {
            return Err("materialization-execution-source-active-or-unverified".into());
        }

        let destination_relative = Path::new(&destination_unit.destination_relative_path);
        if !safe_relative_path(destination_relative)
            || destination_relative.parent() != Some(destination_directory_relative)
        {
            return Err("materialization-execution-destination-relative-path-unsafe".into());
        }
        let final_path = canonical_cloud_root.join(destination_relative);
        let staging_name = format!(
            ".disksage-{}-{index:04}.partial",
            &approval.approval_id[..16]
        );
        let staging_path = final_path
            .parent()
            .ok_or_else(|| "materialization-execution-destination-parent-missing".to_string())?
            .join(staging_name);
        if std::fs::symlink_metadata(&final_path).is_ok()
            || std::fs::symlink_metadata(&staging_path).is_ok()
        {
            return Err("materialization-execution-destination-collision".into());
        }
        source_paths.push(source_path);
        final_paths.push(final_path);
        staging_paths.push(staging_path);
    }

    let destination_directory = canonical_cloud_root.join(destination_directory_relative);
    if std::fs::create_dir_all(&destination_directory).is_err() {
        rollback(&[], &created_directories);
        return Err("materialization-execution-destination-create-failed".into());
    }
    let canonical_destination_directory = match std::fs::canonicalize(&destination_directory) {
        Ok(path) => path,
        Err(_) => {
            rollback(&[], &created_directories);
            return Err("materialization-execution-destination-unavailable".into());
        }
    };
    if !canonical_destination_directory.starts_with(&canonical_cloud_root) {
        rollback(&[], &created_directories);
        return Err("materialization-execution-destination-escapes-cloud-root".into());
    }

    let mut created_files = Vec::new();
    let execution_result = (|| -> Result<Vec<IncompleteDownloadMaterializedUnit>, String> {
        for ((source_path, source_unit), staging_path) in source_paths
            .iter()
            .zip(&materialization.units)
            .zip(&staging_paths)
        {
            copy_range_to_staging(source_path, source_unit, staging_path)?;
            created_files.push(staging_path.clone());
        }
        for (staging, final_path) in staging_paths.iter().zip(&final_paths) {
            if std::fs::symlink_metadata(final_path).is_ok() {
                return Err("materialization-execution-destination-collision".into());
            }
            finalize_staging_create_new(staging, final_path)?;
            if let Some(position) = created_files.iter().position(|path| path == staging) {
                created_files.remove(position);
            }
            created_files.push(final_path.clone());
        }
        #[cfg(unix)]
        File::open(&canonical_destination_directory)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| "materialization-execution-destination-sync-failed".to_string())?;

        let mut receipt_units = Vec::with_capacity(plan.units.len());
        for (((source_unit, destination_unit), source_path), final_path) in materialization
            .units
            .iter()
            .zip(&plan.units)
            .zip(&source_paths)
            .zip(&final_paths)
        {
            let active = observe_path_active_use(source_path);
            let source_metadata = std::fs::symlink_metadata(source_path)
                .map_err(|_| "materialization-execution-source-unavailable".to_string())?;
            let output_metadata = std::fs::symlink_metadata(final_path)
                .map_err(|_| "materialization-execution-output-unavailable".to_string())?;
            let verified = active.evidence_complete
                && !active.active
                && source_metadata_matches(&source_metadata, source_unit)
                && output_metadata.is_file()
                && !output_metadata.file_type().is_symlink()
                && output_metadata.len() == source_unit.output_bytes
                && hash_file(final_path)? == source_unit.content_digests;
            if !verified {
                return Err("materialization-execution-final-verification-failed".into());
            }
            receipt_units.push(IncompleteDownloadMaterializedUnit {
                materialization_unit_fingerprint: source_unit.unit_fingerprint.clone(),
                kind: source_unit.kind,
                source_relative_path: source_unit.source_relative_path.clone(),
                range_start: source_unit.range_start,
                range_end: source_unit.range_end,
                destination_relative_path: destination_unit.destination_relative_path.clone(),
                output_bytes: source_unit.output_bytes,
                content_digests: source_unit.content_digests.clone(),
                source_stable: true,
                output_verified: true,
                write_performed: true,
            });
        }
        Ok(receipt_units)
    })();

    let receipt_units = match execution_result {
        Ok(units) => units,
        Err(error) => {
            rollback(&created_files, &created_directories);
            return Err(error);
        }
    };
    let mut receipt = IncompleteDownloadMaterializationReceipt {
        schema_version: INCOMPLETE_DOWNLOAD_EXECUTION_VERSION,
        receipt_id: String::new(),
        destination_plan_fingerprint: plan.destination_plan_fingerprint.clone(),
        materialization_plan_fingerprint: materialization.plan_fingerprint.clone(),
        approval_id: approval.approval_id.clone(),
        provider: plan.provider,
        account_scope: plan.account_scope,
        cloud_root: plan.cloud_root.clone(),
        destination_subdirectory: plan.destination_subdirectory.clone(),
        executed_at_ms,
        fresh_capacity,
        source_file_count: materialization.source_file_count,
        unit_count: receipt_units.len(),
        materialized_bytes: receipt_units.iter().map(|unit| unit.output_bytes).sum(),
        units: receipt_units,
        all_outputs_verified: true,
        provider_sync_confirmed: false,
        source_eviction_authorized: false,
        source_mutation_performed: false,
        production_time_ms: None,
        production_time_source: None,
    };
    receipt.receipt_id = match receipt_id_for(&receipt) {
        Ok(id) => id,
        Err(error) => {
            rollback(&created_files, &created_directories);
            return Err(error);
        }
    };
    if !incomplete_download_materialization_receipt_integrity_valid(&receipt) {
        rollback(&created_files, &created_directories);
        return Err("materialization-execution-receipt-integrity-invalid".into());
    }
    let receipt_path = match write_immutable_receipt(
        &receipt,
        receipt_dir,
        &canonical_source_root,
        &canonical_cloud_root,
    ) {
        Ok(path) => path,
        Err(error) => {
            rollback(&created_files, &created_directories);
            return Err(error);
        }
    };
    Ok((receipt, receipt_path))
}

pub fn summarize_incomplete_download_materialization_receipt(
    receipt: &IncompleteDownloadMaterializationReceipt,
) -> IncompleteDownloadMaterializationReceiptSummary {
    IncompleteDownloadMaterializationReceiptSummary {
        schema_version: receipt.schema_version,
        output_mode: "redacted-materialization-receipt-summary".into(),
        receipt_id: receipt.receipt_id.clone(),
        destination_plan_fingerprint: receipt.destination_plan_fingerprint.clone(),
        materialization_plan_fingerprint: receipt.materialization_plan_fingerprint.clone(),
        approval_id: receipt.approval_id.clone(),
        provider: receipt.provider,
        account_scope: receipt.account_scope,
        executed_at_ms: receipt.executed_at_ms,
        capacity_evidence_kind: receipt.fresh_capacity.snapshot.evidence_kind,
        capacity_evidence_fingerprint: receipt.fresh_capacity.snapshot.evidence_fingerprint.clone(),
        source_file_count: receipt.source_file_count,
        unit_count: receipt.unit_count,
        materialized_bytes: receipt.materialized_bytes,
        all_outputs_verified: receipt.all_outputs_verified,
        provider_sync_confirmed: receipt.provider_sync_confirmed,
        source_eviction_authorized: receipt.source_eviction_authorized,
        source_mutation_performed: receipt.source_mutation_performed,
        production_time_assigned: receipt.production_time_ms.is_some(),
        filename_date_used_as_production_time: false,
        filesystem_times_used_only_for_source_stability: true,
        paths_names_ranges_and_digests_redacted: true,
        redacted_from_summary: vec![
            "cloud-root".into(),
            "destination-subdirectory".into(),
            "source-relative-paths".into(),
            "destination-relative-paths".into(),
            "source-and-destination-file-names".into(),
            "source-range-offsets".into(),
            "content-digests".into(),
        ],
    }
}

#[cfg(all(test, not(coverage)))]
mod tests {
    use super::*;
    use crate::cloud::CloudRoot;
    use crate::incomplete_download::{
        collect_incomplete_download_audit, DEFAULT_MAX_ENTRIES, DEFAULT_STALE_AFTER_DAYS,
    };
    use crate::incomplete_download_materialization::{
        plan_incomplete_download_materialization, MATERIALIZATION_ACTIVE_USE_TEST_LOCK,
    };
    use crate::incomplete_download_materialization_destination::{
        approve_incomplete_download_destination, plan_incomplete_download_destination,
    };
    use crate::incomplete_download_recovery::{
        validate_incomplete_download_recovery, RecoveryValidationLimits,
    };
    use crate::provider_capacity::{parse_icloud_brctl_quota, DEFAULT_CAPACITY_RESERVE_BYTES};

    fn zip_bytes(payload: &[u8]) -> Vec<u8> {
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
        bytes
    }

    fn write_zip(path: &Path, payload: &[u8]) -> Vec<u8> {
        let bytes = zip_bytes(payload);
        let mut file = File::create(path).unwrap();
        file.write_all(&bytes).unwrap();
        file.sync_all().unwrap();
        drop(file);
        bytes
    }

    fn fixtures(
        source: &Path,
        cloud: &Path,
    ) -> (
        IncompleteDownloadMaterializationReport,
        IncompleteDownloadDestinationPlan,
        IncompleteDownloadDestinationApproval,
        CloudCapacitySnapshot,
    ) {
        let created = std::fs::metadata(source).unwrap().modified().unwrap();
        let observed_at_ms = created
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
            + 31 * 86_400_000;
        let audit = collect_incomplete_download_audit(
            source,
            observed_at_ms,
            DEFAULT_MAX_ENTRIES,
            DEFAULT_STALE_AFTER_DAYS,
        )
        .unwrap();
        let recovery = validate_incomplete_download_recovery(
            source,
            &audit,
            observed_at_ms + 1,
            RecoveryValidationLimits::default(),
        )
        .unwrap();
        let materialization =
            plan_incomplete_download_materialization(source, &audit, &recovery, observed_at_ms + 2)
                .unwrap();
        let capacity = parse_icloud_brctl_quota(
            "10000000000 bytes of quota remaining in personal account\n",
            observed_at_ms + 3,
        )
        .unwrap();
        let root = CloudRoot {
            id: "icloud:test".into(),
            provider: CloudProvider::Icloud,
            account_scope: CloudAccountScope::Unknown,
            label: "iCloud".into(),
            path: cloud.to_string_lossy().into_owned(),
            readable: true,
            access_issue: None,
        };
        let plan = plan_incomplete_download_destination(
            &materialization,
            &root,
            "DiskSage/Recovered",
            capacity.clone(),
            DEFAULT_CAPACITY_RESERVE_BYTES,
            observed_at_ms + 4,
        )
        .unwrap();
        let approval = approve_incomplete_download_destination(
            &plan,
            &plan.destination_plan_fingerprint,
            observed_at_ms + 5,
            "human:test",
            "approved exact test materialization",
        )
        .unwrap();
        (materialization, plan, approval, capacity)
    }

    #[test]
    fn executes_approved_range_atomically_and_writes_immutable_receipt() {
        let _active_use_guard = MATERIALIZATION_ACTIVE_USE_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let source = tempfile::tempdir().unwrap();
        let cloud = tempfile::tempdir().unwrap();
        let receipts = tempfile::tempdir().unwrap();
        let source_path = source.path().join("whole.zip.crdownload");
        let original = write_zip(&source_path, b"validated payload");
        let (materialization, plan, approval, capacity) = fixtures(source.path(), cloud.path());
        let (receipt, receipt_path) = execute_incomplete_download_materialization(
            source.path(),
            &materialization,
            &plan,
            &approval,
            &plan.destination_plan_fingerprint,
            capacity,
            receipts.path(),
            approval.approved_at_ms + 1,
        )
        .unwrap();

        assert!(incomplete_download_materialization_receipt_integrity_valid(
            &receipt
        ));
        assert_eq!(receipt.unit_count, 1);
        assert_eq!(receipt.materialized_bytes, original.len() as u64);
        assert_eq!(std::fs::read(&source_path).unwrap(), original);
        assert_eq!(
            std::fs::read(
                Path::new(&plan.cloud_root).join(&plan.units[0].destination_relative_path)
            )
            .unwrap(),
            original
        );
        assert!(receipt_path.is_file());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&receipt_path)
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o400
            );
        }
        assert!(!receipt.provider_sync_confirmed);
        assert!(!receipt.source_eviction_authorized);
        assert!(!receipt.source_mutation_performed);
        let summary = summarize_incomplete_download_materialization_receipt(&receipt);
        let encoded = serde_json::to_string(&summary).unwrap();
        assert!(!encoded.contains("whole.zip.crdownload"));
        assert!(!encoded.contains(cloud.path().to_string_lossy().as_ref()));
        assert!(!encoded.contains(&receipt.units[0].content_digests.sha256));
        assert!(!summary.production_time_assigned);
        assert!(!summary.filename_date_used_as_production_time);
        let mut tampered = receipt.clone();
        tampered.materialized_bytes += 1;
        assert!(!incomplete_download_materialization_receipt_integrity_valid(&tampered));
    }

    #[test]
    fn materializes_only_the_validated_embedded_zip_range() {
        let _active_use_guard = MATERIALIZATION_ACTIVE_USE_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let source = tempfile::tempdir().unwrap();
        let cloud = tempfile::tempdir().unwrap();
        let receipts = tempfile::tempdir().unwrap();
        let zip = zip_bytes(b"embedded validated payload");
        let mut source_bytes = b"unrelated-prefix".to_vec();
        source_bytes.extend_from_slice(&zip);
        source_bytes.extend_from_slice(b"unrelated-trailing");
        let source_path = source.path().join("embedded.bin.crdownload");
        let mut file = File::create(&source_path).unwrap();
        file.write_all(&source_bytes).unwrap();
        file.sync_all().unwrap();
        drop(file);

        let (materialization, plan, approval, capacity) = fixtures(source.path(), cloud.path());
        assert_eq!(materialization.unit_count, 1);
        assert_eq!(
            materialization.units[0].kind,
            MaterializationUnitKind::EmbeddedZipRange
        );
        let (receipt, _) = execute_incomplete_download_materialization(
            source.path(),
            &materialization,
            &plan,
            &approval,
            &plan.destination_plan_fingerprint,
            capacity,
            receipts.path(),
            approval.approved_at_ms + 1,
        )
        .unwrap();
        assert_eq!(receipt.materialized_bytes, zip.len() as u64);
        assert_eq!(
            std::fs::read(
                Path::new(&plan.cloud_root).join(&plan.units[0].destination_relative_path)
            )
            .unwrap(),
            zip
        );
        assert_eq!(std::fs::read(&source_path).unwrap(), source_bytes);
    }

    #[test]
    fn rejects_wrong_approval_without_destination_or_receipt_mutation() {
        let _active_use_guard = MATERIALIZATION_ACTIVE_USE_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let source = tempfile::tempdir().unwrap();
        let cloud = tempfile::tempdir().unwrap();
        let receipts = tempfile::tempdir().unwrap();
        write_zip(
            &source.path().join("whole.zip.crdownload"),
            b"validated payload",
        );
        let (materialization, plan, approval, capacity) = fixtures(source.path(), cloud.path());
        assert!(execute_incomplete_download_materialization(
            source.path(),
            &materialization,
            &plan,
            &approval,
            &"0".repeat(64),
            capacity,
            receipts.path(),
            approval.approved_at_ms + 1,
        )
        .is_err());
        assert!(!cloud.path().join("DiskSage").exists());
        assert_eq!(std::fs::read_dir(receipts.path()).unwrap().count(), 0);
    }

    #[test]
    fn collision_and_stale_capacity_fail_before_writes() {
        let _active_use_guard = MATERIALIZATION_ACTIVE_USE_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let source = tempfile::tempdir().unwrap();
        let cloud = tempfile::tempdir().unwrap();
        let receipts = tempfile::tempdir().unwrap();
        write_zip(
            &source.path().join("whole.zip.crdownload"),
            b"validated payload",
        );
        let (materialization, plan, approval, capacity) = fixtures(source.path(), cloud.path());
        assert!(execute_incomplete_download_materialization(
            source.path(),
            &materialization,
            &plan,
            &approval,
            &plan.destination_plan_fingerprint,
            capacity,
            receipts.path(),
            approval.approved_at_ms + MAX_CAPACITY_AGE_MS + 1,
        )
        .is_err());
        assert!(!cloud.path().join("DiskSage").exists());
        assert_eq!(std::fs::read_dir(receipts.path()).unwrap().count(), 0);
    }

    #[test]
    fn overlapping_receipt_directory_is_not_created_and_outputs_roll_back() {
        let _active_use_guard = MATERIALIZATION_ACTIVE_USE_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let source = tempfile::tempdir().unwrap();
        let cloud = tempfile::tempdir().unwrap();
        write_zip(
            &source.path().join("whole.zip.crdownload"),
            b"validated payload",
        );
        let (materialization, plan, approval, capacity) = fixtures(source.path(), cloud.path());
        let overlapping_receipts = source.path().join("receipts");
        assert!(execute_incomplete_download_materialization(
            source.path(),
            &materialization,
            &plan,
            &approval,
            &plan.destination_plan_fingerprint,
            capacity,
            &overlapping_receipts,
            approval.approved_at_ms + 1,
        )
        .is_err());
        assert!(!overlapping_receipts.exists());
        assert!(!cloud.path().join("DiskSage").exists());
    }

    #[test]
    fn finalization_never_replaces_an_existing_destination() {
        let temp = tempfile::tempdir().unwrap();
        let staging = temp.path().join("staging.partial");
        let final_path = temp.path().join("final.zip");
        std::fs::write(&staging, b"new").unwrap();
        std::fs::write(&final_path, b"existing").unwrap();
        assert!(finalize_staging_create_new(&staging, &final_path).is_err());
        assert_eq!(std::fs::read(&final_path).unwrap(), b"existing");
        assert_eq!(std::fs::read(&staging).unwrap(), b"new");
    }
}
