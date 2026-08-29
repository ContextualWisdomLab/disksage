//! Permanent reclaim for completed DiskSage-owned top-level shared-temporary artifacts.
//!
//! A directory name or age never grants deletion authority. The producer must seal the exact
//! directory object with a create-only completion marker; planning then rejects links, active
//! process references, sockets, locks, Git worktrees, and database-shaped data before issuing an
//! exact approval phrase. Execution repeats the complete plan and records real volume availability.

use crate::git_worktree::GitWorktreeActiveUseEvidence;
use serde::{Deserialize, Serialize};
use std::fs::{File, Metadata, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};

pub const SHARED_TEMP_RECLAIM_VERSION: u32 = 1;
pub const COMPLETION_MARKER_NAME: &str = ".disksage-completed-temp-v1.json";
const MAX_ENTRIES: usize = 250_000;
const MAX_APPROVAL_AGE_MS: u64 = 5 * 60 * 1_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompletedTempArtifactEvidence {
    pub schema_kind: String,
    pub version: u32,
    pub producer: String,
    pub completed_at_ms: u64,
    pub root_object_id: String,
    pub completion_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedTempReclaimPlan {
    pub schema_kind: String,
    pub version: u32,
    pub generated_at_ms: u64,
    pub shared_root: String,
    pub path: String,
    pub root_object_id: String,
    pub producer: String,
    pub completion_id: String,
    pub allocated_bytes: u64,
    pub entry_count: u64,
    pub tree_fingerprint: String,
    pub active_use: GitWorktreeActiveUseEvidence,
    pub plan_fingerprint: String,
    pub exact_approval_phrase: Option<String>,
    pub eligible_after_human_approval: bool,
    pub blockers: Vec<String>,
    pub filesystem_mutation_executed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedTempReclaimApproval {
    pub version: u32,
    pub approval_id: String,
    pub plan_fingerprint: String,
    pub exact_approval_phrase: String,
    pub approved_at_ms: u64,
    pub approved_by: String,
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedTempReclaimReceipt {
    pub schema_kind: String,
    pub version: u32,
    pub receipt_id: String,
    pub approval_id: String,
    pub plan_fingerprint: String,
    pub completed_at_ms: u64,
    pub allocated_bytes_upper_bound: u64,
    pub before_available_bytes: u64,
    pub after_available_bytes: u64,
    pub observed_available_gain_bytes: u64,
    pub path_absence_verified: bool,
    pub permanent_delete_executed: bool,
}

#[derive(Default)]
struct TreeEvidence {
    allocated_bytes: u64,
    entry_count: u64,
    fingerprint: String,
    blockers: Vec<String>,
}

fn hash_field(hasher: &mut blake3::Hasher, value: &[u8]) {
    hasher.update(&(value.len() as u64).to_le_bytes());
    hasher.update(value);
}

fn completion_id(path: &Path, object_id: &str, producer: &str, completed_at_ms: u64) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage.completed-temp-artifact.v1\0");
    hash_field(&mut hasher, path.as_os_str().to_string_lossy().as_bytes());
    hash_field(&mut hasher, object_id.as_bytes());
    hash_field(&mut hasher, producer.as_bytes());
    hasher.update(&completed_at_ms.to_le_bytes());
    hasher.finalize().to_hex().to_string()
}

fn valid_text(value: &str, max_chars: usize) -> bool {
    !value.trim().is_empty()
        && value == value.trim()
        && value.chars().count() <= max_chars
        && !value.chars().any(char::is_control)
}

fn canonical_top_level(path: &Path) -> Result<(PathBuf, PathBuf), String> {
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir))
    {
        return Err("shared-temp-path-invalid".into());
    }
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| "shared-temp-artifact-unavailable".to_string())?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("shared-temp-artifact-not-real-directory".into());
    }
    let canonical =
        std::fs::canonicalize(path).map_err(|_| "shared-temp-artifact-unavailable".to_string())?;
    let parent = canonical
        .parent()
        .ok_or_else(|| "shared-temp-artifact-not-top-level".to_string())?;
    let shared_root = std::fs::canonicalize(std::env::temp_dir())
        .ok()
        .filter(|root| root == Path::new("/private/tmp") || root == Path::new("/tmp"))
        .unwrap_or_else(|| {
            std::fs::canonicalize(if cfg!(target_os = "macos") {
                Path::new("/private/tmp")
            } else {
                Path::new("/tmp")
            })
            .unwrap_or_else(|_| PathBuf::from("/tmp"))
        });
    if parent != shared_root || canonical == shared_root {
        return Err("shared-temp-artifact-not-top-level".into());
    }
    Ok((shared_root, canonical))
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

#[cfg(unix)]
fn change_metadata_token(metadata: &Metadata) -> String {
    use std::os::unix::fs::MetadataExt;
    format!(
        "{}:{}:{}:{}:{}:{}:{}",
        metadata.mode(),
        metadata.uid(),
        metadata.gid(),
        metadata.mtime(),
        metadata.mtime_nsec(),
        metadata.ctime(),
        metadata.ctime_nsec()
    )
}

#[cfg(not(unix))]
fn change_metadata_token(metadata: &Metadata) -> String {
    metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| format!("{}:{}", duration.as_secs(), duration.subsec_nanos()))
        .unwrap_or_else(|| "modified-time-unavailable".into())
}

fn database_or_worktree_marker(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    if matches!(
        name,
        ".git" | "PG_VERSION" | "postmaster.pid" | "WiredTiger"
    ) {
        return true;
    }
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "db" | "sqlite" | "sqlite3" | "duckdb" | "mdb"
            )
        })
}

fn socket_or_lock(path: &Path, metadata: &Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileTypeExt;
        if metadata.file_type().is_socket() {
            return true;
        }
    }
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    name.ends_with(".lock") || name.ends_with(".pid")
}

fn tree_evidence(root: &Path) -> TreeEvidence {
    let mut evidence = TreeEvidence::default();
    let mut records = Vec::new();
    let mut walker = walkdir::WalkDir::new(root).follow_links(false).into_iter();
    while let Some(entry) = walker.next() {
        if records.len() >= MAX_ENTRIES {
            evidence
                .blockers
                .push("shared-temp-tree-entry-limit-reached".into());
            break;
        }
        let Ok(entry) = entry else {
            evidence
                .blockers
                .push("shared-temp-tree-enumeration-incomplete".into());
            continue;
        };
        let path = entry.path();
        if entry.file_type().is_symlink() {
            evidence
                .blockers
                .push("shared-temp-tree-link-present".into());
            if entry.file_type().is_dir() {
                walker.skip_current_dir();
            }
            continue;
        }
        let Ok(metadata) = std::fs::symlink_metadata(path) else {
            evidence
                .blockers
                .push("shared-temp-tree-metadata-incomplete".into());
            continue;
        };
        if database_or_worktree_marker(path) {
            evidence
                .blockers
                .push("shared-temp-worktree-or-database-data-present".into());
        }
        if socket_or_lock(path, &metadata) {
            evidence
                .blockers
                .push("shared-temp-socket-or-lock-present".into());
        }
        let identity = match crate::safety::filesystem_object_id(path) {
            Ok(identity) => identity,
            Err(_) => {
                evidence
                    .blockers
                    .push("shared-temp-tree-identity-incomplete".into());
                String::new()
            }
        };
        let relative = path.strip_prefix(root).unwrap_or(path).to_string_lossy();
        let kind = if metadata.is_dir() {
            "d"
        } else if metadata.is_file() {
            "f"
        } else {
            "o"
        };
        let bytes = allocated_bytes(&metadata);
        let change_metadata = change_metadata_token(&metadata);
        evidence.allocated_bytes = evidence.allocated_bytes.saturating_add(bytes);
        evidence.entry_count = evidence.entry_count.saturating_add(1);
        records.push(format!(
            "{kind}\0{relative}\0{identity}\0{}\0{bytes}\0{change_metadata}",
            metadata.len()
        ));
    }
    records.sort_unstable();
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage.shared-temp-tree.v1\0");
    for record in records {
        hash_field(&mut hasher, record.as_bytes());
    }
    evidence.fingerprint = hasher.finalize().to_hex().to_string();
    evidence.blockers.sort();
    evidence.blockers.dedup();
    evidence
}

fn read_completion_marker(
    path: &Path,
    object_id: &str,
) -> Result<CompletedTempArtifactEvidence, String> {
    let marker_path = path.join(COMPLETION_MARKER_NAME);
    let metadata = std::fs::symlink_metadata(&marker_path)
        .map_err(|_| "shared-temp-completion-marker-missing".to_string())?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 16 * 1024 {
        return Err("shared-temp-completion-marker-unsafe".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o777 != 0o400 {
            return Err("shared-temp-completion-marker-mode-invalid".into());
        }
    }
    let marker: CompletedTempArtifactEvidence = serde_json::from_reader(
        File::open(&marker_path)
            .map_err(|_| "shared-temp-completion-marker-open-failed".to_string())?,
    )
    .map_err(|_| "shared-temp-completion-marker-invalid".to_string())?;
    if marker.schema_kind != "disksage.completed-temp-artifact"
        || marker.version != SHARED_TEMP_RECLAIM_VERSION
        || marker.root_object_id != object_id
        || !valid_text(&marker.producer, 128)
        || marker.completion_id
            != completion_id(path, object_id, &marker.producer, marker.completed_at_ms)
    {
        return Err("shared-temp-completion-marker-invalid".into());
    }
    Ok(marker)
}

/// Seal a completed artifact created by a DiskSage producer. Existing markers are never replaced.
#[cfg(unix)]
pub fn seal_completed_temp_artifact(
    path: &Path,
    producer: &str,
    completed_at_ms: u64,
) -> Result<CompletedTempArtifactEvidence, String> {
    use std::os::unix::fs::OpenOptionsExt;
    if !valid_text(producer, 128) {
        return Err("shared-temp-producer-invalid".into());
    }
    let (_, path) = canonical_top_level(path)?;
    if !crate::safety::is_user_owned_shared_temp_tree(&path) {
        return Err("shared-temp-ownership-evidence-incomplete".into());
    }
    let object_id = crate::safety::filesystem_object_id(&path)
        .map_err(|_| "shared-temp-object-identity-unavailable".to_string())?;
    let marker = CompletedTempArtifactEvidence {
        schema_kind: "disksage.completed-temp-artifact".into(),
        version: SHARED_TEMP_RECLAIM_VERSION,
        producer: producer.into(),
        completed_at_ms,
        root_object_id: object_id.clone(),
        completion_id: completion_id(&path, &object_id, producer, completed_at_ms),
    };
    let encoded = serde_json::to_vec_pretty(&marker)
        .map_err(|_| "shared-temp-completion-marker-invalid".to_string())?;
    let marker_path = path.join(COMPLETION_MARKER_NAME);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o400)
        .open(&marker_path)
        .map_err(|_| "shared-temp-completion-marker-create-failed".to_string())?;
    file.write_all(&encoded)
        .and_then(|_| file.sync_all())
        .map_err(|_| "shared-temp-completion-marker-write-failed".to_string())?;
    File::open(&path)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| "shared-temp-completion-marker-sync-failed".to_string())?;
    Ok(marker)
}

#[cfg(not(unix))]
pub fn seal_completed_temp_artifact(
    _path: &Path,
    _producer: &str,
    _completed_at_ms: u64,
) -> Result<CompletedTempArtifactEvidence, String> {
    Err("shared-temp-reclaim-unsupported-platform".into())
}

fn plan_fingerprint(plan: &SharedTempReclaimPlan) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage.shared-temp-reclaim-plan.v1\0");
    for value in [
        &plan.shared_root,
        &plan.path,
        &plan.root_object_id,
        &plan.producer,
        &plan.completion_id,
        &plan.tree_fingerprint,
    ] {
        hash_field(&mut hasher, value.as_bytes());
    }
    hasher.update(&plan.allocated_bytes.to_le_bytes());
    hasher.update(&plan.entry_count.to_le_bytes());
    for blocker in &plan.blockers {
        hash_field(&mut hasher, blocker.as_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

/// Build read-only reclaim evidence for one exact top-level completed artifact.
pub fn plan_shared_temp_reclaim(
    path: &Path,
    generated_at_ms: u64,
) -> Result<SharedTempReclaimPlan, String> {
    let (shared_root, path) = canonical_top_level(path)?;
    let object_id = crate::safety::filesystem_object_id(&path)
        .map_err(|_| "shared-temp-object-identity-unavailable".to_string())?;
    let marker = read_completion_marker(&path, &object_id)?;
    let tree = tree_evidence(&path);
    let active_use = crate::git_worktree::active_use_evidence(
        &path,
        crate::reclaim::ACTIVE_USE_PROBE_TIMEOUT_MS,
        crate::reclaim::ACTIVE_USE_PROBE_MAX_PIDS,
        true,
    );
    let mut blockers = tree.blockers;
    if !crate::safety::is_user_owned_shared_temp_tree(&path) {
        blockers.push("shared-temp-ownership-evidence-incomplete".into());
    }
    if !active_use.assessed
        || !active_use.evidence_complete
        || active_use.error.is_some()
        || active_use.results_truncated
    {
        blockers.push("shared-temp-active-use-evidence-incomplete".into());
    } else if active_use.active {
        blockers.push("shared-temp-active-use-detected".into());
    }
    blockers.sort();
    blockers.dedup();
    let mut plan = SharedTempReclaimPlan {
        schema_kind: "disksage.shared-temp-reclaim-plan".into(),
        version: SHARED_TEMP_RECLAIM_VERSION,
        generated_at_ms,
        shared_root: shared_root.to_string_lossy().into_owned(),
        path: path.to_string_lossy().into_owned(),
        root_object_id: object_id,
        producer: marker.producer,
        completion_id: marker.completion_id,
        allocated_bytes: tree.allocated_bytes,
        entry_count: tree.entry_count,
        tree_fingerprint: tree.fingerprint,
        active_use,
        plan_fingerprint: String::new(),
        exact_approval_phrase: None,
        eligible_after_human_approval: blockers.is_empty(),
        blockers,
        filesystem_mutation_executed: false,
    };
    plan.plan_fingerprint = plan_fingerprint(&plan);
    if plan.eligible_after_human_approval {
        plan.exact_approval_phrase = Some(format!(
            "DiskSage completed temp artifact 1 {} 승인 {}",
            plan.allocated_bytes, plan.plan_fingerprint
        ));
    }
    Ok(plan)
}

fn approval_id(plan: &SharedTempReclaimPlan, approved_at_ms: u64, approved_by: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage.shared-temp-reclaim-approval.v1\0");
    hash_field(&mut hasher, plan.plan_fingerprint.as_bytes());
    hash_field(&mut hasher, approved_by.as_bytes());
    hasher.update(&approved_at_ms.to_le_bytes());
    hasher.finalize().to_hex().to_string()
}

pub fn approve_shared_temp_reclaim(
    plan: &SharedTempReclaimPlan,
    exact_phrase: &str,
    approved_at_ms: u64,
    approved_by: &str,
    rationale: &str,
) -> Result<SharedTempReclaimApproval, String> {
    if !plan.eligible_after_human_approval
        || plan.exact_approval_phrase.as_deref() != Some(exact_phrase)
        || approved_at_ms < plan.generated_at_ms
    {
        return Err("shared-temp-approval-plan-mismatch".into());
    }
    if !valid_text(approved_by, 256)
        || !approved_by.starts_with("human:")
        || !valid_text(rationale, 1_000)
    {
        return Err("shared-temp-approval-text-invalid".into());
    }
    Ok(SharedTempReclaimApproval {
        version: SHARED_TEMP_RECLAIM_VERSION,
        approval_id: approval_id(plan, approved_at_ms, approved_by),
        plan_fingerprint: plan.plan_fingerprint.clone(),
        exact_approval_phrase: exact_phrase.into(),
        approved_at_ms,
        approved_by: approved_by.into(),
        rationale: rationale.into(),
    })
}

fn receipt_id(receipt: &SharedTempReclaimReceipt) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage.shared-temp-reclaim-receipt.v1\0");
    for value in [&receipt.approval_id, &receipt.plan_fingerprint] {
        hash_field(&mut hasher, value.as_bytes());
    }
    for value in [
        receipt.completed_at_ms,
        receipt.allocated_bytes_upper_bound,
        receipt.before_available_bytes,
        receipt.after_available_bytes,
        receipt.observed_available_gain_bytes,
    ] {
        hasher.update(&value.to_le_bytes());
    }
    hasher.update(&[
        receipt.path_absence_verified as u8,
        receipt.permanent_delete_executed as u8,
    ]);
    hasher.finalize().to_hex().to_string()
}

#[cfg(unix)]
fn private_destination_path(
    artifact_path: &Path,
    destination: &Path,
    error: &str,
) -> Result<PathBuf, String> {
    use std::os::unix::fs::PermissionsExt;
    if !destination.is_absolute()
        || destination
            .components()
            .any(|part| matches!(part, Component::ParentDir))
        || destination.starts_with(artifact_path)
    {
        return Err(error.into());
    }
    let parent = destination.parent().filter(|parent| !parent.as_os_str().is_empty()).ok_or_else(|| error.to_string())?;
    let parent_metadata = std::fs::symlink_metadata(parent).map_err(|_| error.to_string())?;
    if !parent_metadata.is_dir()
        || parent_metadata.file_type().is_symlink()
        || parent_metadata.permissions().mode() & 0o022 != 0
    {
        return Err(error.into());
    }
    let canonical_parent = std::fs::canonicalize(parent).map_err(|_| error.to_string())?;
    if canonical_parent.starts_with(artifact_path) {
        return Err(error.into());
    }
    let file_name = destination
        .file_name()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| error.to_string())?;
    Ok(canonical_parent.join(file_name))
}

#[cfg(unix)]
fn validate_receipt_path(artifact_path: &Path, receipt_path: &Path) -> Result<PathBuf, String> {
    let resolved = private_destination_path(
        artifact_path,
        receipt_path,
        "shared-temp-receipt-parent-unsafe",
    )?;
    if std::fs::symlink_metadata(&resolved).is_ok() {
        return Err("shared-temp-receipt-path-invalid".into());
    }
    Ok(resolved)
}

#[cfg(unix)]
fn validate_journal_path(artifact_path: &Path, journal_path: &Path) -> Result<PathBuf, String> {
    use std::os::unix::fs::PermissionsExt;
    let resolved = private_destination_path(
        artifact_path,
        journal_path,
        "shared-temp-journal-path-unsafe",
    )?;
    match std::fs::symlink_metadata(&resolved) {
        Ok(metadata)
            if metadata.is_file()
                && !metadata.file_type().is_symlink()
                && metadata.permissions().mode() & 0o022 == 0 =>
        {
            Ok(resolved)
        }
        Ok(_) => Err("shared-temp-journal-path-unsafe".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(resolved),
        Err(_) => Err("shared-temp-journal-path-unsafe".into()),
    }
}

/// Freshly revalidate, permanently remove, and persist a create-only read-only receipt.
#[cfg(unix)]
pub fn execute_shared_temp_reclaim(
    approved_plan: &SharedTempReclaimPlan,
    approval: &SharedTempReclaimApproval,
    journal_path: &Path,
    receipt_path: &Path,
    requested_at_ms: u64,
) -> Result<SharedTempReclaimReceipt, String> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    if approval.version != SHARED_TEMP_RECLAIM_VERSION
        || approval.plan_fingerprint != approved_plan.plan_fingerprint
        || approved_plan.exact_approval_phrase.as_deref() != Some(&approval.exact_approval_phrase)
        || requested_at_ms < approval.approved_at_ms
        || requested_at_ms.saturating_sub(approval.approved_at_ms) > MAX_APPROVAL_AGE_MS
    {
        return Err("shared-temp-execution-approval-invalid-or-stale".into());
    }
    let live = plan_shared_temp_reclaim(Path::new(&approved_plan.path), requested_at_ms)?;
    if live.plan_fingerprint != approved_plan.plan_fingerprint
        || live.root_object_id != approved_plan.root_object_id
        || !live.eligible_after_human_approval
    {
        return Err("shared-temp-live-plan-mismatch".into());
    }
    let artifact_path = Path::new(&live.path);
    let shared_root = Path::new(&live.shared_root);
    let receipt_path = validate_receipt_path(artifact_path, receipt_path)?;
    let journal_path = validate_journal_path(artifact_path, journal_path)?;
    let before = crate::volume_pressure::snapshot_volume(shared_root, requested_at_ms)
        .map_err(|_| "shared-temp-volume-before-unavailable".to_string())?;
    crate::safety::permanent_delete_dir_if_identity(
        artifact_path,
        &live.root_object_id,
        live.allocated_bytes,
        &journal_path,
        requested_at_ms,
    )
    .map_err(|error| format!("shared-temp-permanent-delete-failed:{error}"))?;
    let path_absence_verified = !artifact_path.exists();
    if !path_absence_verified {
        return Err("shared-temp-path-still-present".into());
    }
    let completed_at_ms = crate::cloud::system_now_ms();
    let after = crate::volume_pressure::snapshot_volume(shared_root, completed_at_ms)
        .map_err(|_| "shared-temp-volume-after-unavailable".to_string())?;
    let mut receipt = SharedTempReclaimReceipt {
        schema_kind: "disksage.shared-temp-reclaim-receipt".into(),
        version: SHARED_TEMP_RECLAIM_VERSION,
        receipt_id: String::new(),
        approval_id: approval.approval_id.clone(),
        plan_fingerprint: live.plan_fingerprint,
        completed_at_ms,
        allocated_bytes_upper_bound: live.allocated_bytes,
        before_available_bytes: before.available_bytes,
        after_available_bytes: after.available_bytes,
        observed_available_gain_bytes: after.available_bytes.saturating_sub(before.available_bytes),
        path_absence_verified,
        permanent_delete_executed: true,
    };
    receipt.receipt_id = receipt_id(&receipt);
    let encoded = serde_json::to_vec_pretty(&receipt)
        .map_err(|_| "shared-temp-receipt-json-invalid".to_string())?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o400)
        .open(&receipt_path)
        .map_err(|_| "shared-temp-receipt-create-failed".to_string())?;
    file.write_all(&encoded)
        .and_then(|_| file.sync_all())
        .map_err(|_| "shared-temp-receipt-write-failed".to_string())?;
    if file
        .metadata()
        .map_err(|_| "shared-temp-receipt-metadata-failed".to_string())?
        .permissions()
        .mode()
        & 0o777
        != 0o400
    {
        return Err("shared-temp-receipt-mode-invalid".into());
    }
    File::open(receipt_path.parent().unwrap())
        .and_then(|directory| directory.sync_all())
        .map_err(|_| "shared-temp-receipt-parent-sync-failed".to_string())?;
    Ok(receipt)
}

#[cfg(not(unix))]
pub fn execute_shared_temp_reclaim(
    _approved_plan: &SharedTempReclaimPlan,
    _approval: &SharedTempReclaimApproval,
    _journal_path: &Path,
    _receipt_path: &Path,
    _requested_at_ms: u64,
) -> Result<SharedTempReclaimReceipt, String> {
    Err("shared-temp-reclaim-unsupported-platform".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn artifact(name: &str) -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix(name)
            .tempdir_in(if cfg!(target_os = "macos") {
                "/private/tmp"
            } else {
                "/tmp"
            })
            .unwrap()
    }

    #[cfg(unix)]
    #[test]
    fn completed_artifact_requires_exact_fresh_tree() {
        let artifact = artifact("disksage-completed-");
        std::fs::write(artifact.path().join("result.bin"), b"done").unwrap();
        seal_completed_temp_artifact(artifact.path(), "disksage:test", 10).unwrap();
        let plan = plan_shared_temp_reclaim(artifact.path(), 11).unwrap();
        assert!(plan.eligible_after_human_approval, "{:?}", plan.blockers);
        let phrase = plan.exact_approval_phrase.clone().unwrap();
        let approval =
            approve_shared_temp_reclaim(&plan, &phrase, 12, "human:test", "verified completion")
                .unwrap();
        std::fs::write(artifact.path().join("result.bin"), b"changed").unwrap();
        let private = tempfile::tempdir().unwrap();
        assert_eq!(
            execute_shared_temp_reclaim(
                &plan,
                &approval,
                &private.path().join("journal.jsonl"),
                &private.path().join("receipt.json"),
                13
            )
            .unwrap_err(),
            "shared-temp-live-plan-mismatch"
        );
        assert!(artifact.path().exists());
    }

    #[cfg(unix)]
    #[test]
    fn worktree_lock_socket_and_link_never_receive_authority() {
        use std::os::unix::net::UnixListener;
        let artifact = artifact("disksage-blocked-");
        std::fs::create_dir(artifact.path().join(".git")).unwrap();
        std::fs::write(artifact.path().join("state.lock"), b"locked").unwrap();
        let _socket = UnixListener::bind(artifact.path().join("service.sock")).unwrap();
        std::os::unix::fs::symlink("state.lock", artifact.path().join("escape-link")).unwrap();
        seal_completed_temp_artifact(artifact.path(), "disksage:test", 10).unwrap();
        let plan = plan_shared_temp_reclaim(artifact.path(), 11).unwrap();
        assert!(!plan.eligible_after_human_approval);
        assert!(plan
            .blockers
            .contains(&"shared-temp-worktree-or-database-data-present".into()));
        assert!(plan
            .blockers
            .contains(&"shared-temp-socket-or-lock-present".into()));
        assert!(plan
            .blockers
            .contains(&"shared-temp-tree-link-present".into()));
    }

    #[cfg(unix)]
    #[test]
    fn successful_reclaim_records_real_volume_observation_and_read_only_receipt() {
        use std::os::unix::fs::PermissionsExt;
        let artifact = artifact("disksage-reclaimable-");
        std::fs::write(artifact.path().join("result.bin"), vec![1_u8; 16 * 1024]).unwrap();
        seal_completed_temp_artifact(artifact.path(), "disksage:test", 10).unwrap();
        let plan = plan_shared_temp_reclaim(artifact.path(), 11).unwrap();
        assert!(plan.eligible_after_human_approval, "{:?}", plan.blockers);
        let approval = approve_shared_temp_reclaim(
            &plan,
            plan.exact_approval_phrase.as_deref().unwrap(),
            12,
            "human:test",
            "producer completion verified",
        )
        .unwrap();
        let path = artifact.keep();
        let private = tempfile::tempdir().unwrap();
        let journal = private.path().join(format!(
            "disksage-shared-temp-journal-{}-{}.jsonl",
            std::process::id(),
            &plan.plan_fingerprint[..12]
        ));
        let receipt_path = private.path().join("receipt.json");
        let receipt =
            execute_shared_temp_reclaim(&plan, &approval, &journal, &receipt_path, 13).unwrap();
        assert!(!path.exists());
        assert!(receipt.permanent_delete_executed && receipt.path_absence_verified);
        assert!(receipt.before_available_bytes > 0 && receipt.after_available_bytes > 0);
        assert_eq!(
            std::fs::metadata(&receipt_path)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o400
        );
        let persisted: SharedTempReclaimReceipt =
            serde_json::from_reader(File::open(&receipt_path).unwrap()).unwrap();
        assert_eq!(persisted.receipt_id, receipt.receipt_id);
        let _ = std::fs::remove_file(journal);
    }
}
