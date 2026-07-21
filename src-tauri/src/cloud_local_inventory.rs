//! Bounded, metadata-only inventory of local blocks occupied inside a cloud root.
//!
//! This module never opens file contents and never treats allocation evidence as provider-sync
//! evidence. Its output is a prioritised review list, not an eviction permit.

use std::collections::VecDeque;
use std::fs::{self, Metadata};
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::cloud::{self, CloudAccountScope, CloudProvider, CloudRoot};

const MAX_ENTRY_LIMIT: u64 = 1_000_000;
const MAX_RESULT_LIMIT: usize = 10_000;
const MAX_DEPTH_LIMIT: usize = 64;
const MAX_DURATION_LIMIT_MS: u64 = 300_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CloudLocalInventoryOptions {
    pub min_allocated_bytes: u64,
    pub max_entries: u64,
    pub max_results: usize,
    pub max_depth: usize,
    pub max_duration_ms: u64,
}

impl Default for CloudLocalInventoryOptions {
    fn default() -> Self {
        Self {
            min_allocated_bytes: 128 * 1024 * 1024,
            max_entries: 100_000,
            max_results: 200,
            max_depth: 4,
            max_duration_ms: 30_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CloudLocalAllocationCandidate {
    pub path: String,
    pub logical_bytes: u64,
    pub allocated_bytes: u64,
    pub filesystem_created_ms: Option<u64>,
    pub filesystem_modified_ms: Option<u64>,
    pub allocation_evidence: String,
    pub content_opened: bool,
    pub embedded_metadata_inspected: bool,
    pub provider_sync_attested: bool,
    pub eviction_blockers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CloudLocalAllocationInventory {
    pub version: u32,
    pub cloud_root_id: String,
    pub provider: CloudProvider,
    pub account_scope: CloudAccountScope,
    pub cloud_root: String,
    pub observed_at_ms: u64,
    pub options: CloudLocalInventoryOptions,
    pub visited_entries: u64,
    pub visited_files: u64,
    pub visited_directories: u64,
    pub skipped_entries: u64,
    pub allocated_candidate_bytes: u64,
    pub candidates: Vec<CloudLocalAllocationCandidate>,
    pub results_truncated: bool,
    pub evidence_complete: bool,
    pub stop_reasons: Vec<String>,
    pub notices: Vec<String>,
}

fn base_notices() -> Vec<String> {
    vec![
        "metadata-only-content-not-opened".into(),
        "embedded-production-metadata-not-inspected".into(),
        "provider-sync-not-attested".into(),
        "inventory-does-not-authorize-eviction".into(),
    ]
}

fn validate_options(options: CloudLocalInventoryOptions) -> Result<(), String> {
    if options.max_entries == 0 || options.max_entries > MAX_ENTRY_LIMIT {
        return Err("cloud-local-inventory-max-entries-invalid".into());
    }
    if options.max_results == 0 || options.max_results > MAX_RESULT_LIMIT {
        return Err("cloud-local-inventory-max-results-invalid".into());
    }
    if options.max_depth > MAX_DEPTH_LIMIT {
        return Err("cloud-local-inventory-max-depth-invalid".into());
    }
    if options.max_duration_ms == 0 || options.max_duration_ms > MAX_DURATION_LIMIT_MS {
        return Err("cloud-local-inventory-max-duration-invalid".into());
    }
    Ok(())
}

fn push_unique(values: &mut Vec<String>, value: &str) {
    if !values.iter().any(|existing| existing == value) {
        values.push(value.into());
    }
}

fn system_time_ms(value: std::io::Result<std::time::SystemTime>) -> Option<u64> {
    value
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
}

#[cfg(unix)]
fn allocated_bytes(metadata: &Metadata) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    Some(metadata.blocks().saturating_mul(512))
}

#[cfg(not(unix))]
fn allocated_bytes(_metadata: &Metadata) -> Option<u64> {
    None
}

fn candidate(
    path: &Path,
    metadata: &Metadata,
    allocated_bytes: u64,
) -> CloudLocalAllocationCandidate {
    CloudLocalAllocationCandidate {
        path: path.to_string_lossy().into_owned(),
        logical_bytes: metadata.len(),
        allocated_bytes,
        filesystem_created_ms: system_time_ms(metadata.created()),
        filesystem_modified_ms: system_time_ms(metadata.modified()),
        allocation_evidence: "filesystem:st-blocks-512".into(),
        content_opened: false,
        embedded_metadata_inspected: false,
        provider_sync_attested: false,
        eviction_blockers: vec![
            "provider-sync-unverified".into(),
            "human-eviction-approval-required".into(),
        ],
    }
}

/// Inventory local filesystem allocation under one already-discovered cloud root.
///
/// Directory entries and filesystem metadata are the only inputs. File content is never opened,
/// and provider completion is deliberately left unattested. Bounds are cooperative: a single
/// platform `read_dir` call cannot be interrupted, but the next directory or entry observes the
/// duration, entry, and depth limits.
pub fn inventory_cloud_local_allocations(
    root: &CloudRoot,
    options: CloudLocalInventoryOptions,
    observed_at_ms: u64,
) -> Result<CloudLocalAllocationInventory, String> {
    let started = Instant::now();
    inventory_with_elapsed(root, options, observed_at_ms, || {
        u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
    })
}

/// Build a fail-closed report after an external worker watchdog terminates a blocked platform
/// directory enumeration. No filesystem call is made here, so the timeout path itself cannot block.
pub fn hard_timeout_inventory(
    root: &CloudRoot,
    options: CloudLocalInventoryOptions,
    observed_at_ms: u64,
) -> Result<CloudLocalAllocationInventory, String> {
    validate_options(options)?;
    let mut notices = base_notices();
    notices.push("inventory-incomplete".into());
    notices.push("worker-hard-timeout".into());
    Ok(CloudLocalAllocationInventory {
        version: 1,
        cloud_root_id: root.id.clone(),
        provider: root.provider,
        account_scope: root.account_scope,
        cloud_root: root.path.clone(),
        observed_at_ms,
        options,
        visited_entries: 0,
        visited_files: 0,
        visited_directories: 0,
        skipped_entries: 0,
        allocated_candidate_bytes: 0,
        candidates: Vec::new(),
        results_truncated: false,
        evidence_complete: false,
        stop_reasons: vec!["hard-timeout-reached".into()],
        notices,
    })
}

fn inventory_with_elapsed(
    root: &CloudRoot,
    options: CloudLocalInventoryOptions,
    observed_at_ms: u64,
    mut elapsed_ms: impl FnMut() -> u64,
) -> Result<CloudLocalAllocationInventory, String> {
    validate_options(options)?;
    cloud::validate_cloud_root_readable(root)?;
    let root_path = PathBuf::from(&root.path);
    let root_metadata = fs::symlink_metadata(&root_path)
        .map_err(|_| "cloud-local-inventory-root-metadata-unavailable".to_string())?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err("cloud-local-inventory-root-not-real-directory".into());
    }

    let mut queue = VecDeque::from([(root_path.clone(), 0usize)]);
    let mut candidates = Vec::new();
    let mut visited_entries = 0u64;
    let mut visited_files = 0u64;
    let mut visited_directories = 0u64;
    let mut skipped_entries = 0u64;
    let mut allocated_candidate_bytes = 0u64;
    let mut stop_reasons = Vec::new();

    'directories: while let Some((directory, depth)) = queue.pop_front() {
        if elapsed_ms() >= options.max_duration_ms {
            push_unique(&mut stop_reasons, "max-duration-reached");
            break;
        }
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(_) => {
                skipped_entries = skipped_entries.saturating_add(1);
                push_unique(&mut stop_reasons, "entry-errors");
                continue;
            }
        };
        for entry in entries {
            if elapsed_ms() >= options.max_duration_ms {
                push_unique(&mut stop_reasons, "max-duration-reached");
                break 'directories;
            }
            if visited_entries >= options.max_entries {
                push_unique(&mut stop_reasons, "max-entries-reached");
                break 'directories;
            }
            visited_entries = visited_entries.saturating_add(1);
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => {
                    skipped_entries = skipped_entries.saturating_add(1);
                    push_unique(&mut stop_reasons, "entry-errors");
                    continue;
                }
            };
            let path = entry.path();
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(_) => {
                    skipped_entries = skipped_entries.saturating_add(1);
                    push_unique(&mut stop_reasons, "entry-errors");
                    continue;
                }
            };
            let file_type = metadata.file_type();
            if file_type.is_symlink() {
                skipped_entries = skipped_entries.saturating_add(1);
                continue;
            }
            if file_type.is_dir() {
                visited_directories = visited_directories.saturating_add(1);
                if depth < options.max_depth {
                    queue.push_back((path, depth + 1));
                } else {
                    push_unique(&mut stop_reasons, "max-depth-reached");
                }
                continue;
            }
            if !file_type.is_file() {
                skipped_entries = skipped_entries.saturating_add(1);
                continue;
            }

            visited_files = visited_files.saturating_add(1);
            let Some(local_bytes) = allocated_bytes(&metadata) else {
                skipped_entries = skipped_entries.saturating_add(1);
                push_unique(&mut stop_reasons, "allocated-byte-evidence-unavailable");
                continue;
            };
            if local_bytes == 0 || local_bytes < options.min_allocated_bytes {
                continue;
            }
            allocated_candidate_bytes = allocated_candidate_bytes.saturating_add(local_bytes);
            candidates.push(candidate(&path, &metadata, local_bytes));
        }
    }

    candidates.sort_by(|left, right| {
        right
            .allocated_bytes
            .cmp(&left.allocated_bytes)
            .then_with(|| left.path.cmp(&right.path))
    });
    let results_truncated = candidates.len() > options.max_results;
    candidates.truncate(options.max_results);
    let evidence_complete = stop_reasons.is_empty() && skipped_entries == 0;
    let mut notices = base_notices();
    if results_truncated {
        notices.push("candidate-output-truncated".into());
    }
    if !evidence_complete {
        notices.push("inventory-incomplete".into());
    }

    Ok(CloudLocalAllocationInventory {
        version: 1,
        cloud_root_id: root.id.clone(),
        provider: root.provider,
        account_scope: root.account_scope,
        cloud_root: root_path.to_string_lossy().into_owned(),
        observed_at_ms,
        options,
        visited_entries,
        visited_files,
        visited_directories,
        skipped_entries,
        allocated_candidate_bytes,
        candidates,
        results_truncated,
        evidence_complete,
        stop_reasons,
        notices,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::fs::File;
    use std::io::{Seek, SeekFrom, Write};

    fn root(path: &Path) -> CloudRoot {
        CloudRoot {
            id: "icloud:test".into(),
            provider: CloudProvider::Icloud,
            account_scope: CloudAccountScope::Personal,
            label: "iCloud test".into(),
            path: path.to_string_lossy().into_owned(),
            readable: true,
            access_issue: None,
        }
    }

    fn options() -> CloudLocalInventoryOptions {
        CloudLocalInventoryOptions {
            min_allocated_bytes: 1,
            max_entries: 100,
            max_results: 10,
            max_depth: 4,
            max_duration_ms: 10_000,
        }
    }

    fn write_file(path: &Path, size: usize) {
        let mut file = File::create(path).unwrap();
        file.write_all(&vec![0x5a; size]).unwrap();
        file.sync_all().unwrap();
    }

    #[test]
    fn inventories_allocated_files_without_claiming_sync_or_lineage() {
        let temp = tempfile::tempdir().unwrap();
        write_file(&temp.path().join("large.bin"), 8192);
        write_file(&temp.path().join("small.bin"), 4096);

        let report = inventory_cloud_local_allocations(&root(temp.path()), options(), 123).unwrap();

        assert_eq!(report.version, 1);
        assert_eq!(report.observed_at_ms, 123);
        assert_eq!(report.visited_files, 2);
        assert!(report.allocated_candidate_bytes > 0);
        assert!(report.evidence_complete);
        assert!(report.candidates.iter().all(|item| {
            !item.content_opened
                && !item.embedded_metadata_inspected
                && !item.provider_sync_attested
                && item
                    .eviction_blockers
                    .contains(&"provider-sync-unverified".to_string())
        }));
    }

    #[test]
    fn entry_result_and_depth_bounds_are_explicit() {
        let temp = tempfile::tempdir().unwrap();
        for index in 0..4 {
            write_file(
                &temp.path().join(format!("{index}.bin")),
                4096 * (index + 1),
            );
        }
        fs::create_dir(temp.path().join("nested")).unwrap();
        write_file(&temp.path().join("nested/deep.bin"), 4096);

        let mut bounded = options();
        bounded.max_entries = 3;
        bounded.max_results = 1;
        bounded.max_depth = 0;
        let report = inventory_cloud_local_allocations(&root(temp.path()), bounded, 1).unwrap();

        assert!(!report.evidence_complete);
        assert!(report.stop_reasons.iter().any(|reason| {
            matches!(reason.as_str(), "max-entries-reached" | "max-depth-reached")
        }));
        assert!(report.candidates.len() <= 1);
        if report.allocated_candidate_bytes > 0 && report.visited_files > 1 {
            assert!(report.results_truncated);
        }
    }

    #[test]
    fn duration_bound_is_reported_by_injected_monotonic_clock() {
        let temp = tempfile::tempdir().unwrap();
        write_file(&temp.path().join("file.bin"), 4096);
        let ticks = Cell::new(0u64);
        let mut bounded = options();
        bounded.max_duration_ms = 1;
        let report = inventory_with_elapsed(&root(temp.path()), bounded, 1, || {
            let current = ticks.get();
            ticks.set(current + 5_000);
            current
        })
        .unwrap();

        assert!(!report.evidence_complete);
        assert_eq!(report.stop_reasons, vec!["max-duration-reached"]);
        assert!(report.candidates.is_empty());
    }

    #[test]
    fn hard_timeout_report_is_pure_empty_and_fail_closed() {
        let report = hard_timeout_inventory(&root(Path::new("/Cloud")), options(), 99).unwrap();
        assert_eq!(report.observed_at_ms, 99);
        assert!(!report.evidence_complete);
        assert_eq!(report.stop_reasons, vec!["hard-timeout-reached"]);
        assert_eq!(report.visited_entries, 0);
        assert_eq!(report.allocated_candidate_bytes, 0);
        assert!(report.candidates.is_empty());
        assert!(report.notices.contains(&"worker-hard-timeout".to_string()));
    }

    #[test]
    fn rejects_unbounded_or_non_directory_inputs() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("file.bin");
        write_file(&file, 4096);
        let mut invalid = options();
        invalid.max_entries = 0;
        assert_eq!(
            inventory_cloud_local_allocations(&root(temp.path()), invalid, 1).unwrap_err(),
            "cloud-local-inventory-max-entries-invalid"
        );
        let error = inventory_cloud_local_allocations(&root(&file), options(), 1).unwrap_err();
        assert!(
            error.starts_with("cloud-root-unreadable:")
                || error == "cloud-local-inventory-root-not-real-directory"
        );
    }

    #[test]
    fn sparse_logical_size_is_not_misreported_as_allocated_bytes() {
        let temp = tempfile::tempdir().unwrap();
        let sparse = temp.path().join("sparse.bin");
        let mut file = File::create(&sparse).unwrap();
        file.seek(SeekFrom::Start(16 * 1024 * 1024)).unwrap();
        file.write_all(&[1]).unwrap();
        file.sync_all().unwrap();
        let metadata = fs::metadata(&sparse).unwrap();

        let local = allocated_bytes(&metadata).unwrap_or_default();
        assert!(local < metadata.len());
        let report = inventory_cloud_local_allocations(&root(temp.path()), options(), 1).unwrap();
        assert_eq!(report.candidates[0].logical_bytes, metadata.len());
        assert_eq!(report.candidates[0].allocated_bytes, local);
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_not_followed() {
        use std::os::unix::fs::symlink;

        let cloud = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        write_file(&outside.path().join("outside.bin"), 8192);
        symlink(outside.path(), cloud.path().join("linked")).unwrap();

        let report = inventory_cloud_local_allocations(&root(cloud.path()), options(), 1).unwrap();
        assert_eq!(report.visited_files, 0);
        assert!(report.candidates.is_empty());
        assert_eq!(report.skipped_entries, 1);
    }
}
