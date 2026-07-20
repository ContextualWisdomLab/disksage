//! Read-only reclaimability planning.
//!
//! File length and even allocated block counts are not proof of bytes that deletion will free.
//! Hard links, copy-on-write clones, compression, sparse allocation, snapshots, and Trash retention
//! can all separate allocation accounting from physical recovery. This module therefore exposes
//! allocated blocks only as an observation and leaves physical reclaimability unknown until it is
//! measured after the complete destructive lifecycle.

use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::str::FromStr;

pub const RECLAIM_PLAN_SCHEMA_KIND: &str = "disksage.reclaim-plan";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlannedOperation {
    Trash,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RootKind {
    File,
    Directory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReclaimabilityStatus {
    Unverified,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReclaimEstimate {
    pub logical_bytes: u64,
    /// Observed allocated blocks after deduplicating observable hard-link identities.
    /// Copy-on-write shared extents remain counted once per inode, so this is not reclaim proof.
    pub allocated_bytes: Option<u64>,
    /// Intentionally unknown before an operation and a provider/filesystem free-space observation.
    pub physically_reclaimable_bytes: Option<u64>,
    pub status: ReclaimabilityStatus,
    pub reason_codes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PathReclaimEstimate {
    pub path: String,
    pub kind: RootKind,
    pub files: u64,
    pub dirs: u64,
    pub skipped: u64,
    pub estimate: ReclaimEstimate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReclaimPlan {
    pub schema_kind: &'static str,
    pub schema_version: u32,
    pub operation: PlannedOperation,
    pub paths: Vec<PathReclaimEstimate>,
    pub totals: ReclaimEstimate,
}

#[cfg(unix)]
type FileIdentity = (u64, u64);

#[cfg(not(unix))]
type FileIdentity = ();

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

    fn record_file(&mut self, metadata: &std::fs::Metadata) {
        self.files = self.files.saturating_add(1);
        self.logical_bytes = self.logical_bytes.saturating_add(metadata.len());
        record_allocated_bytes(metadata, &mut self.seen_files, &mut self.allocated_bytes);
    }

    fn record_dir(&mut self, metadata: &std::fs::Metadata) {
        self.dirs = self.dirs.saturating_add(1);
        record_allocated_bytes(metadata, &mut self.seen_files, &mut self.allocated_bytes);
    }
}

#[cfg(unix)]
fn initial_allocated_bytes() -> Option<u64> {
    Some(0)
}

#[cfg(not(unix))]
fn initial_allocated_bytes() -> Option<u64> {
    None
}

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

#[cfg(not(unix))]
fn record_allocated_bytes(
    _metadata: &std::fs::Metadata,
    _seen: &mut HashSet<FileIdentity>,
    total: &mut Option<u64>,
) {
    *total = None;
}

fn reason_codes(operation: PlannedOperation, allocation_available: bool) -> Vec<String> {
    let mut reasons = vec![
        "physical-reclaimability-unverified".to_string(),
        "shared-extents-or-clones-unproven".to_string(),
    ];
    if allocation_available {
        reasons.push("allocated-bytes-are-not-reclaimability-proof".to_string());
    } else {
        reasons.push("allocated-size-unavailable".to_string());
    }
    if operation == PlannedOperation::Trash {
        reasons.push("trash-retains-bytes-until-emptied".to_string());
    }
    reasons
}

fn estimate(acc: &Accumulator, operation: PlannedOperation) -> ReclaimEstimate {
    ReclaimEstimate {
        logical_bytes: acc.logical_bytes,
        allocated_bytes: acc.allocated_bytes,
        physically_reclaimable_bytes: None,
        status: ReclaimabilityStatus::Unverified,
        reason_codes: reason_codes(operation, acc.allocated_bytes.is_some()),
    }
}

fn normalize_roots(raw_paths: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    if raw_paths.is_empty() {
        return Err("at least one path is required".to_string());
    }

    let mut paths = Vec::with_capacity(raw_paths.len());
    for raw in raw_paths {
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
        paths.push(
            raw.canonicalize()
                .map_err(|error| format!("cannot canonicalize {}: {error}", raw.display()))?,
        );
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
    Ok(roots)
}

fn record_for_both(
    metadata: &std::fs::Metadata,
    local: &mut Accumulator,
    totals: &mut Accumulator,
) {
    local.record_file(metadata);
    totals.record_file(metadata);
}

fn record_dir_for_both(
    metadata: &std::fs::Metadata,
    local: &mut Accumulator,
    totals: &mut Accumulator,
) {
    local.record_dir(metadata);
    totals.record_dir(metadata);
}

fn scan_root(
    root: &Path,
    operation: PlannedOperation,
    totals: &mut Accumulator,
) -> Result<PathReclaimEstimate, String> {
    let metadata = std::fs::metadata(root)
        .map_err(|error| format!("cannot inspect {}: {error}", root.display()))?;
    let mut local = Accumulator::new();

    let kind = if metadata.is_file() {
        record_for_both(&metadata, &mut local, totals);
        RootKind::File
    } else {
        let walker = jwalk::WalkDir::new(root)
            .follow_links(false)
            .skip_hidden(false)
            .process_read_dir(|_depth, _path, _state, children| {
                children.retain(|entry| {
                    entry
                        .as_ref()
                        .map(crate::scanner::keep_entry)
                        .unwrap_or(true)
                });
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
                if entry.read_children_error.is_some() {
                    local.skipped = local.skipped.saturating_add(1);
                    totals.skipped = totals.skipped.saturating_add(1);
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
        RootKind::Directory
    };

    Ok(PathReclaimEstimate {
        path: root.to_string_lossy().into_owned(),
        kind,
        files: local.files,
        dirs: local.dirs,
        skipped: local.skipped,
        estimate: estimate(&local, operation),
    })
}

/// Builds a read-only plan. It never moves, unlinks, or mutates any supplied path.
pub fn plan_reclaim(
    raw_paths: &[PathBuf],
    operation: PlannedOperation,
) -> Result<ReclaimPlan, String> {
    let roots = normalize_roots(raw_paths)?;
    let mut totals = Accumulator::new();
    let mut paths = Vec::with_capacity(roots.len());
    for root in roots {
        paths.push(scan_root(&root, operation, &mut totals)?);
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
            .contains(&"shared-extents-or-clones-unproven".to_string()));
        #[cfg(unix)]
        assert!(plan.totals.allocated_bytes.unwrap() > 0);

        let json = serde_json::to_value(&plan).unwrap();
        assert_eq!(json["schema_kind"], "disksage.reclaim-plan");
        assert_eq!(json["schema_version"], 1);
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
            .contains(&"trash-retains-bytes-until-emptied".to_string()));
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
}
