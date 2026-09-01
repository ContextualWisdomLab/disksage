//! Exact-child, age/owner/active-use-bound reclaim for the system temporary directory.

use crate::git_worktree::{active_use_evidence, size_evidence, GitWorktreeActiveUseEvidence};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const SCHEMA_VERSION: u32 = 1;
const MAX_CHILDREN: usize = 10_000;
const MAX_AGE_SECONDS: u64 = 365 * 86_400;
const REMOVAL_UNAVAILABLE: &str = "temp-reclaim-removal-private-approval-unavailable";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TempReclaimOptions {
    pub min_age_seconds: u64,
    pub max_children: usize,
    pub max_entries_per_child: u64,
    pub scan_timeout_ms: u64,
}

impl Default for TempReclaimOptions {
    fn default() -> Self {
        Self {
            min_age_seconds: 7 * 86_400,
            max_children: 1_000,
            max_entries_per_child: 1_000_000,
            scan_timeout_ms: 30_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TempReclaimCandidate {
    pub ontology_class: String,
    pub path: String,
    pub kind: String,
    pub modified_ms: u64,
    pub age_seconds: u64,
    pub allocated_bytes: u64,
    pub logical_bytes: u64,
    pub device: u64,
    pub inode: u64,
    pub active_use: GitWorktreeActiveUseEvidence,
    pub candidate_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TempReclaimPlan {
    pub schema_kind: String,
    pub schema_version: u32,
    pub ontology_class: String,
    pub root: String,
    pub observed_at_ms: u64,
    pub options: TempReclaimOptions,
    pub candidate_count: usize,
    pub candidate_allocated_bytes: u64,
    pub candidates: Vec<TempReclaimCandidate>,
    pub skipped_count: usize,
    pub issues: Vec<String>,
    pub evidence_complete: bool,
    pub candidate_set_fingerprint: String,
    pub exact_approval_phrase: Option<String>,
    pub filesystem_mutation_executed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TempReclaimRemoval {
    pub schema_version: u32,
    pub ontology_class: &'static str,
    pub candidate_set_fingerprint: String,
    pub removed_count: usize,
    pub removed_allocated_bytes_upper_bound: u64,
    pub rationale: String,
    pub executed_at_ms: u64,
    pub filesystem_mutation_executed: bool,
    pub recoverability: &'static str,
}

fn validate_options(options: TempReclaimOptions) -> Result<(), String> {
    if options.min_age_seconds == 0
        || options.min_age_seconds > MAX_AGE_SECONDS
        || options.max_children == 0
        || options.max_children > MAX_CHILDREN
        || options.max_entries_per_child == 0
        || options.max_entries_per_child > 20_000_000
        || options.scan_timeout_ms == 0
        || options.scan_timeout_ms > 300_000
    {
        return Err("temp-reclaim-options-invalid".into());
    }
    Ok(())
}

fn canonical_temp_root(root: &Path) -> Result<PathBuf, String> {
    if !root.is_absolute()
        || root
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("temp-reclaim-root-invalid".into());
    }
    let canonical = fs::canonicalize(root).map_err(|_| "temp-reclaim-root-unavailable")?;
    #[cfg(target_os = "windows")]
    let expected = fs::canonicalize(std::env::temp_dir())
        .map_err(|_| "temp-reclaim-system-temp-unavailable".to_string())?;
    #[cfg(target_os = "macos")]
    let expected = PathBuf::from("/private/tmp");
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    let expected = PathBuf::from("/tmp");
    if canonical != expected {
        return Err("temp-reclaim-root-not-system-temp".into());
    }
    Ok(canonical)
}

#[cfg(unix)]
fn identity(metadata: &fs::Metadata) -> (u32, u64, u64) {
    use std::os::unix::fs::MetadataExt;
    (metadata.uid(), metadata.dev(), metadata.ino())
}

#[cfg(not(unix))]
fn identity(_metadata: &fs::Metadata) -> (u32, u64, u64) {
    (u32::MAX, 0, 0)
}

#[cfg(unix)]
fn current_uid() -> u32 {
    unsafe { libc::geteuid() }
}

#[cfg(not(unix))]
fn current_uid() -> u32 {
    u32::MAX
}

fn candidate_fingerprint(
    path: &str,
    kind: &str,
    modified_ms: u64,
    allocated_bytes: u64,
    device: u64,
    inode: u64,
) -> String {
    let mut hasher = blake3::Hasher::new();
    for value in [
        "disksage.temp-candidate.v1".to_string(),
        path.to_string(),
        kind.to_string(),
        modified_ms.to_string(),
        allocated_bytes.to_string(),
        device.to_string(),
        inode.to_string(),
    ] {
        hasher.update(&(value.len() as u64).to_le_bytes());
        hasher.update(value.as_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

fn candidate_set_fingerprint(candidates: &[TempReclaimCandidate]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage.temp-candidate-set.v1");
    for candidate in candidates {
        hasher.update(&(candidate.candidate_fingerprint.len() as u64).to_le_bytes());
        hasher.update(candidate.candidate_fingerprint.as_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

pub fn plan_temp_reclaim(
    requested_root: &Path,
    options: TempReclaimOptions,
    observed_at_ms: u64,
) -> Result<TempReclaimPlan, String> {
    validate_options(options)?;
    if observed_at_ms == 0 {
        return Err("temp-reclaim-time-invalid".into());
    }
    let root = canonical_temp_root(requested_root)?;
    let mut entries = fs::read_dir(&root)
        .map_err(|_| "temp-reclaim-root-unreadable".to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "temp-reclaim-root-entry-unreadable".to_string())?;
    entries.sort_by_key(|entry| entry.file_name());
    let results_truncated = entries.len() > options.max_children;
    entries.truncate(options.max_children);
    let current_uid = current_uid();
    let actor_cwd = std::env::current_dir()
        .ok()
        .and_then(|path| fs::canonicalize(path).ok());
    let mut candidates = Vec::new();
    let mut issues = Vec::new();
    let mut skipped_count = 0usize;
    for entry in entries {
        let path = entry.path();
        let metadata = match fs::symlink_metadata(&path) {
            Ok(value) => value,
            Err(_) => {
                skipped_count += 1;
                issues.push("temp-child-metadata-unavailable".into());
                continue;
            }
        };
        let file_type = metadata.file_type();
        let (owner, device, inode) = identity(&metadata);
        let modified_ms = metadata
            .modified()
            .ok()
            .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
            .and_then(|value| u64::try_from(value.as_millis()).ok())
            .unwrap_or(observed_at_ms);
        let age_seconds = observed_at_ms.saturating_sub(modified_ms) / 1_000;
        if file_type.is_symlink()
            || (!file_type.is_file() && !file_type.is_dir())
            || owner != current_uid
            || age_seconds < options.min_age_seconds
            || actor_cwd.as_ref().is_some_and(|cwd| cwd.starts_with(&path))
        {
            continue;
        }
        let size = size_evidence(
            &path,
            options.max_entries_per_child,
            options.scan_timeout_ms,
        );
        let active_use =
            active_use_evidence(&path, options.scan_timeout_ms, 64, file_type.is_dir());
        if !size.evidence_complete
            || !active_use.assessed
            || !active_use.evidence_complete
            || active_use.active
        {
            skipped_count += 1;
            issues.push("temp-child-evidence-incomplete-or-active".into());
            continue;
        }
        let kind = if file_type.is_dir() {
            "directory"
        } else {
            "file"
        };
        let path_string = path.to_string_lossy().into_owned();
        candidates.push(TempReclaimCandidate {
            ontology_class: "https://disksage.app/ontology#TemporaryArtifact".into(),
            candidate_fingerprint: candidate_fingerprint(
                &path_string,
                kind,
                modified_ms,
                size.allocated_bytes,
                device,
                inode,
            ),
            path: path_string,
            kind: kind.into(),
            modified_ms,
            age_seconds,
            allocated_bytes: size.allocated_bytes,
            logical_bytes: size.logical_bytes,
            device,
            inode,
            active_use,
        });
    }
    candidates.sort_by(|left, right| left.path.cmp(&right.path));
    issues.sort();
    issues.dedup();
    let candidate_set_fingerprint = candidate_set_fingerprint(&candidates);
    let candidate_allocated_bytes = candidates.iter().fold(0u64, |total, candidate| {
        total.saturating_add(candidate.allocated_bytes)
    });
    let evidence_complete = !results_truncated && skipped_count == 0;
    let exact_approval_phrase = (!candidates.is_empty() && evidence_complete)
        .then(|| format!("DiskSage temporary artifact reclaim 승인 {candidate_set_fingerprint}"));
    Ok(TempReclaimPlan {
        schema_kind: "disksage.temp-reclaim-plan".into(),
        schema_version: SCHEMA_VERSION,
        ontology_class: "https://disksage.app/ontology#TemporaryArtifact".into(),
        root: root.to_string_lossy().into_owned(),
        observed_at_ms,
        options,
        candidate_count: candidates.len(),
        candidate_allocated_bytes,
        candidates,
        skipped_count,
        issues,
        evidence_complete,
        candidate_set_fingerprint,
        exact_approval_phrase,
        filesystem_mutation_executed: false,
    })
}

/// Permanent deletion is deliberately unavailable until DiskSage can bind a private approval to
/// every exact child and move each object through the shared identity-checked Trash boundary.
/// Planning remains available so operators can inspect allocation and active-use evidence without
/// granting mutation authority.
pub fn remove_temp_candidates(
    _requested_root: &Path,
    _options: TempReclaimOptions,
    _expected_candidate_set_fingerprint: &str,
    _confirmation_phrase: &str,
    rationale: &str,
    _executed_at_ms: u64,
) -> Result<TempReclaimRemoval, String> {
    if rationale.trim() != rationale
        || rationale.is_empty()
        || rationale.len() > 1_000
        || rationale.chars().any(char::is_control)
    {
        return Err("temp-reclaim-rationale-invalid".into());
    }
    Err(REMOVAL_UNAVAILABLE.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_identity_changes_the_approval_fingerprint() {
        let first = candidate_fingerprint("/private/tmp/a", "file", 1, 2, 3, 4);
        let second = candidate_fingerprint("/private/tmp/a", "file", 1, 2, 3, 5);
        assert_ne!(first, second);
        assert_eq!(first.len(), 64);
    }

    #[test]
    fn unsafe_or_unbounded_options_are_rejected() {
        let mut options = TempReclaimOptions::default();
        options.min_age_seconds = 0;
        assert!(validate_options(options).is_err());
        options = TempReclaimOptions::default();
        options.max_children = MAX_CHILDREN + 1;
        assert!(validate_options(options).is_err());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_system_temp_root_is_accepted() {
        let requested = std::env::temp_dir();
        let expected = fs::canonicalize(&requested).expect("canonical Windows temp root");
        assert_eq!(canonical_temp_root(&requested).unwrap(), expected);
    }

    #[test]
    fn removal_fails_before_filesystem_observation() {
        assert_eq!(
            remove_temp_candidates(
                Path::new("/not-observed"),
                TempReclaimOptions::default(),
                "fingerprint",
                "phrase",
                "reviewed",
                1,
            ),
            Err(REMOVAL_UNAVAILABLE.into())
        );
    }
}
