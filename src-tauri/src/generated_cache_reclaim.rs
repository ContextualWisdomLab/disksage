//! Evidence-bound auditing and removal of explicitly regenerable cache roots.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub const GENERATED_CACHE_SCHEMA_VERSION: u32 = 1;
const MAX_ENTRIES: u64 = 200_000;
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

fn observe_tree(path: &Path) -> Result<(u64, u64, String, Vec<String>), String> {
    use std::os::unix::fs::MetadataExt;
    let mut stack = vec![path.to_path_buf()];
    let mut bytes = 0_u64;
    let mut entries = 0_u64;
    let mut locks = Vec::new();
    let mut hasher = blake3::Hasher::new();
    while let Some(current) = stack.pop() {
        if entries >= MAX_ENTRIES {
            return Err("generated-cache-entry-limit".into());
        }
        let metadata = std::fs::symlink_metadata(&current)
            .map_err(|_| "generated-cache-metadata-unavailable".to_string())?;
        if metadata.file_type().is_symlink() {
            return Err("generated-cache-symlink-rejected".into());
        }
        entries += 1;
        bytes = bytes.saturating_add(metadata.blocks().saturating_mul(512));
        hasher.update(&metadata.len().to_le_bytes());
        hasher.update(&metadata.mtime().to_le_bytes());
        if current.file_name().is_some_and(|name| name == ".lock") {
            locks.push(current.to_string_lossy().into_owned());
        }
        if metadata.is_dir() {
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
        && (activity.git_worktree_registered
            || activity.git_dirty
            || activity.git_common_dir.is_some())
    {
        blockers.push("git-workspace-retained".into());
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
    plan_with_evidence(path, home, evidence, observed_at_ms)
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
    if approval.plan_fingerprint != plan.plan_fingerprint
        || fresh.plan_fingerprint != plan.plan_fingerprint
        || !fresh.blockers.is_empty()
        || attempted_at_ms < approval.approved_at_ms
        || attempted_at_ms.saturating_sub(approval.approved_at_ms) > MAX_APPROVAL_AGE_MS
    {
        return Err("generated-cache-fresh-evidence-mismatch".into());
    }
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

pub fn write_immutable_receipt(path: &Path, receipt: &GeneratedCacheReceipt) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|_| "generated-cache-receipt-create-failed".to_string())?;
    let bytes = serde_json::to_vec_pretty(receipt)
        .map_err(|_| "generated-cache-receipt-encode-failed".to_string())?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| "generated-cache-receipt-write-failed".into())
}

/// Reserve a create-only receipt before removal, then durably finalize success or failure.
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
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(receipt_path)
        .map_err(|_| "generated-cache-receipt-create-failed".to_string())?;
    file.sync_all()
        .map_err(|_| "generated-cache-receipt-reserve-failed".to_string())?;
    let receipt = execute_with(plan, approval, fresh, attempted_at_ms, remove)?;
    let bytes = serde_json::to_vec_pretty(&receipt)
        .map_err(|_| "generated-cache-receipt-encode-failed".to_string())?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| "generated-cache-receipt-write-failed".to_string())?;
    Ok(receipt)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let root = PathBuf::from("/private/tmp/disksage-generated-cache-test-dirty");
        std::fs::create_dir_all(&root).unwrap();
        let mut evidence = inactive();
        evidence.git_dirty = true;
        evidence.git_worktree_registered = true;
        let plan = plan_with_evidence(&root, home, evidence, 1).unwrap();
        assert!(plan.blockers.contains(&"git-workspace-retained".into()));
        std::fs::remove_dir_all(root).unwrap();
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
        let receipt =
            execute_and_record(&plan, &approval, &plan, 3, &receipt_path, |_| Ok(())).unwrap();
        assert!(receipt.removed);
        assert!(!receipt.provider_data_mutated);
        assert!(execute_and_record(&plan, &approval, &plan, 4, &receipt_path, |_| Ok(())).is_err());
    }
}
