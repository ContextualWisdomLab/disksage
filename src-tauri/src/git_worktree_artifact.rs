//! Read-only, fingerprint-bound audit of regenerable artifacts in stale Git worktrees.
//!
//! The source worktree audit remains authoritative. Only its exact removal candidates are scanned,
//! so tracked/untracked cleanliness, retention reachability, active CWD, and open-file evidence are
//! inherited without weakening those gates. This module never deletes an artifact or a worktree.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use crate::git_worktree::{
    bounded_size_evidence, GitWorktreeAuditEntry, GitWorktreeAuditReport, GitWorktreeDisposition,
    GitWorktreeSizeEvidence, GIT_WORKTREE_AUDIT_SCHEMA_KIND,
};

pub const GIT_WORKTREE_ARTIFACT_AUDIT_SCHEMA_KIND: &str = "disksage.git-worktree-artifact-audit/v1";
const GIT_WORKTREE_ARTIFACT_AUDIT_VERSION: u32 = 1;

const ARTIFACT_SPECS: &[(&str, &[&str])] = &[
    ("node_modules", &["package.json"]),
    ("target", &["Cargo.toml"]),
    (".venv", &["pyproject.toml", "requirements.txt", "setup.py"]),
    ("venv", &["pyproject.toml", "requirements.txt", "setup.py"]),
    ("__pycache__", &[]),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitWorktreeArtifactAuditOptions {
    pub discovery_timeout_ms: u64,
    pub max_discovery_entries_per_worktree: u64,
    pub size_scan_timeout_ms: u64,
    pub max_entries_per_artifact: u64,
    pub max_artifacts: usize,
}

impl Default for GitWorktreeArtifactAuditOptions {
    fn default() -> Self {
        Self {
            discovery_timeout_ms: 30_000,
            max_discovery_entries_per_worktree: 250_000,
            size_scan_timeout_ms: 60_000,
            max_entries_per_artifact: 2_000_000,
            max_artifacts: 4_096,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GitWorktreeArtifactDisposition {
    ReclaimCandidate,
    EvidenceGap,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitWorktreeArtifactAuditEntry {
    pub path: String,
    pub path_fingerprint: String,
    pub worktree_path_fingerprint: String,
    pub worktree_entry_fingerprint: String,
    pub kind: String,
    pub size: GitWorktreeSizeEvidence,
    pub disposition: GitWorktreeArtifactDisposition,
    pub blockers: Vec<String>,
    pub entry_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitWorktreeArtifactAuditReport {
    pub schema_kind: String,
    pub version: u32,
    pub generated_at_ms: u64,
    pub source_worktree_audit_schema_kind: String,
    pub source_worktree_removal_plan_fingerprint: String,
    pub retention_reference_set_fingerprint: String,
    pub candidate_worktree_count: usize,
    pub scanned_worktree_count: usize,
    pub artifact_count: usize,
    pub reclaim_candidate_count: usize,
    pub reclaim_candidate_allocated_bytes: u64,
    pub reclaim_candidate_logical_bytes: u64,
    pub evidence_gap_count: usize,
    pub evidence_complete: bool,
    pub cleanup_plan_fingerprint: String,
    pub exact_approval_phrase: Option<String>,
    pub entries: Vec<GitWorktreeArtifactAuditEntry>,
    pub issues: Vec<String>,
    pub filesystem_mutation_executed: bool,
    pub worktree_mutation_executed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct GitWorktreeArtifactAuditPublicSummary {
    pub schema_kind: String,
    pub version: u32,
    pub generated_at_ms: u64,
    pub source_worktree_audit_schema_kind: String,
    pub source_worktree_removal_plan_fingerprint: String,
    pub retention_reference_set_fingerprint: String,
    pub candidate_worktree_count: usize,
    pub scanned_worktree_count: usize,
    pub artifact_count: usize,
    pub reclaim_candidate_count: usize,
    pub reclaim_candidate_allocated_bytes: u64,
    pub reclaim_candidate_logical_bytes: u64,
    pub evidence_gap_count: usize,
    pub evidence_complete: bool,
    pub cleanup_plan_fingerprint: String,
    pub exact_approval_phrase: Option<String>,
    pub filesystem_mutation_executed: bool,
    pub worktree_mutation_executed: bool,
    pub local_paths_redacted: bool,
    pub branch_names_redacted: bool,
    pub metadata_semantics: Vec<String>,
    pub notices: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DiscoveredArtifact {
    path: PathBuf,
    kind: &'static str,
}

fn valid_hex64(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn validate_options(options: GitWorktreeArtifactAuditOptions) -> Result<(), String> {
    if options.discovery_timeout_ms == 0
        || options.discovery_timeout_ms > 300_000
        || options.max_discovery_entries_per_worktree == 0
        || options.max_discovery_entries_per_worktree > 5_000_000
        || options.size_scan_timeout_ms == 0
        || options.size_scan_timeout_ms > 300_000
        || options.max_entries_per_artifact == 0
        || options.max_entries_per_artifact > 5_000_000
        || options.max_artifacts == 0
        || options.max_artifacts > 100_000
    {
        return Err("git-worktree-artifact-audit-options-invalid".into());
    }
    Ok(())
}

fn source_candidate_valid(entry: &GitWorktreeAuditEntry) -> bool {
    entry.disposition == GitWorktreeDisposition::RemovalCandidate
        && valid_hex64(&entry.path_fingerprint)
        && valid_hex64(&entry.entry_fingerprint)
        && !entry.primary
        && !entry.audit_origin
        && !entry.bare
        && !entry.locked
        && !entry.prunable
        && entry.status_clean == Some(true)
        && entry.status_entry_count == Some(0)
        && entry.contained_in_reference == Some(true)
        && !entry.head_is_retained_tip
        && entry.actor_cwd_inside == Some(false)
        && entry.size.evidence_complete
        && entry.active_use.assessed
        && entry.active_use.evidence_complete
        && !entry.active_use.active
        && entry.blockers.is_empty()
}

fn source_audit_valid(audit: &GitWorktreeAuditReport) -> bool {
    let candidates: Vec<_> = audit
        .entries
        .iter()
        .filter(|entry| entry.disposition == GitWorktreeDisposition::RemovalCandidate)
        .collect();
    let candidate_allocated = candidates.iter().fold(0u64, |sum, entry| {
        sum.saturating_add(entry.size.allocated_bytes)
    });
    audit.schema_kind == GIT_WORKTREE_AUDIT_SCHEMA_KIND
        && audit.version == 2
        && audit.evidence_complete
        && audit.evidence_gap_count == 0
        && !audit.filesystem_mutation_executed
        && valid_hex64(&audit.retention_reference_set_fingerprint)
        && valid_hex64(&audit.removal_plan_fingerprint)
        && audit.removal_candidate_count == candidates.len()
        && audit.removal_candidate_allocated_bytes == candidate_allocated
        && candidates.iter().all(|entry| source_candidate_valid(entry))
}

fn regular_marker(parent: &Path, marker: &str) -> bool {
    fs::symlink_metadata(parent.join(marker))
        .is_ok_and(|metadata| !metadata.file_type().is_symlink() && metadata.is_file())
}

fn artifact_spec(path: &Path) -> Option<(&'static str, &'static [&'static str])> {
    let name = path.file_name()?;
    ARTIFACT_SPECS
        .iter()
        .find(|(kind, _)| name == OsStr::new(kind))
        .copied()
}

fn discover_artifacts(
    root: &Path,
    options: GitWorktreeArtifactAuditOptions,
) -> Result<Vec<DiscoveredArtifact>, String> {
    let canonical_root =
        fs::canonicalize(root).map_err(|_| "git-worktree-artifact-root-unavailable")?;
    let root_metadata =
        fs::symlink_metadata(root).map_err(|_| "git-worktree-artifact-root-unavailable")?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err("git-worktree-artifact-root-invalid".into());
    }
    let started = Instant::now();
    let mut stack = vec![canonical_root.clone()];
    let mut visited_entries = 0u64;
    let mut artifacts = Vec::new();
    while let Some(path) = stack.pop() {
        if started.elapsed() >= Duration::from_millis(options.discovery_timeout_ms) {
            return Err("git-worktree-artifact-discovery-timeout".into());
        }
        if visited_entries >= options.max_discovery_entries_per_worktree {
            return Err("git-worktree-artifact-discovery-entry-limit".into());
        }
        visited_entries = visited_entries.saturating_add(1);
        let metadata = fs::symlink_metadata(&path)
            .map_err(|_| "git-worktree-artifact-discovery-metadata-unavailable")?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            continue;
        }
        if path != canonical_root {
            if let Some((kind, markers)) = artifact_spec(&path) {
                if markers.is_empty()
                    || path.parent().is_some_and(|parent| {
                        markers.iter().any(|marker| regular_marker(parent, marker))
                    })
                {
                    let canonical = fs::canonicalize(&path)
                        .map_err(|_| "git-worktree-artifact-path-unavailable")?;
                    if !canonical.starts_with(&canonical_root) || canonical == canonical_root {
                        return Err("git-worktree-artifact-path-escape".into());
                    }
                    artifacts.push(DiscoveredArtifact {
                        path: canonical,
                        kind,
                    });
                    if artifacts.len() > options.max_artifacts {
                        return Err("git-worktree-artifact-count-limit".into());
                    }
                }
                // A directory named like a build artifact without a valid adjacent project marker
                // is ambiguous. Preserve it and do not mine nested content for candidates.
                continue;
            }
        }
        let mut children = fs::read_dir(&path)
            .map_err(|_| "git-worktree-artifact-discovery-directory-unreadable")?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "git-worktree-artifact-discovery-entry-unavailable")?;
        children.sort_by_key(|child| child.file_name());
        stack.extend(children.into_iter().rev().map(|child| child.path()));
    }
    artifacts.sort_by(|left, right| left.path.cmp(&right.path));
    artifacts.dedup_by(|left, right| left.path == right.path);
    Ok(artifacts)
}

fn path_fingerprint(worktree_path_fingerprint: &str, kind: &str, path: &Path) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"disksage.git-worktree-artifact-path\0v1\0");
    hasher.update(worktree_path_fingerprint.as_bytes());
    hasher.update([0]);
    hasher.update(kind.as_bytes());
    hasher.update([0]);
    hasher.update(path.as_os_str().as_encoded_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn entry_fingerprint(entry: &GitWorktreeArtifactAuditEntry) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage.git-worktree-artifact-entry\0v1\0");
    for value in [
        entry.path_fingerprint.as_str(),
        entry.worktree_path_fingerprint.as_str(),
        entry.worktree_entry_fingerprint.as_str(),
        entry.kind.as_str(),
        entry.size.method.as_str(),
    ] {
        hasher.update(value.as_bytes());
        hasher.update(&[0]);
    }
    hasher.update(&entry.size.allocated_bytes.to_le_bytes());
    hasher.update(&entry.size.logical_bytes.to_le_bytes());
    hasher.update(&entry.size.visited_entries.to_le_bytes());
    hasher.update(&[u8::from(entry.size.evidence_complete)]);
    hasher.update(&[match entry.disposition {
        GitWorktreeArtifactDisposition::ReclaimCandidate => 1,
        GitWorktreeArtifactDisposition::EvidenceGap => 2,
    }]);
    for blocker in &entry.blockers {
        hasher.update(blocker.as_bytes());
        hasher.update(&[0]);
    }
    hasher.finalize().to_hex().to_string()
}

fn cleanup_plan_fingerprint(
    source_plan_fingerprint: &str,
    reference_set_fingerprint: &str,
    entries: &[GitWorktreeArtifactAuditEntry],
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage.git-worktree-artifact-cleanup-plan\0v1\0");
    hasher.update(source_plan_fingerprint.as_bytes());
    hasher.update(&[0]);
    hasher.update(reference_set_fingerprint.as_bytes());
    hasher.update(&[0]);
    for entry in entries {
        hasher.update(entry.entry_fingerprint.as_bytes());
        hasher.update(&[0]);
    }
    hasher.finalize().to_hex().to_string()
}

/// Audit regenerable artifacts inside exact stale-worktree removal candidates.
///
/// No mtime, filename date, or inferred production time is used. Staleness comes only from the
/// source Git retention-reachability and cleanliness contract.
pub fn audit_git_worktree_artifacts(
    source_audit: &GitWorktreeAuditReport,
    options: GitWorktreeArtifactAuditOptions,
    generated_at_ms: u64,
) -> Result<GitWorktreeArtifactAuditReport, String> {
    validate_options(options)?;
    if generated_at_ms == 0 {
        return Err("git-worktree-artifact-generated-at-invalid".into());
    }
    if !source_audit_valid(source_audit) {
        return Err("git-worktree-artifact-source-audit-invalid".into());
    }
    let mut candidate_worktrees: Vec<_> = source_audit
        .entries
        .iter()
        .filter(|entry| entry.disposition == GitWorktreeDisposition::RemovalCandidate)
        .collect();
    candidate_worktrees.sort_by(|left, right| {
        left.path_fingerprint
            .cmp(&right.path_fingerprint)
            .then_with(|| left.entry_fingerprint.cmp(&right.entry_fingerprint))
    });

    let mut entries = Vec::new();
    let mut issues = Vec::new();
    let mut scanned_worktree_count = 0usize;
    for worktree in &candidate_worktrees {
        let root = Path::new(&worktree.path);
        let discovered = match discover_artifacts(root, options) {
            Ok(discovered) => {
                scanned_worktree_count = scanned_worktree_count.saturating_add(1);
                discovered
            }
            Err(error) => {
                issues.push(error);
                continue;
            }
        };
        for artifact in discovered {
            let size = bounded_size_evidence(
                &artifact.path,
                options.max_entries_per_artifact,
                options.size_scan_timeout_ms,
            );
            let blockers = if size.evidence_complete {
                Vec::new()
            } else {
                vec![size
                    .error
                    .clone()
                    .unwrap_or_else(|| "git-worktree-artifact-size-evidence-incomplete".into())]
            };
            let disposition = if blockers.is_empty() {
                GitWorktreeArtifactDisposition::ReclaimCandidate
            } else {
                GitWorktreeArtifactDisposition::EvidenceGap
            };
            let mut entry = GitWorktreeArtifactAuditEntry {
                path: artifact.path.to_string_lossy().into_owned(),
                path_fingerprint: path_fingerprint(
                    &worktree.path_fingerprint,
                    artifact.kind,
                    &artifact.path,
                ),
                worktree_path_fingerprint: worktree.path_fingerprint.clone(),
                worktree_entry_fingerprint: worktree.entry_fingerprint.clone(),
                kind: artifact.kind.into(),
                size,
                disposition,
                blockers,
                entry_fingerprint: String::new(),
            };
            entry.entry_fingerprint = entry_fingerprint(&entry);
            entries.push(entry);
            if entries.len() > options.max_artifacts {
                return Err("git-worktree-artifact-count-limit".into());
            }
        }
    }
    entries.sort_by(|left, right| {
        left.worktree_path_fingerprint
            .cmp(&right.worktree_path_fingerprint)
            .then_with(|| left.path_fingerprint.cmp(&right.path_fingerprint))
    });
    let reclaim_entries: Vec<_> = entries
        .iter()
        .filter(|entry| entry.disposition == GitWorktreeArtifactDisposition::ReclaimCandidate)
        .collect();
    let reclaim_candidate_allocated_bytes = reclaim_entries.iter().fold(0u64, |sum, entry| {
        sum.saturating_add(entry.size.allocated_bytes)
    });
    let reclaim_candidate_logical_bytes = reclaim_entries.iter().fold(0u64, |sum, entry| {
        sum.saturating_add(entry.size.logical_bytes)
    });
    let evidence_gap_count = issues.len().saturating_add(
        entries
            .iter()
            .filter(|entry| entry.disposition == GitWorktreeArtifactDisposition::EvidenceGap)
            .count(),
    );
    let evidence_complete =
        evidence_gap_count == 0 && scanned_worktree_count == candidate_worktrees.len();
    let cleanup_plan_fingerprint = cleanup_plan_fingerprint(
        &source_audit.removal_plan_fingerprint,
        &source_audit.retention_reference_set_fingerprint,
        &entries,
    );
    let exact_approval_phrase = (evidence_complete && !reclaim_entries.is_empty()).then(|| {
        format!(
            "APPROVE DISKSAGE GENERATED ARTIFACT CLEANUP {} {} {}",
            cleanup_plan_fingerprint,
            reclaim_entries.len(),
            reclaim_candidate_allocated_bytes
        )
    });
    Ok(GitWorktreeArtifactAuditReport {
        schema_kind: GIT_WORKTREE_ARTIFACT_AUDIT_SCHEMA_KIND.into(),
        version: GIT_WORKTREE_ARTIFACT_AUDIT_VERSION,
        generated_at_ms,
        source_worktree_audit_schema_kind: source_audit.schema_kind.clone(),
        source_worktree_removal_plan_fingerprint: source_audit.removal_plan_fingerprint.clone(),
        retention_reference_set_fingerprint: source_audit
            .retention_reference_set_fingerprint
            .clone(),
        candidate_worktree_count: candidate_worktrees.len(),
        scanned_worktree_count,
        artifact_count: entries.len(),
        reclaim_candidate_count: reclaim_entries.len(),
        reclaim_candidate_allocated_bytes,
        reclaim_candidate_logical_bytes,
        evidence_gap_count,
        evidence_complete,
        cleanup_plan_fingerprint,
        exact_approval_phrase,
        entries,
        issues,
        filesystem_mutation_executed: false,
        worktree_mutation_executed: false,
    })
}

pub fn public_summary(
    report: &GitWorktreeArtifactAuditReport,
) -> GitWorktreeArtifactAuditPublicSummary {
    GitWorktreeArtifactAuditPublicSummary {
        schema_kind: report.schema_kind.clone(),
        version: report.version,
        generated_at_ms: report.generated_at_ms,
        source_worktree_audit_schema_kind: report.source_worktree_audit_schema_kind.clone(),
        source_worktree_removal_plan_fingerprint: report
            .source_worktree_removal_plan_fingerprint
            .clone(),
        retention_reference_set_fingerprint: report.retention_reference_set_fingerprint.clone(),
        candidate_worktree_count: report.candidate_worktree_count,
        scanned_worktree_count: report.scanned_worktree_count,
        artifact_count: report.artifact_count,
        reclaim_candidate_count: report.reclaim_candidate_count,
        reclaim_candidate_allocated_bytes: report.reclaim_candidate_allocated_bytes,
        reclaim_candidate_logical_bytes: report.reclaim_candidate_logical_bytes,
        evidence_gap_count: report.evidence_gap_count,
        evidence_complete: report.evidence_complete,
        cleanup_plan_fingerprint: report.cleanup_plan_fingerprint.clone(),
        exact_approval_phrase: report.exact_approval_phrase.clone(),
        filesystem_mutation_executed: report.filesystem_mutation_executed,
        worktree_mutation_executed: report.worktree_mutation_executed,
        local_paths_redacted: true,
        branch_names_redacted: true,
        metadata_semantics: vec![
            "only exact source-audit removal candidates are scanned".into(),
            "target, node_modules, .venv, venv, and __pycache__ require exact structural rules"
                .into(),
            "mtime and filename dates are not treated as production evidence".into(),
            "allocated bytes use bounded filesystem block accounting".into(),
        ],
        notices: vec![
            "dry-run only; no artifact or worktree was removed".into(),
            "private output contains sensitive local paths".into(),
            "the approval phrase is evidence for a separate executor, not execution".into(),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git_worktree::{
        GitWorktreeActiveUseEvidence, GitWorktreeAuditEntry, GitWorktreeReferenceBinding,
    };

    fn hex(value: char) -> String {
        value.to_string().repeat(64)
    }

    fn candidate(path: &Path) -> GitWorktreeAuditEntry {
        GitWorktreeAuditEntry {
            path: path.to_string_lossy().into_owned(),
            path_fingerprint: hex('a'),
            head: "b".repeat(40),
            branch: Some("refs/heads/merged".into()),
            detached: false,
            bare: false,
            primary: false,
            audit_origin: false,
            locked: false,
            lock_reason: None,
            prunable: false,
            prunable_reason: None,
            status_clean: Some(true),
            status_entry_count: Some(0),
            contained_in_reference: Some(true),
            head_is_retained_tip: false,
            actor_cwd_inside: Some(false),
            size: GitWorktreeSizeEvidence {
                method: "bounded-filesystem-st-blocks-sum".into(),
                evidence_complete: true,
                allocated_bytes: 4096,
                logical_bytes: 100,
                visited_entries: 10,
                error: None,
            },
            active_use: GitWorktreeActiveUseEvidence {
                method: "lsof-recursive-pid".into(),
                assessed: true,
                evidence_complete: true,
                active: false,
                observed_pids: Vec::new(),
                results_truncated: false,
                error: None,
            },
            disposition: GitWorktreeDisposition::RemovalCandidate,
            blockers: Vec::new(),
            entry_fingerprint: hex('c'),
        }
    }

    fn audit(path: &Path) -> GitWorktreeAuditReport {
        let entry = candidate(path);
        GitWorktreeAuditReport {
            schema_kind: GIT_WORKTREE_AUDIT_SCHEMA_KIND.into(),
            version: 2,
            repository_root: path.to_string_lossy().into_owned(),
            common_dir: path.join(".git").to_string_lossy().into_owned(),
            generated_at_ms: 1,
            retention_references: vec![GitWorktreeReferenceBinding {
                reference_ref: "refs/heads/develop".into(),
                reference_oid: "d".repeat(40),
            }],
            retention_reference_set_fingerprint: hex('d'),
            retention_reachable_commit_count: 1,
            worktree_count: 1,
            removal_candidate_count: 1,
            removal_candidate_allocated_bytes: entry.size.allocated_bytes,
            preserved_count: 0,
            evidence_gap_count: 0,
            evidence_complete: true,
            removal_plan_fingerprint: hex('e'),
            exact_approval_phrase: Some("source approval".into()),
            entries: vec![entry],
            issues: Vec::new(),
            filesystem_mutation_executed: false,
        }
    }

    fn file(path: &Path, bytes: &[u8]) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, bytes).unwrap();
    }

    #[test]
    fn audit_finds_only_structurally_bound_artifacts_and_never_mutates() {
        let temp = tempfile::tempdir().unwrap();
        file(&temp.path().join("web/package.json"), b"{}");
        file(
            &temp.path().join("web/node_modules/package/payload.bin"),
            b"payload",
        );
        file(&temp.path().join("rust/Cargo.toml"), b"[package]");
        file(&temp.path().join("rust/target/debug/output"), b"target");
        file(&temp.path().join("python/__pycache__/module.pyc"), b"cache");
        file(
            &temp.path().join("orphan/node_modules/payload.bin"),
            b"preserve",
        );

        let report =
            audit_git_worktree_artifacts(&audit(temp.path()), Default::default(), 2).unwrap();
        assert!(report.evidence_complete);
        assert_eq!(report.artifact_count, 3);
        assert_eq!(report.reclaim_candidate_count, 3);
        assert!(report.reclaim_candidate_allocated_bytes > 0);
        assert!(report.exact_approval_phrase.is_some());
        assert!(!report.filesystem_mutation_executed);
        assert!(!report.worktree_mutation_executed);
        assert!(temp.path().join("web/node_modules").exists());
        assert!(temp.path().join("orphan/node_modules").exists());
        assert!(!report
            .entries
            .iter()
            .any(|entry| entry.path.contains("orphan")));

        let public = serde_json::to_string(&public_summary(&report)).unwrap();
        assert!(!public.contains(temp.path().to_string_lossy().as_ref()));
        assert!(!public.contains("refs/heads/merged"));
        assert!(public.contains("\"local_paths_redacted\":true"));
    }

    #[cfg(unix)]
    #[test]
    fn discovery_does_not_follow_artifact_symlinks() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        file(&temp.path().join("web/package.json"), b"{}");
        file(&outside.path().join("payload"), b"outside");
        fs::create_dir_all(temp.path().join("web")).unwrap();
        symlink(outside.path(), temp.path().join("web/node_modules")).unwrap();

        let report =
            audit_git_worktree_artifacts(&audit(temp.path()), Default::default(), 2).unwrap();
        assert_eq!(report.artifact_count, 0);
        assert!(outside.path().join("payload").exists());
    }

    #[test]
    fn invalid_source_audit_and_unbounded_options_fail_closed() {
        let temp = tempfile::tempdir().unwrap();
        let mut invalid = audit(temp.path());
        invalid.entries[0].active_use.active = true;
        assert_eq!(
            audit_git_worktree_artifacts(&invalid, Default::default(), 2).unwrap_err(),
            "git-worktree-artifact-source-audit-invalid"
        );

        let options = GitWorktreeArtifactAuditOptions {
            max_artifacts: 0,
            ..Default::default()
        };
        assert_eq!(
            audit_git_worktree_artifacts(&audit(temp.path()), options, 2).unwrap_err(),
            "git-worktree-artifact-audit-options-invalid"
        );
    }
}
