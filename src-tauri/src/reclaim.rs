//! Read-only reclaimability planning.
//!
//! File length and even allocated block counts are not proof of bytes that deletion will free.
//! Hard links, copy-on-write clones, compression, sparse allocation, snapshots, and Trash retention
//! can all separate allocation accounting from physical recovery. This module therefore exposes
//! allocated blocks only as an observation and leaves physical reclaimability unknown until it is
//! measured after the complete destructive lifecycle.

#![deny(missing_docs)]

use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Stable discriminator emitted by every reclaim-plan JSON document.
pub const RECLAIM_PLAN_SCHEMA_KIND: &str = "disksage.reclaim-plan";
/// Maximum number of normalized top-level roots accepted by one plan.
pub const MAX_RECLAIM_PATHS: usize = 1_000;
/// Maximum UTF-8 byte length accepted for one evidence path.
pub const MAX_RECLAIM_PATH_UTF8_BYTES: usize = 4_096;
/// Indicates that no post-operation physical-capacity proof exists.
pub const REASON_PHYSICAL_UNVERIFIED: &str = "physical-reclaimability-unverified";
/// Indicates that copy-on-write or other shared extents remain unproven.
pub const REASON_SHARED_EXTENTS: &str = "shared-extents-or-clones-unproven";
/// Indicates that observed allocated blocks are not a reclaimability proof.
pub const REASON_ALLOCATED_NOT_PROOF: &str = "allocated-bytes-are-not-reclaimability-proof";
/// Indicates that the platform did not expose allocated-block accounting.
pub const REASON_ALLOCATED_UNAVAILABLE: &str = "allocated-size-unavailable";
/// Indicates that one or more entries could not be included in complete evidence.
pub const REASON_EVIDENCE_INCOMPLETE: &str = "evidence-incomplete-skipped-entries";
/// Indicates that moving an item to Trash does not immediately return its blocks.
pub const REASON_TRASH_RETAINS: &str = "trash-retains-bytes-until-emptied";
/// Bounded timeout used by the optional active-use probe.
pub const ACTIVE_USE_PROBE_TIMEOUT_MS: u64 = 2_000;
/// Maximum process identifiers retained by the optional active-use probe.
pub const ACTIVE_USE_PROBE_MAX_PIDS: usize = 128;

/// Destructive lifecycle whose consequences the read-only plan is estimating.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlannedOperation {
    /// Move the selected paths to the operating-system Trash.
    Trash,
    /// Permanently remove the selected paths after an independent approval boundary.
    ///
    /// This variant changes only explanatory reason codes. This module never deletes or mutates a
    /// path.
    Delete,
}

impl FromStr for PlannedOperation {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "trash" => Ok(Self::Trash),
            "delete" => Ok(Self::Delete),
            other => Err(format!(
                "unsupported operation: {other}; expected trash or delete"
            )),
        }
    }
}

/// Filesystem object type observed at a normalized top-level root.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RootKind {
    /// A regular-file root.
    File,
    /// A directory root traversed without following symbolic links.
    Directory,
}

/// Confidence state for the physical-reclaimability claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReclaimabilityStatus {
    /// No post-operation free-space or filesystem-native reclaim proof exists.
    Unverified,
}

/// Byte evidence for one path or for the complete normalized selection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReclaimEstimate {
    /// Sum of regular-file lengths after normalized-root traversal.
    pub logical_bytes: u64,
    /// Observed allocated blocks after deduplicating observable hard-link identities.
    ///
    /// Copy-on-write shared extents remain counted once per inode, so this value is not proof of
    /// capacity that an operation will return to the filesystem.
    pub allocated_bytes: Option<u64>,
    /// Verified bytes returned to free capacity after the complete destructive lifecycle.
    ///
    /// The planner intentionally emits `None`; only post-operation free-space evidence or a
    /// filesystem-native proof may populate this claim in a separate receipt contract.
    pub physically_reclaimable_bytes: Option<u64>,
    /// Current confidence state for `physically_reclaimable_bytes`.
    pub status: ReclaimabilityStatus,
    /// Stable machine-readable explanations for uncertainty and accounting limits.
    pub reason_codes: Vec<String>,
}

/// Evidence and traversal counters for one normalized top-level root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PathReclaimEstimate {
    /// Exact canonical local path shown to the operator who requested the plan.
    ///
    /// This field is local private evidence. Callers must redact or omit it before telemetry,
    /// analytics, remote logging, support bundles, or cross-account export.
    pub path: String,
    /// Filesystem object type observed at the root.
    pub kind: RootKind,
    /// Number of regular files observed under the root.
    pub files: u64,
    /// Number of directories observed under the root, including a directory root itself.
    pub dirs: u64,
    /// Number of entries excluded or unreadable during the bounded traversal.
    pub skipped: u64,
    /// Logical, allocation, and physical-reclaimability evidence for this root.
    pub estimate: ReclaimEstimate,
    /// Optional bounded `lsof` evidence. Omitted unless the caller explicitly requests it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_use: Option<crate::git_worktree::GitWorktreeActiveUseEvidence>,
}

/// Optional evidence controls for a reclaim plan.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReclaimPlanOptions {
    /// Collect bounded process/file-use evidence for each normalized root.
    pub include_active_use: bool,
}

/// Read-only reclaim evidence for a normalized, deduplicated path selection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReclaimPlan {
    /// Stable schema discriminator.
    pub schema_kind: &'static str,
    /// Schema revision for compatibility checks.
    pub schema_version: u32,
    /// Destructive lifecycle whose consequences were considered.
    pub operation: PlannedOperation,
    /// Per-root evidence in deterministic normalized-root order.
    pub paths: Vec<PathReclaimEstimate>,
    /// Aggregate evidence across all normalized roots.
    pub totals: ReclaimEstimate,
}

/// Platform-native identity used to deduplicate hard-linked filesystem objects.
#[cfg(unix)]
type FileIdentity = (u64, u64);

/// Placeholder identity on platforms without Unix device-and-inode accounting.
#[cfg(not(unix))]
type FileIdentity = ();

/// Mutable counters collected while scanning one root or the complete selection.
#[derive(Debug)]
struct Accumulator {
    logical_bytes: u64,
    allocated_bytes: Option<u64>,
    files: u64,
    dirs: u64,
    skipped: u64,
    seen_files: HashSet<FileIdentity>,
}

impl Accumulator {
    /// Creates an empty accumulator with platform-appropriate allocation support.
    fn new() -> Self {
        Self {
            logical_bytes: 0,
            allocated_bytes: initial_allocated_bytes(),
            files: 0,
            dirs: 0,
            skipped: 0,
            seen_files: HashSet::new(),
        }
    }

    /// Adds one regular file while deduplicating its observable allocation identity.
    fn record_file(&mut self, metadata: &std::fs::Metadata) {
        self.files = self.files.saturating_add(1);
        self.logical_bytes = self.logical_bytes.saturating_add(metadata.len());
        record_allocated_bytes(metadata, &mut self.seen_files, &mut self.allocated_bytes);
    }

    /// Adds one directory's allocation metadata and directory counter.
    fn record_dir(&mut self, metadata: &std::fs::Metadata) {
        self.dirs = self.dirs.saturating_add(1);
        record_allocated_bytes(metadata, &mut self.seen_files, &mut self.allocated_bytes);
    }
}

/// Initializes allocated-byte accounting on Unix platforms.
#[cfg(unix)]
fn initial_allocated_bytes() -> Option<u64> {
    Some(0)
}

/// Marks allocated-byte accounting unavailable on non-Unix platforms.
#[cfg(not(unix))]
fn initial_allocated_bytes() -> Option<u64> {
    None
}

/// Adds Unix allocated blocks once for each device-and-inode identity.
#[cfg(unix)]
fn record_allocated_bytes(
    metadata: &std::fs::Metadata,
    seen: &mut HashSet<FileIdentity>,
    total: &mut Option<u64>,
) {
    use std::os::unix::fs::MetadataExt;

    if !seen.insert((metadata.dev(), metadata.ino())) {
        return;
    }
    if let Some(value) = total.as_mut() {
        *value = value.saturating_add(metadata.blocks().saturating_mul(512));
    }
}

/// Keeps allocated-byte accounting unavailable when the platform lacks a supported identity.
#[cfg(not(unix))]
fn record_allocated_bytes(
    _metadata: &std::fs::Metadata,
    _seen: &mut HashSet<FileIdentity>,
    total: &mut Option<u64>,
) {
    *total = None;
}

/// Builds the stable reason-code set for one reclaim estimate.
fn reason_codes(
    operation: PlannedOperation,
    allocation_available: bool,
    skipped_entries: u64,
) -> Vec<String> {
    let mut reasons = vec![
        REASON_PHYSICAL_UNVERIFIED.to_string(),
        REASON_SHARED_EXTENTS.to_string(),
    ];
    if allocation_available {
        reasons.push(REASON_ALLOCATED_NOT_PROOF.to_string());
    } else {
        reasons.push(REASON_ALLOCATED_UNAVAILABLE.to_string());
    }
    if skipped_entries > 0 {
        reasons.push(REASON_EVIDENCE_INCOMPLETE.to_string());
    }
    if operation == PlannedOperation::Trash {
        reasons.push(REASON_TRASH_RETAINS.to_string());
    }
    reasons
}

/// Converts scan counters into the immutable external estimate contract.
fn estimate(acc: &Accumulator, operation: PlannedOperation) -> ReclaimEstimate {
    ReclaimEstimate {
        logical_bytes: acc.logical_bytes,
        allocated_bytes: acc.allocated_bytes,
        physically_reclaimable_bytes: None,
        status: ReclaimabilityStatus::Unverified,
        reason_codes: reason_codes(operation, acc.allocated_bytes.is_some(), acc.skipped),
    }
}

/// Validates and returns a bounded UTF-8 path for local private evidence.
fn validated_evidence_path(path: &Path) -> Result<String, String> {
    let value = path
        .to_str()
        .ok_or_else(|| "reclaim-plan paths must be valid UTF-8".to_string())?;
    if value.is_empty() {
        return Err("reclaim-plan paths must not be empty".to_string());
    }
    if value.chars().any(char::is_control) {
        return Err("reclaim-plan paths must not contain control characters".to_string());
    }
    if value.len() > MAX_RECLAIM_PATH_UTF8_BYTES {
        return Err(format!(
            "reclaim-plan paths must not exceed {MAX_RECLAIM_PATH_UTF8_BYTES} UTF-8 bytes"
        ));
    }
    Ok(value.to_string())
}

/// Rejects selections whose root count exceeds the bounded planning contract.
fn validate_root_count(roots: &[PathBuf]) -> Result<(), String> {
    if roots.len() > MAX_RECLAIM_PATHS {
        return Err(format!(
            "reclaim plans support at most {MAX_RECLAIM_PATHS} normalized roots"
        ));
    }
    Ok(())
}

/// Canonicalizes, deduplicates, and removes roots already covered by a parent directory.
fn normalize_roots(raw_paths: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    if raw_paths.is_empty() {
        return Err("at least one path is required".to_string());
    }
    validate_root_count(raw_paths)?;

    let mut paths = Vec::with_capacity(raw_paths.len());
    for raw in raw_paths {
        validated_evidence_path(raw)?;
        let metadata = std::fs::symlink_metadata(raw)
            .map_err(|error| format!("cannot inspect {}: {error}", raw.display()))?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "symbolic-link roots are not accepted: {}",
                raw.display()
            ));
        }
        if !metadata.is_file() && !metadata.is_dir() {
            return Err(format!("unsupported path type: {}", raw.display()));
        }
        let canonical = raw
            .canonicalize()
            .map_err(|error| format!("cannot canonicalize {}: {error}", raw.display()))?;
        validated_evidence_path(&canonical)?;
        paths.push(canonical);
    }

    paths.sort();
    paths.dedup();

    let mut roots: Vec<PathBuf> = Vec::new();
    for path in paths {
        let covered = roots
            .iter()
            .any(|root| root.is_dir() && path.starts_with(root));
        if !covered {
            roots.push(path);
        }
    }
    validate_root_count(&roots)?;
    Ok(roots)
}

/// Records a file in both its root-local and selection-wide accumulators.
fn record_for_both(
    metadata: &std::fs::Metadata,
    local: &mut Accumulator,
    totals: &mut Accumulator,
) {
    local.record_file(metadata);
    totals.record_file(metadata);
}

/// Records a directory in both its root-local and selection-wide accumulators.
fn record_dir_for_both(
    metadata: &std::fs::Metadata,
    local: &mut Accumulator,
    totals: &mut Accumulator,
) {
    local.record_dir(metadata);
    totals.record_dir(metadata);
}

/// Scans one normalized root without following links or mutating filesystem contents.
fn scan_root(
    root: &Path,
    operation: PlannedOperation,
    totals: &mut Accumulator,
    options: ReclaimPlanOptions,
) -> Result<PathReclaimEstimate, String> {
    let metadata = std::fs::metadata(root)
        .map_err(|error| format!("cannot inspect {}: {error}", root.display()))?;
    let mut local = Accumulator::new();

    let kind = if metadata.is_file() {
        record_for_both(&metadata, &mut local, totals);
        RootKind::File
    } else {
        let filtered_entries = Arc::new(AtomicU64::new(0));
        let filtered_entries_for_walk = Arc::clone(&filtered_entries);
        let walker = walkdir::WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_entry(move |entry| {
                if entry.depth() == 0 {
                    return true;
                }
                let keep = crate::scanner::keep_entry(entry);
                if !keep {
                    filtered_entries_for_walk.fetch_add(1, Ordering::Relaxed);
                }
                keep
            });

        for entry in walker {
            let Ok(entry) = entry else {
                local.skipped = local.skipped.saturating_add(1);
                totals.skipped = totals.skipped.saturating_add(1);
                continue;
            };
            if entry.file_type().is_dir() {
                match entry.metadata() {
                    Ok(metadata) => record_dir_for_both(&metadata, &mut local, totals),
                    Err(_) => {
                        local.skipped = local.skipped.saturating_add(1);
                        totals.skipped = totals.skipped.saturating_add(1);
                    }
                }
            } else if entry.file_type().is_file() {
                match entry.metadata() {
                    Ok(metadata) => record_for_both(&metadata, &mut local, totals),
                    Err(_) => {
                        local.skipped = local.skipped.saturating_add(1);
                        totals.skipped = totals.skipped.saturating_add(1);
                    }
                }
            }
        }
        let filtered = filtered_entries.load(Ordering::Relaxed);
        local.skipped = local.skipped.saturating_add(filtered);
        totals.skipped = totals.skipped.saturating_add(filtered);
        if local.dirs == 0 {
            return Err(format!(
                "directory root became unavailable while scanning: {}",
                root.display()
            ));
        }
        RootKind::Directory
    };

    let active_use = options.include_active_use.then(|| {
        crate::git_worktree::active_use_evidence(
            root,
            ACTIVE_USE_PROBE_TIMEOUT_MS,
            ACTIVE_USE_PROBE_MAX_PIDS,
            matches!(kind, RootKind::Directory),
        )
    });

    Ok(PathReclaimEstimate {
        path: validated_evidence_path(root)?,
        kind,
        files: local.files,
        dirs: local.dirs,
        skipped: local.skipped,
        estimate: estimate(&local, operation),
        active_use,
    })
}

/// Builds a read-only plan. It never moves, unlinks, or mutates any supplied path.
pub fn plan_reclaim(
    raw_paths: &[PathBuf],
    operation: PlannedOperation,
) -> Result<ReclaimPlan, String> {
    plan_reclaim_with_options(raw_paths, operation, ReclaimPlanOptions::default())
}

/// Builds a read-only plan with explicit evidence controls.
pub fn plan_reclaim_with_options(
    raw_paths: &[PathBuf],
    operation: PlannedOperation,
    options: ReclaimPlanOptions,
) -> Result<ReclaimPlan, String> {
    let roots = normalize_roots(raw_paths)?;
    let mut totals = Accumulator::new();
    let mut paths = Vec::with_capacity(roots.len());
    for root in roots {
        paths.push(scan_root(&root, operation, &mut totals, options)?);
    }

    Ok(ReclaimPlan {
        schema_kind: RECLAIM_PLAN_SCHEMA_KIND,
        schema_version: 1,
        operation,
        paths,
        totals: estimate(&totals, operation),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn rejects_empty_and_symbolic_link_roots() {
        assert!(plan_reclaim(&[], PlannedOperation::Trash).is_err());

        #[cfg(unix)]
        {
            let temp = tempfile::tempdir().unwrap();
            let file = temp.path().join("file");
            let link = temp.path().join("link");
            fs::write(&file, b"payload").unwrap();
            std::os::unix::fs::symlink(&file, &link).unwrap();
            assert!(plan_reclaim(&[link], PlannedOperation::Trash).is_err());
        }
    }

    #[test]
    fn reports_logical_allocated_and_unknown_physical_bytes() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("payload.bin");
        fs::write(&file, vec![7u8; 8_192]).unwrap();

        let plan = plan_reclaim(&[file], PlannedOperation::Delete).unwrap();
        assert_eq!(plan.schema_kind, RECLAIM_PLAN_SCHEMA_KIND);
        assert_eq!(plan.schema_version, 1);
        assert_eq!(plan.totals.logical_bytes, 8_192);
        assert_eq!(plan.totals.physically_reclaimable_bytes, None);
        assert_eq!(plan.totals.status, ReclaimabilityStatus::Unverified);
        assert!(plan
            .totals
            .reason_codes
            .contains(&REASON_SHARED_EXTENTS.to_string()));
        assert!(!plan
            .totals
            .reason_codes
            .contains(&REASON_TRASH_RETAINS.to_string()));
        #[cfg(unix)]
        assert!(plan.totals.allocated_bytes.unwrap() > 0);

        let json = serde_json::to_value(&plan).unwrap();
        assert_eq!(json["schema_kind"], "disksage.reclaim-plan");
        assert_eq!(json["schema_version"], 1);
        assert!(json["paths"][0].get("active_use").is_none());
    }

    #[test]
    fn active_use_evidence_is_opt_in_and_path_local() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("payload.bin");
        fs::write(&file, b"payload").unwrap();

        let plan = plan_reclaim_with_options(
            &[file],
            PlannedOperation::Trash,
            ReclaimPlanOptions {
                include_active_use: true,
            },
        )
        .unwrap();
        let evidence = plan.paths[0].active_use.as_ref().unwrap();
        assert!(evidence.evidence_complete || evidence.error.is_some());
        assert_eq!(evidence.method, "lsof-file-pid+ps-argv");
        assert!(evidence.observed_pids.len() <= ACTIVE_USE_PROBE_MAX_PIDS);
    }

    #[test]
    fn nested_selected_paths_are_counted_once() {
        let temp = tempfile::tempdir().unwrap();
        let child = temp.path().join("child.bin");
        fs::write(&child, vec![1u8; 1_024]).unwrap();

        let plan =
            plan_reclaim(&[temp.path().to_path_buf(), child], PlannedOperation::Trash).unwrap();

        assert_eq!(plan.paths.len(), 1);
        assert_eq!(plan.totals.logical_bytes, 1_024);
        assert!(plan
            .totals
            .reason_codes
            .contains(&REASON_TRASH_RETAINS.to_string()));
    }

    #[cfg(unix)]
    #[test]
    fn hard_link_allocation_is_not_double_counted() {
        use std::os::unix::fs::MetadataExt;

        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("first.bin");
        let second = temp.path().join("second.bin");
        fs::write(&first, vec![5u8; 4_096]).unwrap();
        fs::hard_link(&first, &second).unwrap();
        let expected_allocated = fs::metadata(&first).unwrap().blocks() * 512;

        let plan = plan_reclaim(&[first, second], PlannedOperation::Delete).unwrap();

        assert_eq!(plan.totals.logical_bytes, 8_192);
        assert_eq!(plan.totals.allocated_bytes, Some(expected_allocated));
    }

    #[test]
    fn operation_parser_is_bounded() {
        assert_eq!("trash".parse(), Ok(PlannedOperation::Trash));
        assert_eq!("delete".parse(), Ok(PlannedOperation::Delete));
        assert!("move".parse::<PlannedOperation>().is_err());
    }

    #[test]
    fn evidence_paths_and_normalized_root_count_are_bounded() {
        let boundary = PathBuf::from("x".repeat(MAX_RECLAIM_PATH_UTF8_BYTES));
        assert!(validated_evidence_path(&boundary).is_ok());

        let too_long = PathBuf::from("x".repeat(MAX_RECLAIM_PATH_UTF8_BYTES + 1));
        assert!(validated_evidence_path(&too_long).is_err());
        assert!(validated_evidence_path(Path::new("safe\nunsafe")).is_err());
        assert!(validated_evidence_path(Path::new("")).is_err());

        let at_limit: Vec<PathBuf> = (0..MAX_RECLAIM_PATHS)
            .map(|index| PathBuf::from(format!("root-{index}")))
            .collect();
        assert!(validate_root_count(&at_limit).is_ok());

        let roots: Vec<PathBuf> = (0..=MAX_RECLAIM_PATHS)
            .map(|index| PathBuf::from(format!("root-{index}")))
            .collect();
        assert!(validate_root_count(&roots).is_err());

        let raw_roots = vec![PathBuf::from("missing-root"); MAX_RECLAIM_PATHS + 1];
        assert_eq!(
            normalize_roots(&raw_roots).unwrap_err(),
            format!("reclaim plans support at most {MAX_RECLAIM_PATHS} normalized roots")
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_non_utf8_evidence_paths() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let path = PathBuf::from(OsString::from_vec(vec![b'f', 0x80]));
        assert!(validated_evidence_path(&path).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn filtered_symbolic_links_are_reported_as_incomplete_evidence() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("target.bin");
        let link = temp.path().join("link.bin");
        fs::write(&target, b"payload").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();

        let plan = plan_reclaim(&[temp.path().to_path_buf()], PlannedOperation::Delete).unwrap();

        assert_eq!(plan.paths[0].files, 1);
        assert_eq!(plan.paths[0].dirs, 1);
        assert_eq!(plan.paths[0].skipped, 1);
        assert!(plan.paths[0]
            .estimate
            .reason_codes
            .contains(&REASON_EVIDENCE_INCOMPLETE.to_string()));
        assert!(plan
            .totals
            .reason_codes
            .contains(&REASON_EVIDENCE_INCOMPLETE.to_string()));
    }
}
