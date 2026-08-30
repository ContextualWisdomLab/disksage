//! Bounded, read-only exact-duplicate evidence for an operator-selected source tree.
//!
//! Unlike the interactive duplicate browser, this audit never silently treats unreadable or
//! unstable files as complete evidence. Paths and content digests belong only in a private report;
//! the public summary is path-redacted and never authorizes deletion.

use crate::cloud::{self, MetadataEvidence};
use crate::content_digest::{ContentDigests, ContentHasher};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, Metadata};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
#[cfg(all(unix, not(coverage)))]
use std::process::Command;

pub const EXACT_DUPLICATE_AUDIT_VERSION: u32 = 1;
pub const DEFAULT_MAX_ENTRIES: usize = 200_000;
pub const MAX_ENTRIES: usize = 1_000_000;
pub const DEFAULT_MIN_BYTES: u64 = 1;
const MAX_DEPTH: usize = 64;
const IO_BUFFER_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExactDuplicateAuditMember {
    pub member_fingerprint: String,
    pub metadata_fingerprint: String,
    pub relative_path: String,
    pub logical_bytes: u64,
    pub filesystem_created_ms: u64,
    pub filesystem_modified_ms: u64,
    pub production_metadata: ExactDuplicateProductionMetadata,
    pub storage_identity_fingerprint: Option<String>,
    pub source_stable: bool,
    pub path_identity_verified: bool,
    pub write_performed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExactDuplicateProductionMetadata {
    pub production_time_ms: u64,
    pub production_time_source: String,
    pub production_time_confidence: String,
    pub embedded_production_time_ms: Option<u64>,
    pub filename_date_ms: Option<u64>,
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub context: Vec<String>,
    pub duration_ms: Option<u64>,
    pub embedded_evidence: Vec<MetadataEvidence>,
    pub metadata_probe_complete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExactDuplicateAuditCluster {
    pub cluster_fingerprint: String,
    pub content_digests: ContentDigests,
    pub logical_bytes_per_file: u64,
    pub file_count: usize,
    pub logical_duplicate_bytes: u64,
    pub logical_redundant_bytes: u64,
    pub distinct_storage_identity_count: Option<usize>,
    pub physical_reclaimable_bytes: Option<u64>,
    pub requires_human_canonical_selection: bool,
    pub automatic_delete_allowed: bool,
    pub members: Vec<ExactDuplicateAuditMember>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExactDuplicateAuditReport {
    pub schema_version: u32,
    pub observed_at_ms: u64,
    pub source_root: String,
    pub source_scope_fingerprint: String,
    pub min_bytes: u64,
    pub max_entries: usize,
    pub evidence_complete: bool,
    pub entries_seen: usize,
    pub file_count: usize,
    pub size_collision_candidate_count: usize,
    pub content_hashed_file_count: usize,
    pub cluster_count: usize,
    pub duplicate_file_count: usize,
    pub logical_duplicate_bytes: u64,
    pub logical_redundant_bytes: u64,
    pub physical_reclaimable_bytes: Option<u64>,
    pub metadata_evidence_complete: bool,
    pub production_time_source_counts: BTreeMap<String, u64>,
    pub issue_counts: BTreeMap<String, u64>,
    pub audit_fingerprint: String,
    pub production_metadata_evaluated: bool,
    pub production_date_policy: String,
    pub exact_content_match_is_delete_approval: bool,
    pub automatic_delete_allowed: bool,
    pub mutation_performed: bool,
    pub clusters: Vec<ExactDuplicateAuditCluster>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExactDuplicateAuditSummary {
    pub schema_version: u32,
    pub output_mode: String,
    pub observed_at_ms: u64,
    pub source_scope_fingerprint: String,
    pub min_bytes: u64,
    pub max_entries: usize,
    pub evidence_complete: bool,
    pub entries_seen: usize,
    pub file_count: usize,
    pub size_collision_candidate_count: usize,
    pub content_hashed_file_count: usize,
    pub cluster_count: usize,
    pub duplicate_file_count: usize,
    pub logical_duplicate_bytes: u64,
    pub logical_redundant_bytes: u64,
    pub physical_reclaimable_bytes: Option<u64>,
    pub metadata_evidence_complete: bool,
    pub production_time_source_counts: BTreeMap<String, u64>,
    pub issue_counts: BTreeMap<String, u64>,
    pub audit_fingerprint: String,
    pub content_digest_algorithms: Vec<String>,
    pub local_paths_included: bool,
    pub content_digests_included: bool,
    pub production_metadata_evaluated: bool,
    pub production_date_policy: String,
    pub exact_content_match_is_delete_approval: bool,
    pub requires_human_canonical_selection: bool,
    pub automatic_delete_allowed: bool,
    pub mutation_performed: bool,
    pub reclaim_plan_fingerprint: Option<String>,
    pub exact_reclaim_approval_phrase: Option<String>,
    pub canonical_selection_policy: String,
    pub quality_equivalence_basis: String,
    pub notices: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExactDuplicateReclaimExecution {
    pub schema_version: u32,
    pub audit_fingerprint: String,
    pub reclaim_plan_fingerprint: String,
    pub candidate_file_count: usize,
    pub removed_file_count: usize,
    pub active_file_count: usize,
    pub failed_file_count: usize,
    pub skipped_file_count: usize,
    pub failure_reasons: Vec<String>,
    pub removed_allocated_bytes_upper_bound: u64,
    pub evidence_complete: bool,
    pub executed: bool,
    pub executed_at_ms: u64,
    pub rationale: String,
}

#[derive(Debug, Clone)]
struct FileObservation {
    path: PathBuf,
    relative_path: String,
    logical_bytes: u64,
    filesystem_created_ms: u64,
    filesystem_modified_ms: u64,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
}

fn normalized(value: &str) -> String {
    value.replace('\\', "/")
}

fn valid_relative_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn is_managed_photo_library(path: &Path) -> bool {
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_ascii_lowercase();
    name.ends_with(".photoslibrary") || name.ends_with(".photolibrary")
}

fn system_time_ms(value: std::io::Result<std::time::SystemTime>) -> u64 {
    value
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

fn increment_issue(issue_counts: &mut BTreeMap<String, u64>, reason: &str) {
    *issue_counts.entry(reason.to_string()).or_insert(0) += 1;
}

fn hash_value(hasher: &mut blake3::Hasher, value: &[u8]) {
    hasher.update(&(value.len() as u64).to_le_bytes());
    hasher.update(value);
}

fn source_scope_fingerprint(source_root: &str, min_bytes: u64, max_entries: usize) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-exact-duplicate-source-scope-v1\0");
    hash_value(&mut hasher, source_root.as_bytes());
    hash_value(&mut hasher, &min_bytes.to_le_bytes());
    hash_value(&mut hasher, &(max_entries as u64).to_le_bytes());
    hasher.finalize().to_hex().to_string()
}

fn member_fingerprint_fields(
    relative_path: &str,
    logical_bytes: u64,
    filesystem_created_ms: u64,
    filesystem_modified_ms: u64,
    digests: &ContentDigests,
    metadata_fingerprint: &str,
    storage_identity_fingerprint: Option<&str>,
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-exact-duplicate-member-v1\0");
    for value in [
        relative_path.as_bytes(),
        digests.blake3.as_bytes(),
        digests.sha256.as_bytes(),
        digests.quick_xor_base64.as_bytes(),
        metadata_fingerprint.as_bytes(),
        storage_identity_fingerprint.unwrap_or_default().as_bytes(),
    ] {
        hash_value(&mut hasher, value);
    }
    for value in [logical_bytes, filesystem_created_ms, filesystem_modified_ms] {
        hash_value(&mut hasher, &value.to_le_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

fn metadata_fingerprint(metadata: &ExactDuplicateProductionMetadata) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-exact-duplicate-production-metadata-v1\0");
    for value in [
        metadata.production_time_source.as_bytes(),
        metadata.production_time_confidence.as_bytes(),
        metadata.title.as_deref().unwrap_or_default().as_bytes(),
    ] {
        hash_value(&mut hasher, value);
    }
    for value in [
        Some(metadata.production_time_ms),
        metadata.embedded_production_time_ms,
        metadata.filename_date_ms,
        metadata.duration_ms,
    ] {
        hash_value(&mut hasher, &[u8::from(value.is_some())]);
        hash_value(&mut hasher, &value.unwrap_or_default().to_le_bytes());
    }
    hash_value(&mut hasher, &[u8::from(metadata.metadata_probe_complete)]);
    for values in [&metadata.authors, &metadata.context] {
        hash_value(&mut hasher, &(values.len() as u64).to_le_bytes());
        for value in values {
            hash_value(&mut hasher, value.as_bytes());
        }
    }
    hash_value(
        &mut hasher,
        &(metadata.embedded_evidence.len() as u64).to_le_bytes(),
    );
    for evidence in &metadata.embedded_evidence {
        for value in [
            evidence.field.as_bytes(),
            evidence.value.as_bytes(),
            evidence.source.as_bytes(),
            evidence.confidence.as_bytes(),
        ] {
            hash_value(&mut hasher, value);
        }
    }
    hasher.finalize().to_hex().to_string()
}

#[cfg(unix)]
fn storage_identity_fingerprint(observation: &FileObservation) -> Option<String> {
    storage_identity_fingerprint_from_metadata(observation.device, observation.inode)
}

#[cfg(unix)]
fn storage_identity_fingerprint_from_metadata(device: u64, inode: u64) -> Option<String> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-exact-duplicate-storage-identity-v1\0");
    hash_value(&mut hasher, &device.to_le_bytes());
    hash_value(&mut hasher, &inode.to_le_bytes());
    Some(hasher.finalize().to_hex().to_string())
}

#[cfg(not(unix))]
fn storage_identity_fingerprint(_observation: &FileObservation) -> Option<String> {
    None
}

fn select_production_time(
    embedded_time_ms: Option<u64>,
    embedded_source: Option<&str>,
    embedded_confidence: Option<&str>,
    filename_date_ms: Option<u64>,
    filesystem_created_ms: u64,
    filesystem_modified_ms: u64,
) -> (u64, String, String) {
    if let Some(value) = embedded_time_ms {
        (
            value,
            embedded_source.unwrap_or("embedded:unknown").to_string(),
            embedded_confidence.unwrap_or("medium").to_string(),
        )
    } else if let Some(value) = filename_date_ms {
        (value, "filename:path-token".into(), "low".into())
    } else if filesystem_created_ms > 0 {
        (
            filesystem_created_ms,
            "filesystem:created".into(),
            "low".into(),
        )
    } else {
        (
            filesystem_modified_ms,
            "filesystem:modified-fallback".into(),
            "low".into(),
        )
    }
}

#[cfg(not(coverage))]
fn production_metadata(observation: &FileObservation) -> ExactDuplicateProductionMetadata {
    let metadata = cloud::probe_content_metadata_for_audit(&observation.path);
    let embedded_production_time_ms = metadata.production_time_ms;
    let filename_date_ms = cloud::filename_date_ms(&observation.path);
    let (production_time_ms, production_time_source, production_time_confidence) =
        select_production_time(
            embedded_production_time_ms,
            metadata.production_time_source.as_deref(),
            metadata.production_time_confidence.as_deref(),
            filename_date_ms,
            observation.filesystem_created_ms,
            observation.filesystem_modified_ms,
        );
    let metadata_probe_complete = !metadata
        .evidence
        .iter()
        .any(|evidence| evidence.field == "metadata-probe-warning");
    ExactDuplicateProductionMetadata {
        production_time_ms,
        production_time_source,
        production_time_confidence,
        embedded_production_time_ms,
        filename_date_ms,
        title: metadata.title,
        authors: metadata.authors,
        context: metadata.context,
        duration_ms: metadata.duration_ms,
        embedded_evidence: metadata.evidence,
        metadata_probe_complete,
    }
}

fn member_fingerprint(
    observation: &FileObservation,
    digests: &ContentDigests,
    production_metadata: &ExactDuplicateProductionMetadata,
    storage_identity_fingerprint: Option<&str>,
) -> String {
    let metadata_fingerprint = metadata_fingerprint(production_metadata);
    member_fingerprint_fields(
        &observation.relative_path,
        observation.logical_bytes,
        observation.filesystem_created_ms,
        observation.filesystem_modified_ms,
        digests,
        &metadata_fingerprint,
        storage_identity_fingerprint,
    )
}

fn cluster_fingerprint(
    digests: &ContentDigests,
    logical_bytes: u64,
    members: &[ExactDuplicateAuditMember],
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-exact-duplicate-audit-cluster-v1\0");
    for value in [
        digests.blake3.as_bytes(),
        digests.sha256.as_bytes(),
        digests.quick_xor_base64.as_bytes(),
    ] {
        hash_value(&mut hasher, value);
    }
    hash_value(&mut hasher, &logical_bytes.to_le_bytes());
    hash_value(&mut hasher, &(members.len() as u64).to_le_bytes());
    for member in members {
        hash_value(&mut hasher, member.member_fingerprint.as_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

fn audit_fingerprint(
    source_scope_fingerprint: &str,
    evidence_complete: bool,
    counts: [usize; 4],
    issue_counts: &BTreeMap<String, u64>,
    clusters: &[ExactDuplicateAuditCluster],
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-exact-duplicate-audit-v1\0");
    hash_value(&mut hasher, source_scope_fingerprint.as_bytes());
    hash_value(&mut hasher, &[u8::from(evidence_complete)]);
    for value in counts {
        hash_value(&mut hasher, &(value as u64).to_le_bytes());
    }
    for (reason, count) in issue_counts {
        hash_value(&mut hasher, reason.as_bytes());
        hash_value(&mut hasher, &count.to_le_bytes());
    }
    for cluster in clusters {
        hash_value(&mut hasher, cluster.cluster_fingerprint.as_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

fn canonical_member(cluster: &ExactDuplicateAuditCluster) -> &ExactDuplicateAuditMember {
    cluster
        .members
        .iter()
        .min_by_key(|member| {
            (
                !member.production_metadata.metadata_probe_complete,
                member
                    .production_metadata
                    .embedded_production_time_ms
                    .is_none(),
                member.filesystem_created_ms == 0,
                member.filesystem_created_ms,
                member.relative_path.as_str(),
            )
        })
        .expect("duplicate clusters always contain at least two members")
}

fn reclaim_plan_fingerprint(report: &ExactDuplicateAuditReport) -> Option<String> {
    (report.evidence_complete && !report.clusters.is_empty()).then(|| {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"disksage-exact-duplicate-reclaim-v1\0");
        hash_value(&mut hasher, report.audit_fingerprint.as_bytes());
        for cluster in &report.clusters {
            hash_value(&mut hasher, cluster.cluster_fingerprint.as_bytes());
            hash_value(
                &mut hasher,
                canonical_member(cluster).member_fingerprint.as_bytes(),
            );
        }
        hasher.finalize().to_hex().to_string()
    })
}

pub fn exact_duplicate_reclaim_approval_phrase(
    report: &ExactDuplicateAuditReport,
) -> Option<String> {
    let fingerprint = reclaim_plan_fingerprint(report)?;
    let candidates = report
        .clusters
        .iter()
        .map(|cluster| cluster.file_count.saturating_sub(1))
        .sum::<usize>();
    Some(format!(
        "DiskSage exact duplicate reclaim {candidates} {} 승인 {fingerprint}",
        report.logical_redundant_bytes
    ))
}

#[cfg(unix)]
fn allocated_bytes(metadata: &Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    metadata.blocks().saturating_mul(512)
}

#[cfg(not(unix))]
fn allocated_bytes(metadata: &Metadata) -> u64 {
    metadata.len()
}

#[cfg(all(unix, not(coverage)))]
pub(crate) fn active_duplicate_candidates(paths: &[PathBuf]) -> Result<BTreeSet<PathBuf>, String> {
    let mut active = BTreeSet::new();
    for chunk in paths.chunks(64) {
        let mut command = Command::new("lsof");
        command.args(["-F", "pn", "--"]);
        command.args(chunk);
        let result = command
            .output()
            .map_err(|_| "duplicate-reclaim-active-use-lsof-unavailable".to_string())?;
        if !matches!(result.status.code(), Some(0) | Some(1))
            || result.stdout.len() > 2 * 1024 * 1024
        {
            return Err("duplicate-reclaim-active-use-lsof-incomplete".into());
        }
        for field in result.stdout.split(|byte| *byte == b'\n') {
            let Some(path) = field.strip_prefix(b"n") else {
                continue;
            };
            if let Some(candidate) = chunk
                .iter()
                .find(|candidate| candidate.as_os_str().as_encoded_bytes() == path)
            {
                active.insert(candidate.clone());
            }
        }
    }
    let ps = Command::new("ps")
        .args(["-axo", "command="])
        .output()
        .map_err(|_| "duplicate-reclaim-active-use-ps-unavailable".to_string())?;
    if !ps.status.success() || ps.stdout.len() > 2 * 1024 * 1024 {
        return Err("duplicate-reclaim-active-use-ps-incomplete".into());
    }
    let commands = String::from_utf8(ps.stdout)
        .map_err(|_| "duplicate-reclaim-active-use-ps-invalid".to_string())?;
    for path in paths {
        if path.to_str().is_some_and(|path| commands.contains(path)) {
            active.insert(path.clone());
        }
    }
    Ok(active)
}

#[cfg(any(not(unix), coverage))]
pub(crate) fn active_duplicate_candidates(_paths: &[PathBuf]) -> Result<BTreeSet<PathBuf>, String> {
    Err("duplicate-reclaim-active-use-unsupported-platform".into())
}

#[cfg(unix)]
fn preserve_staged_candidate(
    original: &Path,
    staged: &Path,
    staging_token: u64,
) -> Result<(), String> {
    if std::fs::symlink_metadata(original).is_err() {
        return std::fs::rename(staged, original)
            .map_err(|_| "duplicate-reclaim-recovery-failed".to_string());
    }
    let file_name = original
        .file_name()
        .ok_or_else(|| "duplicate-reclaim-candidate-name-missing".to_string())?
        .to_string_lossy();
    let recovery =
        original.with_file_name(format!("{file_name}.disksage-recovery-{staging_token}"));
    if std::fs::symlink_metadata(&recovery).is_ok() {
        return Err("duplicate-reclaim-recovery-location-occupied".into());
    }
    std::fs::rename(staged, recovery)
        .map_err(|_| "duplicate-reclaim-recovery-failed".to_string())?;
    Err("duplicate-reclaim-recovery-preserved".into())
}

#[cfg(unix)]
fn remove_if_storage_identity(
    path: &Path,
    expected_storage_identity: &str,
    expected_logical_bytes: u64,
    expected_content_digests: &ContentDigests,
    staging_token: u64,
) -> Result<(), String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| "duplicate-reclaim-candidate-changed".to_string())?;
    let (device, inode) = unix_identity(&metadata);
    if storage_identity_fingerprint_from_metadata(device, inode).as_deref()
        != Some(expected_storage_identity)
    {
        return Err("duplicate-reclaim-candidate-changed".into());
    }
    let parent = path
        .parent()
        .ok_or_else(|| "duplicate-reclaim-candidate-parent-missing".to_string())?;
    let staging_dir = parent.join(format!(
        ".disksage-duplicate-stage-{}-{staging_token}",
        std::process::id()
    ));
    std::fs::create_dir(&staging_dir)
        .map_err(|_| "duplicate-reclaim-staging-unavailable".to_string())?;
    let staged = staging_dir.join(
        path.file_name()
            .ok_or_else(|| "duplicate-reclaim-candidate-name-missing".to_string())?,
    );
    if let Err(error) = std::fs::rename(path, &staged) {
        let _ = std::fs::remove_dir(&staging_dir);
        return Err(format!("duplicate-reclaim-staging-failed:{error}"));
    }
    let staged_matches = std::fs::symlink_metadata(&staged)
        .ok()
        .map(|metadata| unix_identity(&metadata))
        .and_then(|(device, inode)| storage_identity_fingerprint_from_metadata(device, inode))
        .as_deref()
        == Some(expected_storage_identity);
    if !staged_matches {
        let recovery = preserve_staged_candidate(path, &staged, staging_token);
        let _ = std::fs::remove_dir(&staging_dir);
        return recovery.and(Err(
            "duplicate-reclaim-candidate-changed-during-staging".into()
        ));
    }
    let staged_observation = FileObservation {
        path: staged.clone(),
        relative_path: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        logical_bytes: expected_logical_bytes,
        filesystem_created_ms: system_time_ms(metadata.created()),
        filesystem_modified_ms: system_time_ms(metadata.modified()),
        device,
        inode,
    };
    if hash_stable_file(&staged_observation).as_ref() != Ok(expected_content_digests) {
        let recovery = preserve_staged_candidate(path, &staged, staging_token);
        let _ = std::fs::remove_dir(&staging_dir);
        return recovery.and(Err("duplicate-reclaim-candidate-content-changed".into()));
    }
    if let Err(error) = std::fs::remove_file(&staged) {
        let recovery = preserve_staged_candidate(path, &staged, staging_token);
        let _ = std::fs::remove_dir(&staging_dir);
        if let Err(recovery_error) = recovery {
            return Err(recovery_error);
        }
        return Err(format!("duplicate-reclaim-delete-failed:{error}"));
    }
    let _ = std::fs::remove_dir(&staging_dir);
    Ok(())
}

#[cfg(not(unix))]
fn remove_if_storage_identity(
    _path: &Path,
    _expected_storage_identity: &str,
    _expected_logical_bytes: u64,
    _expected_content_digests: &ContentDigests,
    _staging_token: u64,
) -> Result<(), String> {
    Err("duplicate-reclaim-permanent-delete-unsupported-platform".into())
}

fn removal_failure_code(error: &str) -> String {
    error.split(':').next().unwrap_or(error).to_string()
}

/// Permanently remove only freshly re-hashed members of exact-content clusters while retaining
/// one provenance-preferred, byte-identical canonical member in every cluster.
#[cfg(not(coverage))]
pub fn execute_exact_duplicate_reclaim(
    source_root: &Path,
    min_bytes: u64,
    max_entries: usize,
    approved_audit_fingerprint: &str,
    confirmation_phrase: &str,
    rationale: &str,
    executed_at_ms: u64,
) -> Result<ExactDuplicateReclaimExecution, String> {
    let report =
        collect_exact_duplicate_audit(source_root, executed_at_ms, min_bytes, max_entries)?;
    execute_exact_duplicate_reclaim_from_report(
        source_root,
        &report,
        approved_audit_fingerprint,
        confirmation_phrase,
        rationale,
        executed_at_ms,
    )
}

/// Execute an approved immutable private report by freshly revalidating only its exact candidates.
/// Unrelated additions or changes elsewhere in the live source tree cannot expand the approved set.
#[cfg(not(coverage))]
pub fn execute_exact_duplicate_reclaim_from_report(
    source_root: &Path,
    report: &ExactDuplicateAuditReport,
    approved_audit_fingerprint: &str,
    confirmation_phrase: &str,
    rationale: &str,
    executed_at_ms: u64,
) -> Result<ExactDuplicateReclaimExecution, String> {
    if rationale.trim() != rationale || rationale.is_empty() || rationale.chars().count() > 1_000 {
        return Err("duplicate-reclaim-rationale-invalid".into());
    }
    if !exact_duplicate_audit_integrity_valid(&report) || !report.evidence_complete {
        return Err("duplicate-reclaim-evidence-incomplete".into());
    }
    if report.audit_fingerprint != approved_audit_fingerprint {
        return Err("duplicate-reclaim-audit-fingerprint-mismatch".into());
    }
    let plan_fingerprint = reclaim_plan_fingerprint(&report)
        .ok_or_else(|| "duplicate-reclaim-empty-candidate-set".to_string())?;
    if exact_duplicate_reclaim_approval_phrase(&report).as_deref() != Some(confirmation_phrase) {
        return Err("duplicate-reclaim-confirmation-mismatch".into());
    }

    let canonical_root = std::fs::canonicalize(source_root)
        .map_err(|_| "duplicate-reclaim-root-unavailable".to_string())?;
    if canonical_root.to_str() != Some(report.source_root.as_str())
        || executed_at_ms < report.observed_at_ms
    {
        return Err("duplicate-reclaim-source-scope-mismatch".into());
    }
    let candidate_file_count = report
        .clusters
        .iter()
        .map(|cluster| cluster.file_count.saturating_sub(1))
        .sum::<usize>();
    let mut verified = Vec::with_capacity(candidate_file_count);
    for cluster in &report.clusters {
        let retained = canonical_member(cluster).member_fingerprint.as_str();
        for expected in &cluster.members {
            let relative = Path::new(&expected.relative_path);
            if !valid_relative_path(relative) {
                return Err("duplicate-reclaim-relative-path-unsafe".into());
            }
            let path = canonical_root.join(relative);
            let metadata = std::fs::symlink_metadata(&path)
                .map_err(|_| "duplicate-reclaim-candidate-changed".to_string())?;
            let observation = observe_file(&canonical_root, path.clone(), metadata.clone())
                .map_err(|_| "duplicate-reclaim-candidate-changed".to_string())?;
            let digests = hash_stable_file(&observation)?;
            if digests != cluster.content_digests
                || observation.relative_path != expected.relative_path
                || observation.logical_bytes != expected.logical_bytes
                || observation.filesystem_created_ms != expected.filesystem_created_ms
                || observation.filesystem_modified_ms != expected.filesystem_modified_ms
                || storage_identity_fingerprint(&observation)
                    != expected.storage_identity_fingerprint
            {
                return Err("duplicate-reclaim-candidate-changed".into());
            }
            if expected.member_fingerprint != retained {
                verified.push((
                    path,
                    allocated_bytes(&metadata),
                    expected.logical_bytes,
                    cluster.content_digests.clone(),
                    expected
                        .storage_identity_fingerprint
                        .clone()
                        .ok_or_else(|| "duplicate-reclaim-storage-identity-missing".to_string())?,
                ));
            }
        }
    }
    let active = active_duplicate_candidates(
        &verified
            .iter()
            .map(|(path, _, _, _, _)| path.clone())
            .collect::<Vec<_>>(),
    )?;
    let removable = verified
        .into_iter()
        .filter(|(path, _, _, _, _)| !active.contains(path))
        .collect::<Vec<_>>();

    let mut removed_file_count = 0usize;
    let mut removed_allocated_bytes_upper_bound = 0u64;
    let mut failure_reasons = BTreeSet::new();
    for (index, (path, bytes, logical_bytes, content_digests, storage_identity)) in
        removable.into_iter().enumerate()
    {
        match remove_if_storage_identity(
            &path,
            &storage_identity,
            logical_bytes,
            &content_digests,
            executed_at_ms.saturating_add(index as u64),
        ) {
            Ok(()) => {
                removed_file_count = removed_file_count.saturating_add(1);
                removed_allocated_bytes_upper_bound =
                    removed_allocated_bytes_upper_bound.saturating_add(bytes);
            }
            Err(error) => {
                failure_reasons.insert(removal_failure_code(&error));
            }
        }
    }
    let active_file_count = active.len();
    let failed_file_count = candidate_file_count
        .saturating_sub(active_file_count)
        .saturating_sub(removed_file_count);
    let skipped_file_count = candidate_file_count.saturating_sub(removed_file_count);
    Ok(ExactDuplicateReclaimExecution {
        schema_version: EXACT_DUPLICATE_AUDIT_VERSION,
        audit_fingerprint: report.audit_fingerprint.clone(),
        reclaim_plan_fingerprint: plan_fingerprint,
        candidate_file_count,
        removed_file_count,
        active_file_count,
        failed_file_count,
        skipped_file_count,
        failure_reasons: failure_reasons.into_iter().collect(),
        removed_allocated_bytes_upper_bound,
        evidence_complete: skipped_file_count == 0,
        executed: true,
        executed_at_ms,
        rationale: rationale.into(),
    })
}

#[cfg(unix)]
fn unix_identity(metadata: &Metadata) -> (u64, u64) {
    use std::os::unix::fs::MetadataExt;
    (metadata.dev(), metadata.ino())
}

fn observe_file(
    canonical_root: &Path,
    path: PathBuf,
    metadata: Metadata,
) -> Result<FileObservation, String> {
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("duplicate-audit-file-unsafe".into());
    }
    let relative = path
        .strip_prefix(canonical_root)
        .map_err(|_| "duplicate-audit-relative-path-failed".to_string())?;
    if !valid_relative_path(relative) {
        return Err("duplicate-audit-relative-path-unsafe".into());
    }
    let relative_path = relative
        .to_str()
        .map(normalized)
        .ok_or_else(|| "duplicate-audit-relative-path-non-unicode".to_string())?;
    let filesystem_modified_ms = system_time_ms(metadata.modified());
    if filesystem_modified_ms == 0 {
        return Err("duplicate-audit-modified-time-unavailable".into());
    }
    #[cfg(unix)]
    let (device, inode) = unix_identity(&metadata);
    Ok(FileObservation {
        path,
        relative_path,
        logical_bytes: metadata.len(),
        filesystem_created_ms: system_time_ms(metadata.created()),
        filesystem_modified_ms,
        #[cfg(unix)]
        device,
        #[cfg(unix)]
        inode,
    })
}

fn metadata_matches(metadata: &Metadata, observation: &FileObservation) -> bool {
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() != observation.logical_bytes
        || system_time_ms(metadata.created()) != observation.filesystem_created_ms
        || system_time_ms(metadata.modified()) != observation.filesystem_modified_ms
    {
        return false;
    }
    #[cfg(unix)]
    {
        let (device, inode) = unix_identity(metadata);
        if device != observation.device || inode != observation.inode {
            return false;
        }
    }
    true
}

fn hash_stable_file(observation: &FileObservation) -> Result<ContentDigests, String> {
    let before = std::fs::symlink_metadata(&observation.path)
        .map_err(|_| "duplicate-audit-pre-hash-metadata-failed".to_string())?;
    if !metadata_matches(&before, observation) {
        return Err("duplicate-audit-source-changed-before-hash".into());
    }
    let mut file = File::open(&observation.path)
        .map_err(|_| "duplicate-audit-content-open-failed".to_string())?;
    let opened = file
        .metadata()
        .map_err(|_| "duplicate-audit-open-file-metadata-failed".to_string())?;
    if !metadata_matches(&opened, observation) {
        return Err("duplicate-audit-opened-source-mismatch".into());
    }
    let mut hasher = ContentHasher::default();
    let mut buffer = vec![0u8; IO_BUFFER_BYTES];
    let mut bytes_read = 0u64;
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| "duplicate-audit-content-read-failed".to_string())?;
        if read == 0 {
            break;
        }
        bytes_read = bytes_read.saturating_add(read as u64);
        hasher.update(&buffer[..read]);
    }
    if bytes_read != observation.logical_bytes {
        return Err("duplicate-audit-content-length-mismatch".into());
    }
    let after = std::fs::symlink_metadata(&observation.path)
        .map_err(|_| "duplicate-audit-post-hash-metadata-failed".to_string())?;
    if !metadata_matches(&after, observation) {
        return Err("duplicate-audit-source-changed-during-hash".into());
    }
    Ok(hasher.finalize())
}

#[cfg(not(coverage))]
fn member(
    observation: &FileObservation,
    digests: &ContentDigests,
) -> Result<ExactDuplicateAuditMember, String> {
    let production_metadata = production_metadata(observation);
    let after = std::fs::symlink_metadata(&observation.path)
        .map_err(|_| "duplicate-audit-post-metadata-probe-stat-failed".to_string())?;
    if !metadata_matches(&after, observation) {
        return Err("duplicate-audit-source-changed-during-metadata-probe".into());
    }
    let storage_identity_fingerprint = storage_identity_fingerprint(observation);
    let metadata_fingerprint = metadata_fingerprint(&production_metadata);
    Ok(ExactDuplicateAuditMember {
        member_fingerprint: member_fingerprint(
            observation,
            digests,
            &production_metadata,
            storage_identity_fingerprint.as_deref(),
        ),
        metadata_fingerprint,
        relative_path: observation.relative_path.clone(),
        logical_bytes: observation.logical_bytes,
        filesystem_created_ms: observation.filesystem_created_ms,
        filesystem_modified_ms: observation.filesystem_modified_ms,
        production_metadata,
        storage_identity_fingerprint,
        source_stable: true,
        path_identity_verified: cfg!(unix),
        write_performed: false,
    })
}

/// Recursively collect exact duplicate evidence without following symlinks or mutating files.
///
/// Filesystem timestamps are stability evidence only. The audit records production evidence but
/// remains read-only; the separately approved reclaim path retains one byte-identical canonical
/// member and revalidates every candidate before deletion.
#[cfg(not(coverage))]
pub fn collect_exact_duplicate_audit(
    source_root: &Path,
    observed_at_ms: u64,
    min_bytes: u64,
    max_entries: usize,
) -> Result<ExactDuplicateAuditReport, String> {
    if !source_root.is_absolute() {
        return Err("duplicate-audit-root-must-be-absolute".into());
    }
    if min_bytes == 0 {
        return Err("duplicate-audit-min-bytes-out-of-range".into());
    }
    if !(1..=MAX_ENTRIES).contains(&max_entries) {
        return Err("duplicate-audit-max-entries-out-of-range".into());
    }
    let canonical_root = std::fs::canonicalize(source_root)
        .map_err(|_| "duplicate-audit-root-unavailable".to_string())?;
    if canonical_root.to_str().is_none() {
        return Err("duplicate-audit-root-non-unicode".into());
    }
    let root_metadata = std::fs::symlink_metadata(&canonical_root)
        .map_err(|_| "duplicate-audit-root-unavailable".to_string())?;
    if !root_metadata.is_dir() || root_metadata.file_type().is_symlink() {
        return Err("duplicate-audit-root-unsafe".into());
    }
    if is_managed_photo_library(&canonical_root) {
        return Err("duplicate-audit-root-system-managed-photo-library".into());
    }

    let mut evidence_complete = true;
    let mut entries_seen = 0usize;
    let mut file_count = 0usize;
    let mut issue_counts = BTreeMap::new();
    let mut observations = Vec::new();
    let mut pending = vec![(canonical_root.clone(), 0usize)];
    while let Some((directory, depth)) = pending.pop() {
        let entries = match std::fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(_) => {
                evidence_complete = false;
                increment_issue(&mut issue_counts, "duplicate-audit-directory-read-failed");
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
                increment_issue(&mut issue_counts, "duplicate-audit-entry-limit-reached");
                pending.clear();
                break;
            }
            entries_seen += 1;
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => {
                    evidence_complete = false;
                    increment_issue(&mut issue_counts, "duplicate-audit-directory-entry-failed");
                    continue;
                }
            };
            let file_type = match entry.file_type() {
                Ok(file_type) => file_type,
                Err(_) => {
                    evidence_complete = false;
                    increment_issue(&mut issue_counts, "duplicate-audit-file-type-failed");
                    continue;
                }
            };
            if file_type.is_symlink() {
                continue;
            }
            let path = entry.path();
            if file_type.is_dir() {
                if is_managed_photo_library(&path) {
                    increment_issue(
                        &mut issue_counts,
                        "duplicate-audit-system-managed-photo-library-excluded",
                    );
                    continue;
                }
                if depth >= MAX_DEPTH {
                    evidence_complete = false;
                    increment_issue(&mut issue_counts, "duplicate-audit-depth-limit-reached");
                } else {
                    pending.push((path, depth + 1));
                }
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            file_count += 1;
            let metadata = match std::fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(_) => {
                    evidence_complete = false;
                    increment_issue(&mut issue_counts, "duplicate-audit-file-metadata-failed");
                    continue;
                }
            };
            if metadata.len() < min_bytes {
                continue;
            }
            match observe_file(&canonical_root, path, metadata) {
                Ok(observation) => observations.push(observation),
                Err(reason) => {
                    evidence_complete = false;
                    increment_issue(&mut issue_counts, &reason);
                }
            }
        }
    }

    let mut by_size = BTreeMap::<u64, Vec<FileObservation>>::new();
    for observation in observations {
        by_size
            .entry(observation.logical_bytes)
            .or_default()
            .push(observation);
    }
    let size_collision_candidate_count = by_size
        .values()
        .filter(|group| group.len() > 1)
        .map(Vec::len)
        .sum();
    let mut content_hashed_file_count = 0usize;
    let mut clusters = Vec::new();
    for (logical_bytes, size_group) in by_size {
        if size_group.len() < 2 {
            continue;
        }
        let mut by_digest =
            BTreeMap::<(String, String, String), Vec<(FileObservation, ContentDigests)>>::new();
        for observation in size_group {
            match hash_stable_file(&observation) {
                Ok(digests) => {
                    content_hashed_file_count += 1;
                    let key = (
                        digests.blake3.clone(),
                        digests.sha256.clone(),
                        digests.quick_xor_base64.clone(),
                    );
                    by_digest
                        .entry(key)
                        .or_default()
                        .push((observation, digests));
                }
                Err(reason) => {
                    evidence_complete = false;
                    increment_issue(&mut issue_counts, &reason);
                }
            }
        }
        for ((blake3, sha256, quick_xor_base64), observations) in by_digest {
            if observations.len() < 2 {
                continue;
            }
            let mut members = Vec::with_capacity(observations.len());
            for (observation, digests) in observations {
                match member(&observation, &digests) {
                    Ok(member) => members.push(member),
                    Err(reason) => {
                        evidence_complete = false;
                        increment_issue(&mut issue_counts, &reason);
                    }
                }
            }
            if members.len() < 2 {
                continue;
            }
            members.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
            let content_digests = ContentDigests {
                blake3,
                sha256,
                quick_xor_base64,
            };
            let file_count = members.len();
            let logical_duplicate_bytes = logical_bytes.saturating_mul(file_count as u64);
            let logical_redundant_bytes =
                logical_bytes.saturating_mul((file_count as u64).saturating_sub(1));
            let distinct_storage_identity_count = members
                .iter()
                .map(|member| member.storage_identity_fingerprint.as_deref())
                .collect::<Option<std::collections::BTreeSet<_>>>()
                .map(|identities| identities.len());
            clusters.push(ExactDuplicateAuditCluster {
                cluster_fingerprint: cluster_fingerprint(&content_digests, logical_bytes, &members),
                content_digests,
                logical_bytes_per_file: logical_bytes,
                file_count,
                logical_duplicate_bytes,
                logical_redundant_bytes,
                distinct_storage_identity_count,
                physical_reclaimable_bytes: None,
                requires_human_canonical_selection: true,
                automatic_delete_allowed: false,
                members,
            });
        }
    }
    clusters.sort_by(|left, right| {
        right
            .logical_redundant_bytes
            .cmp(&left.logical_redundant_bytes)
            .then_with(|| left.cluster_fingerprint.cmp(&right.cluster_fingerprint))
    });
    let cluster_count = clusters.len();
    let duplicate_file_count: usize = clusters.iter().map(|cluster| cluster.file_count).sum();
    let logical_duplicate_bytes = clusters.iter().fold(0u64, |total, cluster| {
        total.saturating_add(cluster.logical_duplicate_bytes)
    });
    let logical_redundant_bytes = clusters.iter().fold(0u64, |total, cluster| {
        total.saturating_add(cluster.logical_redundant_bytes)
    });
    let metadata_evidence_complete = clusters.iter().all(|cluster| {
        cluster
            .members
            .iter()
            .all(|member| member.production_metadata.metadata_probe_complete)
    });
    let mut production_time_source_counts = BTreeMap::new();
    for member in clusters.iter().flat_map(|cluster| &cluster.members) {
        *production_time_source_counts
            .entry(member.production_metadata.production_time_source.clone())
            .or_insert(0) += 1;
    }
    let source_root = canonical_root
        .to_str()
        .map(normalized)
        .ok_or_else(|| "duplicate-audit-root-non-unicode".to_string())?;
    let source_scope_fingerprint = source_scope_fingerprint(&source_root, min_bytes, max_entries);
    let audit_fingerprint = audit_fingerprint(
        &source_scope_fingerprint,
        evidence_complete,
        [
            entries_seen,
            file_count,
            size_collision_candidate_count,
            content_hashed_file_count,
        ],
        &issue_counts,
        &clusters,
    );
    Ok(ExactDuplicateAuditReport {
        schema_version: EXACT_DUPLICATE_AUDIT_VERSION,
        observed_at_ms,
        source_root,
        source_scope_fingerprint,
        min_bytes,
        max_entries,
        evidence_complete,
        entries_seen,
        file_count,
        size_collision_candidate_count,
        content_hashed_file_count,
        cluster_count,
        duplicate_file_count,
        logical_duplicate_bytes,
        logical_redundant_bytes,
        physical_reclaimable_bytes: None,
        metadata_evidence_complete,
        production_time_source_counts,
        issue_counts,
        audit_fingerprint,
        production_metadata_evaluated: true,
        production_date_policy: "embedded>filename-explicit>filesystem-created>filesystem-modified"
            .into(),
        exact_content_match_is_delete_approval: false,
        automatic_delete_allowed: false,
        mutation_performed: false,
        clusters,
    })
}

pub fn exact_duplicate_audit_integrity_valid(report: &ExactDuplicateAuditReport) -> bool {
    if report.schema_version != EXACT_DUPLICATE_AUDIT_VERSION
        || report.min_bytes == 0
        || !(1..=MAX_ENTRIES).contains(&report.max_entries)
        || !report.production_metadata_evaluated
        || report.production_date_policy
            != "embedded>filename-explicit>filesystem-created>filesystem-modified"
        || report.physical_reclaimable_bytes.is_some()
        || report.exact_content_match_is_delete_approval
        || report.automatic_delete_allowed
        || report.mutation_performed
        || report.cluster_count != report.clusters.len()
        || report.clusters.iter().any(|cluster| {
            cluster.file_count < 2
                || cluster.file_count != cluster.members.len()
                || cluster.logical_duplicate_bytes
                    != cluster
                        .logical_bytes_per_file
                        .saturating_mul(cluster.file_count as u64)
                || cluster.logical_redundant_bytes
                    != cluster
                        .logical_bytes_per_file
                        .saturating_mul((cluster.file_count as u64).saturating_sub(1))
                || cluster.physical_reclaimable_bytes.is_some()
                || cluster.distinct_storage_identity_count
                    != cluster
                        .members
                        .iter()
                        .map(|member| member.storage_identity_fingerprint.as_deref())
                        .collect::<Option<std::collections::BTreeSet<_>>>()
                        .map(|identities| identities.len())
                || !cluster.requires_human_canonical_selection
                || cluster.automatic_delete_allowed
                || cluster.members.iter().any(|member| {
                    !member.source_stable
                        || member.write_performed
                        || member.logical_bytes != cluster.logical_bytes_per_file
                        || member.metadata_fingerprint
                            != metadata_fingerprint(&member.production_metadata)
                        || member.member_fingerprint
                            != member_fingerprint_fields(
                                &member.relative_path,
                                member.logical_bytes,
                                member.filesystem_created_ms,
                                member.filesystem_modified_ms,
                                &cluster.content_digests,
                                &member.metadata_fingerprint,
                                member.storage_identity_fingerprint.as_deref(),
                            )
                })
                || cluster.cluster_fingerprint
                    != cluster_fingerprint(
                        &cluster.content_digests,
                        cluster.logical_bytes_per_file,
                        &cluster.members,
                    )
        })
    {
        return false;
    }
    let duplicate_file_count: usize = report
        .clusters
        .iter()
        .map(|cluster| cluster.file_count)
        .sum();
    let logical_duplicate_bytes = report.clusters.iter().fold(0u64, |total, cluster| {
        total.saturating_add(cluster.logical_duplicate_bytes)
    });
    let logical_redundant_bytes = report.clusters.iter().fold(0u64, |total, cluster| {
        total.saturating_add(cluster.logical_redundant_bytes)
    });
    let metadata_evidence_complete = report.clusters.iter().all(|cluster| {
        cluster
            .members
            .iter()
            .all(|member| member.production_metadata.metadata_probe_complete)
    });
    let mut production_time_source_counts = BTreeMap::new();
    for member in report.clusters.iter().flat_map(|cluster| &cluster.members) {
        *production_time_source_counts
            .entry(member.production_metadata.production_time_source.clone())
            .or_insert(0) += 1;
    }
    report.duplicate_file_count == duplicate_file_count
        && report.logical_duplicate_bytes == logical_duplicate_bytes
        && report.logical_redundant_bytes == logical_redundant_bytes
        && report.metadata_evidence_complete == metadata_evidence_complete
        && report.production_time_source_counts == production_time_source_counts
        && report.source_scope_fingerprint
            == source_scope_fingerprint(&report.source_root, report.min_bytes, report.max_entries)
        && report.audit_fingerprint
            == audit_fingerprint(
                &report.source_scope_fingerprint,
                report.evidence_complete,
                [
                    report.entries_seen,
                    report.file_count,
                    report.size_collision_candidate_count,
                    report.content_hashed_file_count,
                ],
                &report.issue_counts,
                &report.clusters,
            )
}

pub fn summarize_exact_duplicate_audit(
    report: &ExactDuplicateAuditReport,
) -> ExactDuplicateAuditSummary {
    let reclaim_plan_fingerprint = reclaim_plan_fingerprint(report);
    ExactDuplicateAuditSummary {
        schema_version: report.schema_version,
        output_mode: "exact-duplicate-audit-summary".into(),
        observed_at_ms: report.observed_at_ms,
        source_scope_fingerprint: report.source_scope_fingerprint.clone(),
        min_bytes: report.min_bytes,
        max_entries: report.max_entries,
        evidence_complete: report.evidence_complete,
        entries_seen: report.entries_seen,
        file_count: report.file_count,
        size_collision_candidate_count: report.size_collision_candidate_count,
        content_hashed_file_count: report.content_hashed_file_count,
        cluster_count: report.cluster_count,
        duplicate_file_count: report.duplicate_file_count,
        logical_duplicate_bytes: report.logical_duplicate_bytes,
        logical_redundant_bytes: report.logical_redundant_bytes,
        physical_reclaimable_bytes: None,
        metadata_evidence_complete: report.metadata_evidence_complete,
        production_time_source_counts: report.production_time_source_counts.clone(),
        issue_counts: report.issue_counts.clone(),
        audit_fingerprint: report.audit_fingerprint.clone(),
        content_digest_algorithms: vec!["blake3".into(), "sha256".into(), "quickxor".into()],
        local_paths_included: false,
        content_digests_included: false,
        production_metadata_evaluated: true,
        production_date_policy: report.production_date_policy.clone(),
        exact_content_match_is_delete_approval: false,
        requires_human_canonical_selection: report.cluster_count > 0,
        automatic_delete_allowed: false,
        mutation_performed: false,
        reclaim_plan_fingerprint,
        exact_reclaim_approval_phrase: exact_duplicate_reclaim_approval_phrase(report),
        canonical_selection_policy:
            "exact-content-quality-tie>embedded-provenance>oldest-created>relative-path".into(),
        quality_equivalence_basis: "blake3+sha256+quickxor exact encoded bytes".into(),
        notices: vec![
            "read-only-no-file-created-modified-renamed-or-deleted".into(),
            "content-hashes-and-relative-paths-redacted-from-summary".into(),
            "production-date-prefers-embedded-then-explicit-filename-then-filesystem-fallbacks"
                .into(),
            "logical-redundant-bytes-are-not-verified-physical-reclaimable-bytes".into(),
            "identical-content-does-not-prove-identical-lineage-context".into(),
            "canonical-copy-selection-requires-private-metadata-review".into(),
            "no-automatic-delete-approval-created".into(),
            "reclaim-still-requires-exact-human-approval-and-fresh-rehash".into(),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn finds_only_stable_full_content_duplicates_without_authorizing_delete() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("a.bin"), b"same exact content").unwrap();
        std::fs::create_dir(root.path().join("nested")).unwrap();
        std::fs::write(root.path().join("nested/b.bin"), b"same exact content").unwrap();
        std::fs::write(root.path().join("same-size.bin"), b"same exact contenX").unwrap();
        let report = collect_exact_duplicate_audit(root.path(), 42, 1, 100).unwrap();
        assert!(report.evidence_complete);
        assert_eq!(report.cluster_count, 1);
        assert_eq!(report.duplicate_file_count, 2);
        assert_eq!(
            report.logical_redundant_bytes,
            b"same exact content".len() as u64
        );
        assert_eq!(report.physical_reclaimable_bytes, None);
        assert!(!report.automatic_delete_allowed);
        assert!(!report.exact_content_match_is_delete_approval);
        assert!(!report.mutation_performed);
        assert_eq!(report.clusters[0].members.len(), 2);
        assert!(report.clusters[0]
            .members
            .iter()
            .all(|member| member.source_stable && !member.write_performed));
        assert!(exact_duplicate_audit_integrity_valid(&report));
        let summary = summarize_exact_duplicate_audit(&report);
        assert!(!summary.local_paths_included);
        assert!(!summary.content_digests_included);
        assert!(summary.production_metadata_evaluated);
        assert_eq!(
            summary.production_date_policy,
            "embedded>filename-explicit>filesystem-created>filesystem-modified"
        );
        assert_eq!(summary.physical_reclaimable_bytes, None);
        assert!(summary.requires_human_canonical_selection);
        assert!(summary.reclaim_plan_fingerprint.is_some());
        assert!(summary.exact_reclaim_approval_phrase.is_some());
    }

    #[cfg(all(unix, not(coverage)))]
    #[test]
    fn exact_reclaim_keeps_one_byte_identical_canonical_member() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("a.bin"), b"same exact content").unwrap();
        std::fs::write(root.path().join("b.bin"), b"same exact content").unwrap();
        let plan = collect_exact_duplicate_audit(root.path(), 42, 1, 100).unwrap();
        let phrase = exact_duplicate_reclaim_approval_phrase(&plan).unwrap();
        std::fs::write(root.path().join("unrelated.bin"), b"new unrelated content").unwrap();
        let execution = execute_exact_duplicate_reclaim_from_report(
            root.path(),
            &plan,
            &plan.audit_fingerprint,
            &phrase,
            "operator reviewed exact byte-identical copies",
            43,
        )
        .unwrap();
        assert_eq!(execution.candidate_file_count, 1);
        assert_eq!(execution.removed_file_count, 1);
        assert_eq!(execution.active_file_count, 0);
        assert_eq!(execution.failed_file_count, 0);
        assert!(execution.failure_reasons.is_empty());
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 2);
    }

    #[cfg(all(unix, not(coverage)))]
    #[test]
    fn exact_reclaim_fails_before_delete_when_retained_member_disappears() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("a.bin"), b"same exact content").unwrap();
        std::fs::write(root.path().join("b.bin"), b"same exact content").unwrap();
        let plan = collect_exact_duplicate_audit(root.path(), 42, 1, 100).unwrap();
        let retained = canonical_member(&plan.clusters[0]).relative_path.clone();
        std::fs::remove_file(root.path().join(retained)).unwrap();
        let phrase = exact_duplicate_reclaim_approval_phrase(&plan).unwrap();
        assert!(execute_exact_duplicate_reclaim_from_report(
            root.path(),
            &plan,
            &plan.audit_fingerprint,
            &phrase,
            "operator reviewed exact byte-identical copies",
            43,
        )
        .is_err());
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn identity_bound_remove_preserves_replacement_file() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("duplicate.bin");
        std::fs::write(&path, b"approved bytes").unwrap();
        let metadata = std::fs::symlink_metadata(&path).unwrap();
        let (device, inode) = unix_identity(&metadata);
        let approved = storage_identity_fingerprint_from_metadata(device, inode).unwrap();
        let original = observe_file(root.path(), path.clone(), metadata).unwrap();
        let approved_digests = hash_stable_file(&original).unwrap();

        std::fs::remove_file(&path).unwrap();
        std::fs::write(&path, b"replacement bytes").unwrap();
        let error = remove_if_storage_identity(
            &path,
            &approved,
            original.logical_bytes,
            &approved_digests,
            1,
        )
        .unwrap_err();
        assert!(matches!(
            error.as_str(),
            "duplicate-reclaim-candidate-changed" | "duplicate-reclaim-candidate-content-changed"
        ));
        assert_eq!(std::fs::read(&path).unwrap(), b"replacement bytes");
        assert_eq!(
            removal_failure_code("duplicate-reclaim-delete-failed:permission denied"),
            "duplicate-reclaim-delete-failed"
        );
    }

    #[cfg(unix)]
    #[test]
    fn staged_candidate_is_surfaced_when_original_name_is_occupied() {
        let root = tempfile::tempdir().unwrap();
        let original = root.path().join("duplicate.bin");
        let staged = root.path().join(".hidden-stage");
        std::fs::write(&original, b"replacement bytes").unwrap();
        std::fs::write(&staged, b"approved bytes").unwrap();

        assert_eq!(
            preserve_staged_candidate(&original, &staged, 77).unwrap_err(),
            "duplicate-reclaim-recovery-preserved"
        );
        assert_eq!(std::fs::read(&original).unwrap(), b"replacement bytes");
        assert_eq!(
            std::fs::read(root.path().join("duplicate.bin.disksage-recovery-77")).unwrap(),
            b"approved bytes"
        );
        assert!(!staged.exists());
    }

    #[test]
    fn production_date_priority_is_embedded_then_filename_then_created_then_modified() {
        assert_eq!(
            select_production_time(
                Some(10),
                Some("embedded:test"),
                Some("high"),
                Some(20),
                30,
                40,
            ),
            (10, "embedded:test".into(), "high".into())
        );
        assert_eq!(
            select_production_time(None, None, None, Some(20), 30, 40),
            (20, "filename:path-token".into(), "low".into())
        );
        assert_eq!(
            select_production_time(None, None, None, None, 30, 40),
            (30, "filesystem:created".into(), "low".into())
        );
        assert_eq!(
            select_production_time(None, None, None, None, 0, 40),
            (40, "filesystem:modified-fallback".into(), "low".into())
        );
    }

    #[test]
    fn entry_limit_fails_completeness_closed() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("a.bin"), b"same").unwrap();
        std::fs::write(root.path().join("b.bin"), b"same").unwrap();
        let report = collect_exact_duplicate_audit(root.path(), 42, 1, 1).unwrap();
        assert!(!report.evidence_complete);
        assert_eq!(
            report.issue_counts["duplicate-audit-entry-limit-reached"],
            1
        );
        assert!(!report.automatic_delete_allowed);
        assert!(exact_duplicate_audit_integrity_valid(&report));
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_not_followed_or_hashed() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let mut outside = tempfile::NamedTempFile::new().unwrap();
        outside
            .as_file_mut()
            .write_all(b"outside duplicate")
            .unwrap();
        std::fs::write(root.path().join("inside.bin"), b"outside duplicate").unwrap();
        symlink(outside.path(), root.path().join("outside-link.bin")).unwrap();
        let report = collect_exact_duplicate_audit(root.path(), 42, 1, 100).unwrap();
        assert!(report.evidence_complete);
        assert_eq!(report.file_count, 1);
        assert_eq!(report.cluster_count, 0);
        assert!(exact_duplicate_audit_integrity_valid(&report));
    }

    #[test]
    fn managed_photo_libraries_are_never_traversed() {
        let root = tempfile::tempdir().unwrap();
        let library = root.path().join("Photos Library.photoslibrary");
        std::fs::create_dir(&library).unwrap();
        std::fs::write(root.path().join("outside.bin"), b"same exact content").unwrap();
        std::fs::write(library.join("database.bin"), b"same exact content").unwrap();

        let report = collect_exact_duplicate_audit(root.path(), 42, 1, 100).unwrap();

        assert!(report.evidence_complete);
        assert_eq!(report.file_count, 1);
        assert_eq!(report.cluster_count, 0);
        assert_eq!(
            report.issue_counts["duplicate-audit-system-managed-photo-library-excluded"],
            1
        );
        assert!(exact_duplicate_audit_integrity_valid(&report));
        assert_eq!(
            collect_exact_duplicate_audit(&library, 42, 1, 100).unwrap_err(),
            "duplicate-audit-root-system-managed-photo-library"
        );
    }

    #[test]
    fn integrity_rejects_tampered_delete_or_fingerprint_claims() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("a.bin"), b"same").unwrap();
        std::fs::write(root.path().join("b.bin"), b"same").unwrap();
        let report = collect_exact_duplicate_audit(root.path(), 42, 1, 100).unwrap();
        assert!(exact_duplicate_audit_integrity_valid(&report));
        let mut tampered = report.clone();
        tampered.automatic_delete_allowed = true;
        assert!(!exact_duplicate_audit_integrity_valid(&tampered));
        let mut tampered = report;
        tampered.audit_fingerprint = "0".repeat(64);
        assert!(!exact_duplicate_audit_integrity_valid(&tampered));
    }

    #[test]
    fn integrity_rejects_tampered_member_evidence_even_if_parent_hashes_are_recomputed() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("a.bin"), b"same").unwrap();
        std::fs::write(root.path().join("b.bin"), b"same").unwrap();
        let mut report = collect_exact_duplicate_audit(root.path(), 42, 1, 100).unwrap();
        report.clusters[0].members[0].relative_path = "forged.bin".into();
        report.clusters[0].cluster_fingerprint = cluster_fingerprint(
            &report.clusters[0].content_digests,
            report.clusters[0].logical_bytes_per_file,
            &report.clusters[0].members,
        );
        report.audit_fingerprint = audit_fingerprint(
            &report.source_scope_fingerprint,
            report.evidence_complete,
            [
                report.entries_seen,
                report.file_count,
                report.size_collision_candidate_count,
                report.content_hashed_file_count,
            ],
            &report.issue_counts,
            &report.clusters,
        );
        assert!(!exact_duplicate_audit_integrity_valid(&report));
    }
}
