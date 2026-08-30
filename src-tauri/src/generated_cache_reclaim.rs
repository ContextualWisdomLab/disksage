//! Evidence-bound auditing and removal of explicitly regenerable cache roots.

use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub const GENERATED_CACHE_SCHEMA_VERSION: u32 = 1;
const MAX_ENTRIES: u64 = 200_000;
const MAX_HASHED_CONTENT_BYTES: u64 = 512 * 1024 * 1024 * 1024;
const MAX_APPROVAL_AGE_MS: u64 = 15 * 60 * 1_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RegenerationContract {
    TorchHubDownload,
    HomebrewApiMetadata,
    HomebrewBootsnap,
    UvPackageCache,
    PlaywrightBrowserDownload,
    TemporaryGitWorkspace,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneratedCacheActivityEvidence {
    pub evidence_complete: bool,
    pub open_pids: Vec<u32>,
    pub tool_lock_paths: Vec<String>,
    pub live_cwd_present: bool,
    pub git_common_dir: Option<String>,
    pub git_worktree_registered: bool,
    pub git_dirty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneratedCachePlan {
    pub schema_version: u32,
    pub root: String,
    pub contract: RegenerationContract,
    pub allocated_bytes: u64,
    pub entry_count: u64,
    pub content_fingerprint: String,
    pub activity: GeneratedCacheActivityEvidence,
    pub blockers: Vec<String>,
    pub observed_at_ms: u64,
    pub plan_fingerprint: String,
    pub exact_approval_phrase: String,
    pub dry_run: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneratedCacheApproval {
    pub plan_fingerprint: String,
    pub approved_at_ms: u64,
    pub approved_by: String,
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneratedCacheReceipt {
    pub schema_version: u32,
    pub plan_fingerprint: String,
    pub root: String,
    pub attempted_at_ms: u64,
    pub allocated_before_bytes: u64,
    pub removed: bool,
    pub error_code: Option<String>,
    pub provider_data_mutated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneratedCachePendingReceipt {
    pub schema_version: u32,
    pub plan_fingerprint: String,
    pub root: String,
    pub attempted_at_ms: u64,
    pub allocated_before_bytes: u64,
    pub state: String,
    pub provider_data_mutated: bool,
}

fn deny_boundary(path: &Path) -> bool {
    let value = path.to_string_lossy();
    [
        "/Library/Mobile Documents/",
        "/Library/CloudStorage/",
        "/Library/Application Support/OneDrive/",
        "/Pictures/Photos Library.photoslibrary/",
        "/Library/Containers/com.apple.Photos/",
        "/Library/Containers/com.docker.docker/",
        "/.local/share/containers/",
        "/.colima/",
        "/Parallels/",
    ]
    .iter()
    .any(|marker| value.contains(marker))
}

fn hash_reader_bounded<R: Read>(
    reader: &mut R,
    hasher: &mut blake3::Hasher,
    hashed_bytes: &mut u64,
    limit: u64,
) -> Result<(), String> {
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|_| "generated-cache-file-unreadable".to_string())?;
        if read == 0 {
            return Ok(());
        }
        *hashed_bytes = hashed_bytes
            .checked_add(read as u64)
            .ok_or("generated-cache-content-bound-exceeded")?;
        if *hashed_bytes > limit {
            return Err("generated-cache-content-bound-exceeded".into());
        }
        hasher.update(&buffer[..read]);
    }
}

pub fn regeneration_contract(path: &Path, home: &Path) -> Option<RegenerationContract> {
    let pairs = [
        (
            home.join(".cache/torch"),
            RegenerationContract::TorchHubDownload,
        ),
        (
            home.join("Library/Caches/Homebrew/api"),
            RegenerationContract::HomebrewApiMetadata,
        ),
        (
            home.join("Library/Caches/Homebrew/bootsnap"),
            RegenerationContract::HomebrewBootsnap,
        ),
        (home.join(".cache/uv"), RegenerationContract::UvPackageCache),
        (
            home.join("Library/Caches/ms-playwright"),
            RegenerationContract::PlaywrightBrowserDownload,
        ),
    ];
    pairs
        .into_iter()
        .find_map(|(candidate, contract)| (path == candidate).then_some(contract))
        .or_else(|| {
            path.starts_with("/private/tmp")
                .then_some(RegenerationContract::TemporaryGitWorkspace)
        })
}

#[cfg(unix)]
fn path_bytes(path: &std::ffi::OsStr) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    path.as_bytes().to_vec()
}

#[cfg(windows)]
fn path_bytes(path: &std::ffi::OsStr) -> Vec<u8> {
    use std::os::windows::ffi::OsStrExt;
    path.encode_wide().flat_map(u16::to_le_bytes).collect()
}

#[cfg(unix)]
fn update_platform_metadata(hasher: &mut blake3::Hasher, metadata: &std::fs::Metadata) {
    use std::os::unix::fs::MetadataExt;
    hasher.update(&metadata.dev().to_le_bytes());
    hasher.update(&metadata.ino().to_le_bytes());
    hasher.update(&metadata.mode().to_le_bytes());
    hasher.update(&metadata.uid().to_le_bytes());
    hasher.update(&metadata.gid().to_le_bytes());
    hasher.update(&metadata.mtime().to_le_bytes());
    hasher.update(&metadata.mtime_nsec().to_le_bytes());
}

#[cfg(windows)]
fn update_platform_metadata(hasher: &mut blake3::Hasher, metadata: &std::fs::Metadata) {
    use std::os::windows::fs::MetadataExt;
    hasher.update(&metadata.creation_time().to_le_bytes());
    hasher.update(&metadata.last_write_time().to_le_bytes());
    hasher.update(&metadata.file_attributes().to_le_bytes());
    hasher.update(&metadata.file_size().to_le_bytes());
}

#[cfg(unix)]
fn allocated_bytes(metadata: &std::fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    metadata.blocks().saturating_mul(512)
}

#[cfg(windows)]
fn allocated_bytes(metadata: &std::fs::Metadata) -> u64 {
    metadata.len()
}

fn observe_tree(path: &Path) -> Result<(u64, u64, String, Vec<String>), String> {
    let mut stack = vec![path.to_path_buf()];
    let mut bytes = 0_u64;
    let mut entries = 0_u64;
    let mut estimated_content_bytes = 0_u64;
    let mut hashed_content_bytes = 0_u64;
    let mut locks = Vec::new();
    let mut hasher = blake3::Hasher::new();
    while let Some(current) = stack.pop() {
        if entries >= MAX_ENTRIES {
            return Err("generated-cache-entry-limit".into());
        }
        let metadata = std::fs::symlink_metadata(&current)
            .map_err(|_| "generated-cache-metadata-unavailable".to_string())?;
        let is_symlink = metadata.file_type().is_symlink();
        // The allowlisted cache root itself must remain a real directory. Symlinks below that
        // root are ordinary cache entries: fingerprint the link text without resolving or
        // traversing it, so a link can never extend the deletion boundary.
        if current == path && is_symlink {
            return Err("generated-cache-root-symlink-rejected".into());
        }
        if !metadata.is_dir() && !metadata.is_file() && !is_symlink {
            return Err("generated-cache-special-file-rejected".into());
        }
        entries += 1;
        bytes = bytes.saturating_add(allocated_bytes(&metadata));
        let relative = current
            .strip_prefix(path)
            .map_err(|_| "generated-cache-relative-path-failed")?;
        let relative_bytes = path_bytes(relative.as_os_str());
        hasher.update(&(relative_bytes.len() as u64).to_le_bytes());
        hasher.update(&relative_bytes);
        hasher.update(if is_symlink {
            b"symlink"
        } else if metadata.is_dir() {
            b"directory"
        } else {
            b"file"
        });
        hasher.update(&metadata.len().to_le_bytes());
        update_platform_metadata(&mut hasher, &metadata);
        if current.file_name().is_some_and(|name| name == ".lock") {
            locks.push(current.to_string_lossy().into_owned());
        }
        if is_symlink {
            let target = std::fs::read_link(&current)
                .map_err(|_| "generated-cache-symlink-unreadable".to_string())?;
            let target_bytes = path_bytes(target.as_os_str());
            hasher.update(&(target_bytes.len() as u64).to_le_bytes());
            hasher.update(&target_bytes);
        } else if metadata.is_file() {
            estimated_content_bytes = estimated_content_bytes
                .checked_add(metadata.len())
                .ok_or("generated-cache-content-bound-exceeded")?;
            if estimated_content_bytes > MAX_HASHED_CONTENT_BYTES {
                return Err("generated-cache-content-bound-exceeded".into());
            }
            let mut file = std::fs::File::open(&current)
                .map_err(|_| "generated-cache-file-unreadable".to_string())?;
            hash_reader_bounded(
                &mut file,
                &mut hasher,
                &mut hashed_content_bytes,
                MAX_HASHED_CONTENT_BYTES,
            )?;
        } else {
            let mut children = std::fs::read_dir(&current)
                .map_err(|_| "generated-cache-directory-unreadable".to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| "generated-cache-entry-unreadable".to_string())?;
            children.sort_by_key(std::fs::DirEntry::file_name);
            stack.extend(children.into_iter().rev().map(|entry| entry.path()));
        }
    }
    Ok((
        bytes,
        entries,
        hasher.finalize().to_hex().to_string(),
        locks,
    ))
}

pub fn plan_with_evidence(
    path: &Path,
    home: &Path,
    mut activity: GeneratedCacheActivityEvidence,
    observed_at_ms: u64,
) -> Result<GeneratedCachePlan, String> {
    if !path.is_absolute() || !home.is_absolute() || deny_boundary(path) {
        return Err("generated-cache-boundary-denied".into());
    }
    let contract = regeneration_contract(path, home)
        .ok_or_else(|| "generated-cache-regeneration-contract-missing".to_string())?;
    let (allocated_bytes, entry_count, content_fingerprint, locks) = observe_tree(path)?;
    activity.tool_lock_paths = locks;
    let mut blockers = Vec::new();
    if !activity.evidence_complete {
        blockers.push("active-use-evidence-incomplete".into());
    }
    if !activity.open_pids.is_empty() || activity.live_cwd_present {
        blockers.push("process-active".into());
    }
    if !activity.tool_lock_paths.is_empty() {
        blockers.push("tool-lock-present".into());
    }
    if matches!(contract, RegenerationContract::TemporaryGitWorkspace)
        && (activity.git_worktree_registered || activity.git_dirty)
    {
        blockers.push("git-workspace-retained".into());
    }
    if matches!(contract, RegenerationContract::TemporaryGitWorkspace) {
        blockers.push("temporary-workspace-specialized-executor-required".into());
    }
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage.generated-cache-plan\0v1\0");
    hasher.update(path.as_os_str().to_string_lossy().as_bytes());
    hasher.update(content_fingerprint.as_bytes());
    hasher.update(&serde_json::to_vec(&activity).map_err(|_| "generated-cache-plan-encode")?);
    let plan_fingerprint = hasher.finalize().to_hex().to_string();
    Ok(GeneratedCachePlan {
        schema_version: GENERATED_CACHE_SCHEMA_VERSION,
        root: path.to_string_lossy().into_owned(),
        contract,
        allocated_bytes,
        entry_count,
        content_fingerprint,
        activity,
        blockers,
        observed_at_ms,
        exact_approval_phrase: format!("DiskSage generated cache 제거 승인 {plan_fingerprint}"),
        plan_fingerprint,
        dry_run: true,
    })
}

fn bounded_git(path: &Path, args: &[&str]) -> Result<String, String> {
    let mut child = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|_| "generated-cache-git-spawn-failed".to_string())?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => {
                let output = child
                    .wait_with_output()
                    .map_err(|_| "generated-cache-git-output-failed".to_string())?;
                if output.stdout.len() > 128 * 1024 {
                    return Err("generated-cache-git-output-bounded".into());
                }
                return String::from_utf8(output.stdout)
                    .map_err(|_| "generated-cache-git-output-not-utf8".into());
            }
            Ok(Some(_)) => return Err("generated-cache-git-command-failed".into()),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(25)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("generated-cache-git-timeout".into());
            }
            Err(_) => return Err("generated-cache-git-wait-failed".into()),
        }
    }
}

/// Collect path-free process evidence and bounded Git ownership evidence before planning.
pub fn audit(path: &Path, home: &Path, observed_at_ms: u64) -> Result<GeneratedCachePlan, String> {
    let active = crate::git_worktree::active_use_evidence(path, 5_000, 128, true);
    let mut evidence = GeneratedCacheActivityEvidence {
        evidence_complete: active.evidence_complete,
        open_pids: active.observed_pids,
        tool_lock_paths: Vec::new(),
        live_cwd_present: active.active,
        git_common_dir: None,
        git_worktree_registered: false,
        git_dirty: false,
    };
    if path.starts_with("/private/tmp") {
        match bounded_git(
            path,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        ) {
            Ok(common) => {
                evidence.git_common_dir = Some(common.trim().into());
                evidence.git_worktree_registered =
                    bounded_git(path, &["worktree", "list", "--porcelain"]).is_ok_and(|output| {
                        output
                            .lines()
                            .any(|line| line.strip_prefix("worktree ") == path.to_str())
                    });
                evidence.git_dirty = bounded_git(path, &["status", "--porcelain=v1"])
                    .map_or(true, |output| !output.is_empty());
            }
            Err(_) => evidence.evidence_complete = false,
        }
    }
    let mut plan = plan_with_evidence(path, home, evidence, observed_at_ms)?;
    // Content hashing can be long-running. Probe again afterward so the returned plan never relies
    // solely on activity evidence collected before the manifest scan.
    let final_active = crate::git_worktree::active_use_evidence(path, 5_000, 128, true);
    if !final_active.assessed || !final_active.evidence_complete {
        if !plan
            .blockers
            .contains(&"active-use-evidence-incomplete".into())
        {
            plan.blockers.push("active-use-evidence-incomplete".into());
        }
        plan.activity.evidence_complete = false;
    }
    if final_active.active || !final_active.observed_pids.is_empty() {
        if !plan.blockers.contains(&"process-active".into()) {
            plan.blockers.push("process-active".into());
        }
        plan.activity.live_cwd_present |= final_active.active;
        plan.activity.open_pids.extend(final_active.observed_pids);
        plan.activity.open_pids.sort_unstable();
        plan.activity.open_pids.dedup();
    }
    if !plan.blockers.is_empty() {
        // Any post-hash activity makes this plan non-executable. Its approval phrase is retained
        // only for stable output; approve() rejects every blocker.
        plan.blockers.sort();
        plan.blockers.dedup();
    }
    Ok(plan)
}

pub fn approve(
    plan: &GeneratedCachePlan,
    phrase: &str,
    approved_by: &str,
    rationale: &str,
    approved_at_ms: u64,
) -> Result<GeneratedCacheApproval, String> {
    if !plan.blockers.is_empty() || phrase != plan.exact_approval_phrase {
        return Err("generated-cache-approval-denied".into());
    }
    if approved_by.trim().is_empty() || rationale.trim().is_empty() {
        return Err("generated-cache-attribution-required".into());
    }
    Ok(GeneratedCacheApproval {
        plan_fingerprint: plan.plan_fingerprint.clone(),
        approved_at_ms,
        approved_by: approved_by.trim().into(),
        rationale: rationale.trim().into(),
    })
}

pub fn execute_with<F>(
    plan: &GeneratedCachePlan,
    approval: &GeneratedCacheApproval,
    fresh: &GeneratedCachePlan,
    attempted_at_ms: u64,
    remove: F,
) -> Result<GeneratedCacheReceipt, String>
where
    F: FnOnce(&Path) -> Result<(), String>,
{
    validate_execution(plan, approval, fresh, attempted_at_ms)?;
    let result = remove(Path::new(&plan.root));
    Ok(GeneratedCacheReceipt {
        schema_version: GENERATED_CACHE_SCHEMA_VERSION,
        plan_fingerprint: plan.plan_fingerprint.clone(),
        root: plan.root.clone(),
        attempted_at_ms,
        allocated_before_bytes: plan.allocated_bytes,
        removed: result.is_ok(),
        error_code: result.err(),
        provider_data_mutated: false,
    })
}

fn validate_execution(
    plan: &GeneratedCachePlan,
    approval: &GeneratedCacheApproval,
    fresh: &GeneratedCachePlan,
    attempted_at_ms: u64,
) -> Result<(), String> {
    if approval.plan_fingerprint != plan.plan_fingerprint
        || fresh.plan_fingerprint != plan.plan_fingerprint
        || !fresh.blockers.is_empty()
        || fresh.observed_at_ms < approval.approved_at_ms
        || fresh.observed_at_ms > attempted_at_ms
        || attempted_at_ms < approval.approved_at_ms
        || attempted_at_ms.saturating_sub(approval.approved_at_ms) > MAX_APPROVAL_AGE_MS
    {
        return Err("generated-cache-fresh-evidence-mismatch".into());
    }
    Ok(())
}

pub fn remove_regenerable_root(path: &Path, home: &Path) -> Result<(), String> {
    if deny_boundary(path)
        || matches!(
            regeneration_contract(path, home),
            None | Some(RegenerationContract::TemporaryGitWorkspace)
        )
    {
        return Err("generated-cache-removal-boundary-denied".into());
    }
    std::fs::remove_dir_all(path).map_err(|_| "generated-cache-remove-failed".into())
}

/// Atomically stage and permanently remove the exact approved cache object.
pub fn stage_and_remove_regenerable_root(
    plan: &GeneratedCachePlan,
    path: &Path,
    home: &Path,
    now_ms: u64,
) -> Result<(), String> {
    if plan.root != path.to_string_lossy()
        || plan.contract
            != regeneration_contract(path, home).ok_or("generated-cache-removal-boundary-denied")?
    {
        return Err("generated-cache-removal-boundary-denied".into());
    }
    let immediate = audit(path, home, now_ms)?;
    if immediate.plan_fingerprint != plan.plan_fingerprint || !immediate.blockers.is_empty() {
        return Err("generated-cache-prestage-evidence-mismatch".into());
    }
    let parent = path.parent().ok_or("generated-cache-parent-unavailable")?;
    let name = path.file_name().ok_or("generated-cache-name-unavailable")?;
    let staging = parent.join(format!(
        ".disksage-generated-cache-staging-{}",
        &plan.plan_fingerprint[..16]
    ));
    let mut staging_builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        staging_builder.mode(0o700);
    }
    staging_builder.create(&staging)
        .map_err(|_| "generated-cache-staging-create-failed")?;
    let staged = staging.join(name);
    if let Err(error) = std::fs::rename(path, &staged) {
        let _ = std::fs::remove_dir(&staging);
        return Err(format!("generated-cache-staging-rename-failed:{error}"));
    }
    let restore = || {
        if !path.exists() {
            std::fs::rename(&staged, path).map_err(|_| "generated-cache-staging-restore-failed")?;
        }
        std::fs::remove_dir(&staging).map_err(|_| "generated-cache-staging-cleanup-failed")
    };
    let staged_result = (|| {
        let active = crate::git_worktree::active_use_evidence(&staged, 5_000, 128, true);
        if !active.assessed || !active.evidence_complete || active.active {
            return Err("generated-cache-staged-active-use".to_string());
        }
        let (_, _, fingerprint, locks) = observe_tree(&staged)?;
        if fingerprint != plan.content_fingerprint || !locks.is_empty() {
            return Err("generated-cache-staged-manifest-mismatch".into());
        }
        let active_after_hash = crate::git_worktree::active_use_evidence(&staged, 5_000, 128, true);
        if !active_after_hash.assessed
            || !active_after_hash.evidence_complete
            || active_after_hash.active
        {
            return Err("generated-cache-staged-active-use".into());
        }
        std::fs::remove_dir_all(&staged)
            .map_err(|_| "generated-cache-remove-failed".to_string())?;
        std::fs::remove_dir(&staging)
            .map_err(|_| "generated-cache-staging-cleanup-failed".to_string())
    })();
    if let Err(error) = staged_result {
        return match restore() {
            Ok(()) => Err(error),
            Err(restore_error) => Err(format!("{error}:{restore_error}")),
        };
    }
    Ok(())
}

fn create_new_private_file(path: &Path) -> Result<std::fs::File, String> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(path)
        .map_err(|_| "generated-cache-receipt-create-failed".to_string())
}

pub fn write_immutable_receipt(path: &Path, receipt: &GeneratedCacheReceipt) -> Result<(), String> {
    use std::io::Write;
    let mut file = create_new_private_file(path)?;
    let bytes = serde_json::to_vec_pretty(receipt)
        .map_err(|_| "generated-cache-receipt-encode-failed".to_string())?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| "generated-cache-receipt-write-failed".into())
}

/// Write a parseable pending event before removal, then append a terminal receipt.
///
/// A file ending in `pending` is deliberately not retried: the caller must reconcile the root,
/// create a new plan, and obtain a new approval before any further attempt.
pub fn execute_and_record<F>(
    plan: &GeneratedCachePlan,
    approval: &GeneratedCacheApproval,
    fresh: &GeneratedCachePlan,
    attempted_at_ms: u64,
    receipt_path: &Path,
    remove: F,
) -> Result<GeneratedCacheReceipt, String>
where
    F: FnOnce(&Path) -> Result<(), String>,
{
    use std::io::Write;
    validate_execution(plan, approval, fresh, attempted_at_ms)?;
    let mut file = create_new_private_file(receipt_path)?;
    let pending = GeneratedCachePendingReceipt {
        schema_version: GENERATED_CACHE_SCHEMA_VERSION,
        plan_fingerprint: plan.plan_fingerprint.clone(),
        root: plan.root.clone(),
        attempted_at_ms,
        allocated_before_bytes: plan.allocated_bytes,
        state: "pending".into(),
        provider_data_mutated: false,
    };
    let pending_bytes = serde_json::to_vec(&pending)
        .map_err(|_| "generated-cache-receipt-encode-failed".to_string())?;
    file.write_all(&pending_bytes)
        .and_then(|_| file.write_all(b"\n"))
        .and_then(|_| file.sync_all())
        .map_err(|_| "generated-cache-receipt-pending-write-failed".to_string())?;
    let receipt_parent = receipt_path
        .parent()
        .ok_or("generated-cache-receipt-parent-unavailable")?;
    std::fs::File::open(receipt_parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| "generated-cache-receipt-parent-sync-failed".to_string())?;
    let receipt = execute_with(plan, approval, fresh, attempted_at_ms, remove)?;
    let bytes = serde_json::to_vec(&receipt)
        .map_err(|_| "generated-cache-receipt-encode-failed".to_string())?;
    file.write_all(&bytes)
        .and_then(|_| file.write_all(b"\n"))
        .and_then(|_| file.sync_all())
        .map_err(|_| "generated-cache-receipt-terminal-write-failed".to_string())?;
    Ok(receipt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    fn inactive() -> GeneratedCacheActivityEvidence {
        GeneratedCacheActivityEvidence {
            evidence_complete: true,
            open_pids: vec![],
            tool_lock_paths: vec![],
            live_cwd_present: false,
            git_common_dir: None,
            git_worktree_registered: false,
            git_dirty: false,
        }
    }

    #[test]
    fn torch_and_homebrew_metadata_are_exact_allowlisted_contracts() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        for relative in [
            ".cache/torch",
            "Library/Caches/Homebrew/api",
            "Library/Caches/Homebrew/bootsnap",
        ] {
            let root = home.join(relative);
            std::fs::create_dir_all(&root).unwrap();
            std::fs::write(root.join("generated"), b"cache").unwrap();
            let plan = plan_with_evidence(&root, home, inactive(), 1).unwrap();
            assert!(plan.blockers.is_empty());
            assert!(plan.dry_run);
        }
    }

    #[test]
    fn active_uv_playwright_and_dirty_worktree_are_retained() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        for relative in [".cache/uv", "Library/Caches/ms-playwright"] {
            let root = home.join(relative);
            std::fs::create_dir_all(&root).unwrap();
            let mut evidence = inactive();
            evidence.open_pids = vec![42];
            assert!(plan_with_evidence(&root, home, evidence, 1)
                .unwrap()
                .blockers
                .contains(&"process-active".into()));
        }
        let root_dir = tempfile::Builder::new()
            .prefix("disksage-generated-cache-test-dirty-")
            .tempdir_in(crate::rules::shared_temp_root())
            .unwrap();
        let root = root_dir.path().to_path_buf();
        let mut evidence = inactive();
        evidence.git_dirty = true;
        evidence.git_worktree_registered = true;
        let plan = plan_with_evidence(&root, home, evidence, 1).unwrap();
        assert!(plan.blockers.contains(&"git-workspace-retained".into()));
        assert!(plan
            .blockers
            .contains(&"temporary-workspace-specialized-executor-required".into()));
    }

    #[test]
    fn cache_child_symlink_is_fingerprinted_without_following_target() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let root = home.join(".cache/uv");
        let outside = temp.path().join("customer-data");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(&outside, b"must not be read or traversed").unwrap();
        symlink(&outside, root.join("cached-link")).unwrap();

        let before = plan_with_evidence(&root, home, inactive(), 1).unwrap();
        std::fs::write(&outside, b"changed outside content").unwrap();
        let after = plan_with_evidence(&root, home, inactive(), 2).unwrap();

        assert_eq!(before.content_fingerprint, after.content_fingerprint);
        assert_eq!(before.allocated_bytes, after.allocated_bytes);
        assert_eq!(before.entry_count, 2);
    }

    #[test]
    fn allowlisted_root_symlink_remains_denied() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let outside = temp.path().join("outside-cache");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::create_dir_all(home.join(".cache")).unwrap();
        let root = home.join(".cache/uv");
        symlink(&outside, &root).unwrap();

        assert_eq!(
            plan_with_evidence(&root, home, inactive(), 1).unwrap_err(),
            "generated-cache-root-symlink-rejected"
        );
    }

    #[test]
    fn provider_and_virtual_machine_boundaries_are_never_admitted() {
        let home = Path::new("/Users/test");
        for path in [
            "/Users/test/Library/CloudStorage/OneDrive/cache",
            "/Users/test/Pictures/Photos Library.photoslibrary/cache",
            "/Users/test/.colima/default/disk",
            "/Users/test/Parallels/Linux.pvm",
        ] {
            assert_eq!(
                plan_with_evidence(Path::new(path), home, inactive(), 1).unwrap_err(),
                "generated-cache-boundary-denied"
            );
        }
    }

    #[test]
    fn exact_approval_executes_via_seam_and_receipt_is_create_only() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let root = home.join(".cache/torch");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("generated"), b"cache").unwrap();
        let plan = plan_with_evidence(&root, home, inactive(), 1).unwrap();
        assert!(approve(&plan, "wrong", "human", "cache regenerates", 2).is_err());
        let approval = approve(
            &plan,
            &plan.exact_approval_phrase,
            "human:local:test",
            "cache regenerates from the official source",
            2,
        )
        .unwrap();
        let receipt_path = temp.path().join("receipt.json");
        let fresh = plan_with_evidence(&root, home, inactive(), 2).unwrap();
        let receipt =
            execute_and_record(&plan, &approval, &fresh, 3, &receipt_path, |_| Ok(())).unwrap();
        assert!(receipt.removed);
        assert!(!receipt.provider_data_mutated);
        let journal = std::fs::read_to_string(&receipt_path).unwrap();
        let events = journal.lines().collect::<Vec<_>>();
        assert_eq!(events.len(), 2);
        let pending: GeneratedCachePendingReceipt = serde_json::from_str(events[0]).unwrap();
        assert_eq!(pending.state, "pending");
        let terminal: GeneratedCacheReceipt = serde_json::from_str(events[1]).unwrap();
        assert!(terminal.removed);
        assert!(
            execute_and_record(&plan, &approval, &fresh, 4, &receipt_path, |_| Ok(())).is_err()
        );
    }

    #[test]
    fn invalid_execution_never_reserves_a_receipt() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let root = home.join(".cache/torch");
        std::fs::create_dir_all(&root).unwrap();
        let plan = plan_with_evidence(&root, home, inactive(), 1).unwrap();
        let approval = GeneratedCacheApproval {
            plan_fingerprint: "not-the-plan".into(),
            approved_at_ms: 2,
            approved_by: "human:test".into(),
            rationale: "test mismatch".into(),
        };
        let receipt_path = temp.path().join("receipt.jsonl");
        assert!(execute_and_record(&plan, &approval, &plan, 3, &receipt_path, |_| Ok(())).is_err());
        assert!(!receipt_path.exists());
    }

    #[test]
    fn fresh_observation_must_follow_approval() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let root = home.join(".cache/torch");
        std::fs::create_dir_all(&root).unwrap();
        let plan = plan_with_evidence(&root, home, inactive(), 1).unwrap();
        let approval = approve(
            &plan,
            &plan.exact_approval_phrase,
            "human:test",
            "verified regeneration source",
            2,
        )
        .unwrap();
        assert!(execute_with(&plan, &approval, &plan, 3, |_| Ok(())).is_err());
        let fresh = plan_with_evidence(&root, home, inactive(), 2).unwrap();
        assert!(execute_with(&plan, &approval, &fresh, 3, |_| Ok(())).is_ok());
    }

    #[test]
    fn manifest_detects_content_rename_and_root_replacement() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let root = home.join(".cache/torch");
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("model.bin");
        std::fs::write(&file, b"aaaa").unwrap();
        let original_times = std::fs::metadata(&file).unwrap();
        let first = plan_with_evidence(&root, home, inactive(), 1).unwrap();
        std::fs::write(&file, b"bbbb").unwrap();
        std::fs::File::options()
            .write(true)
            .open(&file)
            .unwrap()
            .set_times(
                std::fs::FileTimes::new()
                    .set_accessed(original_times.accessed().unwrap())
                    .set_modified(original_times.modified().unwrap()),
            )
            .unwrap();
        let rewritten = plan_with_evidence(&root, home, inactive(), 2).unwrap();
        assert_ne!(first.plan_fingerprint, rewritten.plan_fingerprint);
        std::fs::rename(&file, root.join("renamed.bin")).unwrap();
        let renamed = plan_with_evidence(&root, home, inactive(), 3).unwrap();
        assert_ne!(rewritten.plan_fingerprint, renamed.plan_fingerprint);
        std::fs::remove_dir_all(&root).unwrap();
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("renamed.bin"), b"bbbb").unwrap();
        let replaced = plan_with_evidence(&root, home, inactive(), 4).unwrap();
        assert_ne!(renamed.plan_fingerprint, replaced.plan_fingerprint);
    }

    #[test]
    fn exact_cache_is_staged_rechecked_and_removed() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path();
        let root = home.join(".cache/torch");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("model.bin"), b"regenerable").unwrap();
        let plan = plan_with_evidence(&root, home, inactive(), 1).unwrap();
        stage_and_remove_regenerable_root(&plan, &root, home, 2).unwrap();
        assert!(!root.exists());
        assert!(!home
            .join(".cache")
            .join(format!(
                ".disksage-generated-cache-staging-{}",
                &plan.plan_fingerprint[..16]
            ))
            .exists());
    }

    #[test]
    fn bytes_read_not_stale_metadata_enforce_content_bound() {
        let mut reader = std::io::Cursor::new(b"grew-after-metadata".to_vec());
        let mut hasher = blake3::Hasher::new();
        let mut hashed = 0;
        assert_eq!(
            hash_reader_bounded(&mut reader, &mut hasher, &mut hashed, 4).unwrap_err(),
            "generated-cache-content-bound-exceeded"
        );
        assert!(hashed > 4);
    }
}
