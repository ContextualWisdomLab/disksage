//! Bounded, metadata-only inventory of local blocks occupied inside a cloud root.
//!
//! This module never opens file contents and never treats allocation evidence as provider-sync
//! evidence. Its output is a prioritised review list, not an eviction permit.

use std::collections::VecDeque;
use std::fs::{self, Metadata};
use std::io::{Error, ErrorKind};
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::cloud::{self, CloudAccountScope, CloudProvider, CloudRoot};

const MAX_ENTRY_LIMIT: u64 = 1_000_000;
const MAX_RESULT_LIMIT: usize = 10_000;
const MAX_DEPTH_LIMIT: usize = 64;
const MAX_DURATION_LIMIT_MS: u64 = 300_000;
const MAX_ISSUE_LIMIT: usize = 1_000;
const CHECKPOINT_ENTRY_INTERVAL: u64 = 256;
const CHECKPOINT_INTERVAL_MS: u64 = 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CloudLocalInventoryOptions {
    pub min_allocated_bytes: u64,
    pub max_entries: u64,
    pub max_results: usize,
    pub max_depth: usize,
    pub max_duration_ms: u64,
    pub max_issues: usize,
}

impl Default for CloudLocalInventoryOptions {
    fn default() -> Self {
        Self {
            min_allocated_bytes: 128 * 1024 * 1024,
            max_entries: 100_000,
            max_results: 200,
            max_depth: 4,
            max_duration_ms: 30_000,
            max_issues: 200,
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
pub struct CloudLocalInventoryIssue {
    /// Path relative to the selected scan root. `None` means the root itself or an unnamed entry
    /// returned by a failed directory iterator.
    pub relative_scope: Option<String>,
    pub kind: String,
    pub reason: String,
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
    pub issues: Vec<CloudLocalInventoryIssue>,
    pub issues_truncated: bool,
    pub allocated_candidate_bytes: u64,
    pub candidates: Vec<CloudLocalAllocationCandidate>,
    pub results_truncated: bool,
    pub evidence_complete: bool,
    pub stop_reasons: Vec<String>,
    pub notices: Vec<String>,
}

#[derive(Debug, Default)]
struct InventoryState {
    candidates: Vec<CloudLocalAllocationCandidate>,
    visited_entries: u64,
    visited_files: u64,
    visited_directories: u64,
    skipped_entries: u64,
    issues: Vec<CloudLocalInventoryIssue>,
    allocated_candidate_bytes: u64,
    stop_reasons: Vec<String>,
}

#[derive(Debug, Default)]
struct CheckpointCadence {
    emitted: bool,
    visited_entries: u64,
    skipped_entries: u64,
    elapsed_ms: u64,
}

#[derive(Debug, Clone, Copy)]
struct InventoryContext<'a> {
    root: &'a CloudRoot,
    root_path: &'a Path,
    options: CloudLocalInventoryOptions,
    observed_at_ms: u64,
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
    if options.max_issues == 0 || options.max_issues > MAX_ISSUE_LIMIT {
        return Err("cloud-local-inventory-max-issues-invalid".into());
    }
    Ok(())
}

fn stable_io_reason(error: &Error) -> &'static str {
    match error.kind() {
        ErrorKind::NotFound => "not-found",
        ErrorKind::PermissionDenied => "permission-denied",
        ErrorKind::ConnectionRefused => "connection-refused",
        ErrorKind::ConnectionReset => "connection-reset",
        ErrorKind::ConnectionAborted => "connection-aborted",
        ErrorKind::NotConnected => "not-connected",
        ErrorKind::AddrInUse => "address-in-use",
        ErrorKind::AddrNotAvailable => "address-unavailable",
        ErrorKind::BrokenPipe => "broken-pipe",
        ErrorKind::AlreadyExists => "already-exists",
        ErrorKind::WouldBlock => "would-block",
        ErrorKind::InvalidInput => "invalid-input",
        ErrorKind::InvalidData => "invalid-data",
        ErrorKind::TimedOut => "timed-out",
        ErrorKind::WriteZero => "write-zero",
        ErrorKind::Interrupted => "interrupted",
        ErrorKind::Unsupported => "unsupported",
        ErrorKind::UnexpectedEof => "unexpected-eof",
        ErrorKind::OutOfMemory => "out-of-memory",
        _ => "other-io-error",
    }
}

fn relative_scope(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    (!relative.as_os_str().is_empty()).then(|| relative.to_string_lossy().into_owned())
}

fn record_issue(
    issues: &mut Vec<CloudLocalInventoryIssue>,
    skipped_entries: &mut u64,
    max_issues: usize,
    root: &Path,
    scope: &Path,
    kind: &str,
    reason: &str,
) {
    *skipped_entries = skipped_entries.saturating_add(1);
    if issues.len() < max_issues {
        issues.push(CloudLocalInventoryIssue {
            relative_scope: relative_scope(root, scope),
            kind: kind.into(),
            reason: reason.into(),
        });
    }
}

fn push_unique(values: &mut Vec<String>, value: &str) {
    if !values.iter().any(|existing| existing == value) {
        values.push(value.into());
    }
}

fn inventory_report(
    context: InventoryContext<'_>,
    state: &InventoryState,
    checkpoint: bool,
) -> CloudLocalAllocationInventory {
    let mut candidates = state.candidates.clone();
    candidates.sort_by(|left, right| {
        right
            .allocated_bytes
            .cmp(&left.allocated_bytes)
            .then_with(|| left.path.cmp(&right.path))
    });
    let results_truncated = candidates.len() > context.options.max_results;
    candidates.truncate(context.options.max_results);
    let issues_truncated =
        state.skipped_entries > u64::try_from(state.issues.len()).unwrap_or(u64::MAX);
    let evidence_complete =
        !checkpoint && state.stop_reasons.is_empty() && state.skipped_entries == 0;
    let mut notices = base_notices();
    if results_truncated {
        notices.push("candidate-output-truncated".into());
    }
    if checkpoint {
        notices.push("inventory-checkpoint-not-terminal".into());
    } else if !evidence_complete {
        notices.push("inventory-incomplete".into());
    }
    if issues_truncated {
        notices.push("inventory-issues-truncated".into());
    }

    CloudLocalAllocationInventory {
        version: 2,
        cloud_root_id: context.root.id.clone(),
        provider: context.root.provider,
        account_scope: context.root.account_scope,
        cloud_root: context.root_path.to_string_lossy().into_owned(),
        observed_at_ms: context.observed_at_ms,
        options: context.options,
        visited_entries: state.visited_entries,
        visited_files: state.visited_files,
        visited_directories: state.visited_directories,
        skipped_entries: state.skipped_entries,
        issues: state.issues.clone(),
        issues_truncated,
        allocated_candidate_bytes: state.allocated_candidate_bytes,
        candidates,
        results_truncated,
        evidence_complete,
        stop_reasons: state.stop_reasons.clone(),
        notices,
    }
}

fn maybe_emit_checkpoint(
    context: InventoryContext<'_>,
    state: &InventoryState,
    cadence: &mut CheckpointCadence,
    elapsed_ms: u64,
    force: bool,
    emit: &mut impl FnMut(&CloudLocalAllocationInventory) -> Result<(), String>,
) -> Result<(), String> {
    let entries_due = state
        .visited_entries
        .saturating_sub(cadence.visited_entries)
        >= CHECKPOINT_ENTRY_INTERVAL;
    let time_due = elapsed_ms.saturating_sub(cadence.elapsed_ms) >= CHECKPOINT_INTERVAL_MS;
    let issue_due = state.skipped_entries != cadence.skipped_entries;
    if !force && cadence.emitted && !entries_due && !time_due && !issue_due {
        return Ok(());
    }
    emit(&inventory_report(context, state, true))?;
    cadence.emitted = true;
    cadence.visited_entries = state.visited_entries;
    cadence.skipped_entries = state.skipped_entries;
    cadence.elapsed_ms = elapsed_ms;
    Ok(())
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

/// Inventory with bounded in-memory progress snapshots for an external watchdog.
///
/// Checkpoints are non-terminal reports. Callers should retain only the latest snapshot and mark it
/// as a hard timeout if the worker is terminated. No checkpoint is written to the filesystem.
pub fn inventory_cloud_local_allocations_with_checkpoints(
    root: &CloudRoot,
    options: CloudLocalInventoryOptions,
    observed_at_ms: u64,
    mut emit: impl FnMut(&CloudLocalAllocationInventory) -> Result<(), String>,
) -> Result<CloudLocalAllocationInventory, String> {
    let started = Instant::now();
    inventory_with_elapsed_and_checkpoints(
        root,
        options,
        observed_at_ms,
        || u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        true,
        &mut emit,
    )
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
        version: 2,
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
        issues: Vec::new(),
        issues_truncated: false,
        allocated_candidate_bytes: 0,
        candidates: Vec::new(),
        results_truncated: false,
        evidence_complete: false,
        stop_reasons: vec!["hard-timeout-reached".into()],
        notices,
    })
}

/// Validate and convert the latest worker checkpoint into a fail-closed hard-timeout report.
pub fn hard_timeout_inventory_from_checkpoint(
    root: &CloudRoot,
    options: CloudLocalInventoryOptions,
    mut checkpoint: CloudLocalAllocationInventory,
) -> Result<CloudLocalAllocationInventory, String> {
    validate_options(options)?;
    if checkpoint.version != 2
        || checkpoint.cloud_root_id != root.id
        || checkpoint.provider != root.provider
        || checkpoint.account_scope != root.account_scope
        || checkpoint.cloud_root != root.path
        || checkpoint.options != options
        || checkpoint.evidence_complete
        || !checkpoint
            .notices
            .iter()
            .any(|notice| notice == "inventory-checkpoint-not-terminal")
    {
        return Err("cloud-local-inventory-checkpoint-invalid".into());
    }
    checkpoint
        .notices
        .retain(|notice| notice != "inventory-checkpoint-not-terminal");
    push_unique(&mut checkpoint.stop_reasons, "hard-timeout-reached");
    push_unique(&mut checkpoint.notices, "inventory-incomplete");
    push_unique(&mut checkpoint.notices, "worker-hard-timeout");
    push_unique(
        &mut checkpoint.notices,
        "partial-inventory-recovered-from-worker-checkpoint",
    );
    checkpoint.evidence_complete = false;
    Ok(checkpoint)
}

fn inventory_with_elapsed(
    root: &CloudRoot,
    options: CloudLocalInventoryOptions,
    observed_at_ms: u64,
    elapsed_ms: impl FnMut() -> u64,
) -> Result<CloudLocalAllocationInventory, String> {
    inventory_with_elapsed_and_checkpoints(
        root,
        options,
        observed_at_ms,
        elapsed_ms,
        false,
        &mut |_| Ok(()),
    )
}

fn inventory_with_elapsed_and_checkpoints(
    root: &CloudRoot,
    options: CloudLocalInventoryOptions,
    observed_at_ms: u64,
    mut elapsed_ms: impl FnMut() -> u64,
    checkpoints_enabled: bool,
    emit: &mut impl FnMut(&CloudLocalAllocationInventory) -> Result<(), String>,
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
    let mut state = InventoryState::default();
    let mut cadence = CheckpointCadence::default();
    let context = InventoryContext {
        root,
        root_path: &root_path,
        options,
        observed_at_ms,
    };

    'directories: while let Some((directory, depth)) = queue.pop_front() {
        let now_ms = elapsed_ms();
        if now_ms >= options.max_duration_ms {
            push_unique(&mut state.stop_reasons, "max-duration-reached");
            break;
        }
        if checkpoints_enabled {
            let force_checkpoint = !cadence.emitted;
            maybe_emit_checkpoint(
                context,
                &state,
                &mut cadence,
                now_ms,
                force_checkpoint,
                emit,
            )?;
        }
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) => {
                record_issue(
                    &mut state.issues,
                    &mut state.skipped_entries,
                    options.max_issues,
                    &root_path,
                    &directory,
                    "read-directory-failed",
                    stable_io_reason(&error),
                );
                push_unique(&mut state.stop_reasons, "entry-errors");
                continue;
            }
        };
        let mut entries = entries;
        loop {
            let now_ms = elapsed_ms();
            if checkpoints_enabled {
                maybe_emit_checkpoint(context, &state, &mut cadence, now_ms, false, emit)?;
            }
            let Some(entry) = entries.next() else {
                break;
            };
            if elapsed_ms() >= options.max_duration_ms {
                push_unique(&mut state.stop_reasons, "max-duration-reached");
                break 'directories;
            }
            if state.visited_entries >= options.max_entries {
                push_unique(&mut state.stop_reasons, "max-entries-reached");
                break 'directories;
            }
            state.visited_entries = state.visited_entries.saturating_add(1);
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    record_issue(
                        &mut state.issues,
                        &mut state.skipped_entries,
                        options.max_issues,
                        &root_path,
                        &directory,
                        "read-entry-failed",
                        stable_io_reason(&error),
                    );
                    push_unique(&mut state.stop_reasons, "entry-errors");
                    continue;
                }
            };
            let path = entry.path();
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(error) => {
                    record_issue(
                        &mut state.issues,
                        &mut state.skipped_entries,
                        options.max_issues,
                        &root_path,
                        &path,
                        "read-metadata-failed",
                        stable_io_reason(&error),
                    );
                    push_unique(&mut state.stop_reasons, "entry-errors");
                    continue;
                }
            };
            let file_type = metadata.file_type();
            if file_type.is_symlink() {
                record_issue(
                    &mut state.issues,
                    &mut state.skipped_entries,
                    options.max_issues,
                    &root_path,
                    &path,
                    "symlink-skipped",
                    "policy-not-followed",
                );
                continue;
            }
            if file_type.is_dir() {
                state.visited_directories = state.visited_directories.saturating_add(1);
                if depth < options.max_depth {
                    queue.push_back((path, depth + 1));
                } else {
                    push_unique(&mut state.stop_reasons, "max-depth-reached");
                }
                continue;
            }
            if !file_type.is_file() {
                record_issue(
                    &mut state.issues,
                    &mut state.skipped_entries,
                    options.max_issues,
                    &root_path,
                    &path,
                    "unsupported-entry-type",
                    "policy-not-file-or-directory",
                );
                continue;
            }

            state.visited_files = state.visited_files.saturating_add(1);
            let Some(local_bytes) = allocated_bytes(&metadata) else {
                record_issue(
                    &mut state.issues,
                    &mut state.skipped_entries,
                    options.max_issues,
                    &root_path,
                    &path,
                    "allocation-evidence-unavailable",
                    "platform-unsupported",
                );
                push_unique(
                    &mut state.stop_reasons,
                    "allocated-byte-evidence-unavailable",
                );
                continue;
            };
            if local_bytes == 0 || local_bytes < options.min_allocated_bytes {
                continue;
            }
            state.allocated_candidate_bytes =
                state.allocated_candidate_bytes.saturating_add(local_bytes);
            state
                .candidates
                .push(candidate(&path, &metadata, local_bytes));
        }
    }

    Ok(inventory_report(context, &state, false))
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
            max_issues: 10,
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

        assert_eq!(report.version, 2);
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
    fn exact_entry_limit_is_complete_when_no_additional_entry_exists() {
        let temp = tempfile::tempdir().unwrap();
        for index in 0..3 {
            write_file(&temp.path().join(format!("{index}.bin")), 4096);
        }
        let mut exact = options();
        exact.max_entries = 3;
        let report = inventory_cloud_local_allocations(&root(temp.path()), exact, 1).unwrap();
        assert_eq!(report.visited_entries, 3);
        assert!(report.evidence_complete);
        assert!(report.stop_reasons.is_empty());
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
        assert_eq!(report.version, 2);
        assert_eq!(report.observed_at_ms, 99);
        assert!(!report.evidence_complete);
        assert_eq!(report.stop_reasons, vec!["hard-timeout-reached"]);
        assert_eq!(report.visited_entries, 0);
        assert_eq!(report.allocated_candidate_bytes, 0);
        assert!(report.candidates.is_empty());
        assert!(report.issues.is_empty());
        assert!(!report.issues_truncated);
        assert_eq!(report.options.max_issues, 10);
        assert!(report.notices.contains(&"worker-hard-timeout".to_string()));
    }

    #[test]
    fn checkpoints_are_nonterminal_and_recover_partial_progress() {
        let temp = tempfile::tempdir().unwrap();
        for index in 0..260 {
            write_file(&temp.path().join(format!("{index:03}.bin")), 4096);
        }
        let root = root(temp.path());
        let mut bounded = options();
        bounded.max_entries = 400;
        let mut checkpoints = Vec::new();
        let report =
            inventory_cloud_local_allocations_with_checkpoints(&root, bounded, 123, |checkpoint| {
                checkpoints.push(checkpoint.clone());
                Ok(())
            })
            .unwrap();

        assert!(report.evidence_complete);
        assert_eq!(report.visited_entries, 260);
        assert!(checkpoints.len() >= 2);
        assert_eq!(checkpoints[0].visited_entries, 0);
        assert!(checkpoints
            .windows(2)
            .all(|pair| pair[0].visited_entries <= pair[1].visited_entries));
        let checkpoint = checkpoints.last().unwrap().clone();
        assert!(checkpoint.visited_entries >= CHECKPOINT_ENTRY_INTERVAL);
        assert!(!checkpoint.evidence_complete);
        assert!(checkpoint
            .notices
            .contains(&"inventory-checkpoint-not-terminal".to_string()));

        let recovered =
            hard_timeout_inventory_from_checkpoint(&root, bounded, checkpoint.clone()).unwrap();
        assert_eq!(recovered.visited_entries, checkpoint.visited_entries);
        assert_eq!(recovered.visited_files, checkpoint.visited_files);
        assert_eq!(
            recovered.allocated_candidate_bytes,
            checkpoint.allocated_candidate_bytes
        );
        assert_eq!(recovered.candidates, checkpoint.candidates);
        assert!(!recovered.evidence_complete);
        assert!(recovered
            .stop_reasons
            .contains(&"hard-timeout-reached".to_string()));
        assert!(recovered
            .notices
            .contains(&"partial-inventory-recovered-from-worker-checkpoint".to_string()));
        assert!(!recovered
            .notices
            .contains(&"inventory-checkpoint-not-terminal".to_string()));
    }

    #[test]
    fn checkpoint_recovery_rejects_scope_or_option_drift() {
        let root = root(Path::new("/Cloud"));
        let mut checkpoint = hard_timeout_inventory(&root, options(), 1).unwrap();
        checkpoint.stop_reasons.clear();
        checkpoint.notices.clear();
        checkpoint.evidence_complete = false;
        checkpoint.cloud_root_id = "icloud:other".into();
        assert_eq!(
            hard_timeout_inventory_from_checkpoint(&root, options(), checkpoint).unwrap_err(),
            "cloud-local-inventory-checkpoint-invalid"
        );
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
        assert_eq!(report.issues.len(), 1);
        assert_eq!(report.issues[0].kind, "symlink-skipped");
        assert_eq!(report.issues[0].reason, "policy-not-followed");
        assert_eq!(report.issues[0].relative_scope.as_deref(), Some("linked"));
        assert!(!report.issues_truncated);
    }

    #[cfg(unix)]
    #[test]
    fn issue_output_is_bounded_and_accounted_for() {
        use std::os::unix::fs::symlink;

        let cloud = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        for index in 0..3 {
            symlink(outside.path(), cloud.path().join(format!("linked-{index}"))).unwrap();
        }
        let mut bounded = options();
        bounded.max_issues = 1;
        let report = inventory_cloud_local_allocations(&root(cloud.path()), bounded, 1).unwrap();
        assert_eq!(report.skipped_entries, 3);
        assert_eq!(report.issues.len(), 1);
        assert!(report.issues_truncated);
        assert!(report
            .notices
            .contains(&"inventory-issues-truncated".to_string()));
    }
}
