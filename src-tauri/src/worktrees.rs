//! Read-only stale Git worktree inventory.
//!
//! The scanner never runs `git fetch`, `git worktree remove`, `git worktree prune`, or a
//! filesystem deletion. It evaluates only local Git evidence and allocated disk usage so a
//! later, explicitly confirmed cleanup flow can fail closed.

#[cfg(not(coverage))]
use std::collections::BTreeMap;
use std::collections::BTreeSet;
#[cfg(not(coverage))]
use std::path::Path;
use std::path::PathBuf;
#[cfg(not(coverage))]
use std::process::{Command, Output, Stdio};
#[cfg(not(coverage))]
use std::time::{Duration, Instant};

#[cfg(not(coverage))]
const DAY_MS: u64 = 86_400_000;
#[cfg(not(coverage))]
const GIT_TIMEOUT: Duration = Duration::from_secs(1);
#[cfg(not(coverage))]
const GIT_METADATA_READ_TIMEOUT: Duration = Duration::from_millis(250);
#[cfg(not(coverage))]
const GIT_METADATA_MAX_BYTES: u64 = 4_096;
#[cfg(not(coverage))]
const INVENTORY_TIMEOUT: Duration = Duration::from_secs(30);
#[cfg(not(coverage))]
const FILESYSTEM_SCAN_TIMEOUT: Duration = Duration::from_secs(2);
#[cfg(not(coverage))]
const FILESYSTEM_SCAN_TOTAL_TIMEOUT: Duration = Duration::from_secs(10);
#[cfg(not(coverage))]
const FILESYSTEM_SCAN_MAX_ENTRIES: usize = 200_000;
#[cfg(not(coverage))]
const REPOSITORY_SEARCH_MAX_DEPTH: usize = 8;
#[cfg(not(coverage))]
const REPOSITORY_SEARCH_TIMEOUT: Duration = Duration::from_secs(5);
#[cfg(not(coverage))]
const REPOSITORY_SEARCH_MAX_DIRECTORIES: usize = 100_000;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct RawWorktree {
    path: PathBuf,
    head: String,
    branch: Option<String>,
    detached: bool,
    locked_reason: Option<String>,
    prunable_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorktreeCandidate {
    pub repository_common_dir: String,
    pub path: String,
    pub head: String,
    pub branch: Option<String>,
    pub default_ref: Option<String>,
    pub is_primary: bool,
    pub detached: bool,
    pub exists: bool,
    pub dirty: Option<bool>,
    pub locked_reason: Option<String>,
    pub prunable_reason: Option<String>,
    pub ahead: Option<u64>,
    pub behind: Option<u64>,
    pub merged_into_default: Option<bool>,
    pub last_activity_ms: u64,
    pub age_days: u64,
    pub allocated_bytes: u64,
    pub filesystem_scanned: bool,
    pub filesystem_scan_complete: bool,
    pub removal_eligible: bool,
    pub metadata_prune_eligible: bool,
    pub review_reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorktreeReport {
    pub scanned_root: String,
    pub generated_at_ms: u64,
    pub min_age_days: u64,
    pub search_max_depth: usize,
    pub repository_count: usize,
    pub worktrees: Vec<WorktreeCandidate>,
    pub potentially_reclaimable_bytes: u64,
    pub scan_issues: Vec<WorktreeScanIssue>,
    pub notices: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorktreeScanIssue {
    pub path: String,
    pub operation: String,
    pub reason: String,
}

#[cfg(not(coverage))]
fn git_output(cwd: &Path, args: &[&str]) -> Result<Output, String> {
    let mut command = Command::new("git");
    command
        .arg("-c")
        .arg("core.quotePath=false")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|error| format!("spawn:{error}"))?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return child.wait_with_output().map_err(|error| error.to_string()),
            Ok(None) if started.elapsed() < GIT_TIMEOUT => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok(None) => {
                let _ = child.kill();
                std::thread::spawn(move || {
                    let _ = child.wait_with_output();
                });
                return Err(format!("timeout-after-{}s", GIT_TIMEOUT.as_secs()));
            }
            Err(error) => {
                let _ = child.kill();
                std::thread::spawn(move || {
                    let _ = child.wait_with_output();
                });
                return Err(format!("wait:{error}"));
            }
        }
    }
}

#[cfg(not(coverage))]
fn git_text(cwd: &Path, args: &[&str]) -> Option<String> {
    let output = git_output(cwd, args).ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn parse_worktree_porcelain(value: &str) -> Vec<RawWorktree> {
    fn finish(current: &mut RawWorktree, found: &mut Vec<RawWorktree>) {
        if !current.path.as_os_str().is_empty() {
            found.push(std::mem::take(current));
        }
    }

    let mut found = Vec::new();
    let mut current = RawWorktree::default();
    for line in value.lines() {
        if line.is_empty() {
            finish(&mut current, &mut found);
        } else if let Some(path) = line.strip_prefix("worktree ") {
            finish(&mut current, &mut found);
            current.path = PathBuf::from(path);
        } else if let Some(head) = line.strip_prefix("HEAD ") {
            current.head = head.into();
        } else if let Some(branch) = line.strip_prefix("branch ") {
            current.branch = Some(
                branch
                    .strip_prefix("refs/heads/")
                    .unwrap_or(branch)
                    .to_string(),
            );
        } else if line == "detached" {
            current.detached = true;
        } else if let Some(reason) = line.strip_prefix("locked") {
            current.locked_reason = Some(reason.trim().to_string());
        } else if let Some(reason) = line.strip_prefix("prunable") {
            current.prunable_reason = Some(reason.trim().to_string());
        }
    }
    finish(&mut current, &mut found);
    found
}

fn prune_repository_search(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | "node_modules"
            | "target"
            | ".venv"
            | "venv"
            | "__pycache__"
            | "Library"
            | "System"
            | "Applications"
            | ".Trash"
            | ".cache"
            | "Caches"
    )
}

#[cfg(not(coverage))]
fn read_small_text_with_timeout(path: &Path, timeout: Duration) -> Result<String, String> {
    let path = path.to_path_buf();
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let _ = sender.send(std::fs::read_to_string(path));
    });
    match receiver.recv_timeout(timeout) {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => Err(error.to_string()),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Err(format!(
            "metadata-read-timeout-after-{}ms",
            timeout.as_millis()
        )),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            Err("metadata-reader-disconnected".into())
        }
    }
}

#[cfg(not(coverage))]
fn common_dir_from_marker(repository: &Path) -> Result<PathBuf, String> {
    let marker = repository.join(".git");
    let metadata = std::fs::symlink_metadata(&marker).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() {
        return Err("git-marker-is-symlink".into());
    }
    if metadata.is_dir() {
        return std::fs::canonicalize(&marker).map_err(|error| error.to_string());
    }
    if !metadata.is_file() {
        return Err("git-marker-is-not-file-or-directory".into());
    }
    if metadata.len() > GIT_METADATA_MAX_BYTES {
        return Err(format!("git-marker-too-large-{}", metadata.len()));
    }
    let marker_value = read_small_text_with_timeout(&marker, GIT_METADATA_READ_TIMEOUT)?;
    let gitdir_value = marker_value
        .trim()
        .strip_prefix("gitdir:")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "malformed-gitfile".to_string())?;
    let gitdir = PathBuf::from(gitdir_value);
    let gitdir = if gitdir.is_absolute() {
        gitdir
    } else {
        repository.join(gitdir)
    };
    let gitdir = std::fs::canonicalize(&gitdir).map_err(|error| error.to_string())?;
    let commondir_file = gitdir.join("commondir");
    if !commondir_file.is_file() {
        return Ok(gitdir);
    }
    let worktrees_dir = gitdir
        .parent()
        .filter(|parent| parent.file_name().is_some_and(|name| name == "worktrees"))
        .ok_or_else(|| "unsupported-nonstandard-commondir-layout".to_string())?;
    let common = worktrees_dir
        .parent()
        .ok_or_else(|| "worktree-common-dir-parent-missing".to_string())?;
    std::fs::canonicalize(common).map_err(|error| error.to_string())
}

#[cfg(not(coverage))]
fn repository_seeds_with_limits(
    root: &Path,
    max_depth: usize,
    max_directories: usize,
    timeout: Duration,
) -> (BTreeMap<PathBuf, PathBuf>, Vec<WorktreeScanIssue>) {
    if !root.is_dir() {
        return (BTreeMap::new(), Vec::new());
    }
    let mut seeds = BTreeMap::new();
    let mut issues = Vec::new();
    let mut directories = vec![(root.to_path_buf(), 0usize)];
    let mut inspected = 0usize;
    let started = Instant::now();
    while let Some((path, depth)) = directories.pop() {
        inspected = inspected.saturating_add(1);
        if inspected > max_directories || started.elapsed() >= timeout {
            issues.push(WorktreeScanIssue {
                path: root.to_string_lossy().into_owned(),
                operation: "discover-repositories".into(),
                reason: if inspected > max_directories {
                    format!("directory-budget-exhausted-{max_directories}")
                } else {
                    format!("timeout-after-{}s", timeout.as_secs())
                },
            });
            break;
        }
        if path.join(".git").exists() {
            match common_dir_from_marker(&path) {
                Ok(common) => {
                    seeds.entry(common).or_insert(path);
                }
                Err(reason) => issues.push(WorktreeScanIssue {
                    path: path.to_string_lossy().into_owned(),
                    operation: "resolve-git-common-dir".into(),
                    reason,
                }),
            }
            continue;
        }
        if depth >= max_depth {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let Ok(metadata) = std::fs::symlink_metadata(entry.path()) else {
                continue;
            };
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                continue;
            }
            let should_prune = entry
                .file_name()
                .to_str()
                .map(prune_repository_search)
                .unwrap_or(true);
            if !should_prune {
                directories.push((entry.path(), depth.saturating_add(1)));
            }
        }
    }
    (seeds, issues)
}

#[cfg(not(coverage))]
fn repository_seeds(root: &Path) -> (BTreeMap<PathBuf, PathBuf>, Vec<WorktreeScanIssue>) {
    repository_seeds_with_limits(
        root,
        REPOSITORY_SEARCH_MAX_DEPTH,
        REPOSITORY_SEARCH_MAX_DIRECTORIES,
        REPOSITORY_SEARCH_TIMEOUT,
    )
}

#[cfg(not(coverage))]
fn local_default_ref(repository: &Path) -> Option<String> {
    let symbolic = git_output(
        repository,
        &["symbolic-ref", "--quiet", "refs/remotes/origin/HEAD"],
    );
    match symbolic {
        Ok(output) if output.status.success() => {
            let reference = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !reference.is_empty() {
                return Some(reference);
            }
        }
        Err(_) => return None,
        Ok(_) => {}
    }
    let refs = git_text(repository, &["show-ref"])?;
    let available: BTreeSet<&str> = refs
        .lines()
        .filter_map(|line| line.split_whitespace().nth(1))
        .collect();
    for reference in [
        "refs/remotes/origin/main",
        "refs/heads/main",
        "refs/remotes/origin/master",
        "refs/heads/master",
    ] {
        if available.contains(reference) {
            return Some(reference.to_string());
        }
    }
    None
}

#[cfg(not(coverage))]
fn dirty_state(path: &Path) -> Option<bool> {
    let output = git_output(
        path,
        &["status", "--porcelain=v1", "--untracked-files=normal"],
    )
    .ok()?;
    output.status.success().then(|| !output.stdout.is_empty())
}

#[cfg(not(coverage))]
fn ahead_behind(repository: &Path, default_ref: &str, head: &str) -> Option<(u64, u64)> {
    let range = format!("{default_ref}...{head}");
    let value = git_text(repository, &["rev-list", "--left-right", "--count", &range])?;
    let mut fields = value.split_whitespace();
    let behind = fields.next()?.parse().ok()?;
    let ahead = fields.next()?.parse().ok()?;
    Some((ahead, behind))
}

#[cfg(not(coverage))]
fn merged_into(repository: &Path, head: &str, default_ref: &str) -> Option<bool> {
    let output = git_output(
        repository,
        &["merge-base", "--is-ancestor", head, default_ref],
    )
    .ok()?;
    match output.status.code() {
        Some(0) => Some(true),
        Some(1) => Some(false),
        _ => None,
    }
}

#[cfg(not(coverage))]
fn commit_time_ms(repository: &Path, head: &str) -> u64 {
    git_text(repository, &["show", "-s", "--format=%ct", head])
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0)
        .saturating_mul(1_000)
}

#[cfg(all(unix, not(coverage)))]
fn allocated_bytes(metadata: &std::fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    metadata.blocks().saturating_mul(512)
}

#[cfg(all(not(unix), not(coverage)))]
fn allocated_bytes(metadata: &std::fs::Metadata) -> u64 {
    metadata.len()
}

#[cfg(not(coverage))]
fn filesystem_evidence_with_limits(
    root: &Path,
    max_entries: usize,
    timeout: Duration,
) -> (u64, u64, bool) {
    if !root.is_dir() {
        return (0, 0, false);
    }
    let mut bytes = 0u64;
    let mut latest_ms = 0u64;
    let mut inspected = 0usize;
    let started = Instant::now();
    let mut directories = vec![root.to_path_buf()];
    while let Some(directory) = directories.pop() {
        if started.elapsed() >= timeout {
            return (bytes, latest_ms, false);
        }
        let Ok(entries) = std::fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            inspected = inspected.saturating_add(1);
            if inspected > max_entries || started.elapsed() >= timeout {
                return (bytes, latest_ms, false);
            }
            let Ok(metadata) = std::fs::symlink_metadata(entry.path()) else {
                continue;
            };
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                if entry.file_name() != ".git" {
                    directories.push(entry.path());
                }
                continue;
            }
            if metadata.is_file() {
                bytes = bytes.saturating_add(allocated_bytes(&metadata));
                let modified_ms = metadata
                    .modified()
                    .ok()
                    .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|duration| duration.as_millis() as u64)
                    .unwrap_or(0);
                latest_ms = latest_ms.max(modified_ms);
            }
        }
    }
    (bytes, latest_ms, true)
}

#[allow(clippy::too_many_arguments)]
fn classify(
    is_primary: bool,
    exists: bool,
    age_days: u64,
    min_age_days: u64,
    dirty: Option<bool>,
    locked: bool,
    detached: bool,
    branch_known: bool,
    ahead: Option<u64>,
    merged: Option<bool>,
    default_known: bool,
    prunable: bool,
) -> (bool, bool, Vec<String>) {
    let mut reasons = BTreeSet::new();
    if is_primary {
        reasons.insert("primary-worktree-protected".to_string());
    }
    if !exists {
        reasons.insert("worktree-path-missing".to_string());
    }
    if !is_primary && age_days < min_age_days {
        reasons.insert("recent-activity".to_string());
    }
    match dirty {
        Some(true) => {
            reasons.insert("uncommitted-changes".to_string());
        }
        None if exists && !is_primary => {
            reasons.insert("git-status-unavailable".to_string());
        }
        _ => {}
    }
    if locked {
        reasons.insert("worktree-locked".to_string());
    }
    if detached {
        reasons.insert("detached-head".to_string());
    }
    if !is_primary && !branch_known {
        reasons.insert("branch-unresolved".to_string());
    }
    if !is_primary && !default_known {
        reasons.insert("default-ref-unresolved".to_string());
    }
    if ahead.unwrap_or(0) > 0 {
        reasons.insert("unique-commits-ahead-of-default".to_string());
    }
    match merged {
        Some(false) => {
            reasons.insert("head-not-merged-into-default".to_string());
        }
        None if default_known && !is_primary => {
            reasons.insert("merge-status-unavailable".to_string());
        }
        _ => {}
    }

    let removal_eligible = !is_primary
        && exists
        && age_days >= min_age_days
        && dirty == Some(false)
        && !locked
        && !detached
        && branch_known
        && default_known
        && ahead == Some(0)
        && merged == Some(true);
    let metadata_prune_eligible = !exists && prunable && !locked;
    (
        removal_eligible,
        metadata_prune_eligible,
        reasons.into_iter().collect(),
    )
}

/// Inventory all locally registered worktrees reachable from repositories under `root`.
///
/// Local default refs may be stale because this function intentionally does not fetch.
/// Coverage builds validate the deterministic parser and fail-closed classifier. Git processes,
/// timed metadata reads, and bounded filesystem walks remain in the normal integration boundary.
#[cfg(not(coverage))]
pub fn inventory(root: &Path, min_age_days: u64, now_ms: u64) -> WorktreeReport {
    let inventory_started = Instant::now();
    let (repositories, mut scan_issues) = repository_seeds(root);
    let mut worktrees = Vec::new();
    let filesystem_scan_started = Instant::now();
    let registered_worktree_repository_count = repositories
        .keys()
        .filter(|common_dir| common_dir.join("worktrees").is_dir())
        .count();
    'repositories: for (common_dir, seed) in &repositories {
        if !common_dir.join("worktrees").is_dir() {
            continue;
        }
        if inventory_started.elapsed() >= INVENTORY_TIMEOUT {
            scan_issues.push(WorktreeScanIssue {
                path: root.to_string_lossy().into_owned(),
                operation: "inventory-repositories".into(),
                reason: format!("timeout-after-{}s", INVENTORY_TIMEOUT.as_secs()),
            });
            break 'repositories;
        }
        let default_ref = local_default_ref(seed);
        let raw = match git_output(seed, &["worktree", "list", "--porcelain"]) {
            Ok(output) if output.status.success() => {
                parse_worktree_porcelain(&String::from_utf8_lossy(&output.stdout))
            }
            Ok(output) => {
                scan_issues.push(WorktreeScanIssue {
                    path: common_dir.to_string_lossy().into_owned(),
                    operation: "list-worktrees".into(),
                    reason: format!("git-exit-{}", output.status.code().unwrap_or(-1)),
                });
                continue;
            }
            Err(reason) => {
                scan_issues.push(WorktreeScanIssue {
                    path: common_dir.to_string_lossy().into_owned(),
                    operation: "list-worktrees".into(),
                    reason,
                });
                continue;
            }
        };
        for (index, worktree) in raw.into_iter().enumerate() {
            if inventory_started.elapsed() >= INVENTORY_TIMEOUT {
                scan_issues.push(WorktreeScanIssue {
                    path: root.to_string_lossy().into_owned(),
                    operation: "inventory-repositories".into(),
                    reason: format!("timeout-after-{}s", INVENTORY_TIMEOUT.as_secs()),
                });
                break 'repositories;
            }
            let is_primary = index == 0;
            let exists = worktree.path.is_dir();
            let dirty = (exists && !is_primary)
                .then(|| dirty_state(&worktree.path))
                .flatten();
            let filesystem_scanned = exists && !is_primary;
            let filesystem_scan_remaining =
                FILESYSTEM_SCAN_TOTAL_TIMEOUT.saturating_sub(filesystem_scan_started.elapsed());
            let (bytes, latest_file_ms, filesystem_scan_complete) =
                if filesystem_scanned && !filesystem_scan_remaining.is_zero() {
                    filesystem_evidence_with_limits(
                        &worktree.path,
                        FILESYSTEM_SCAN_MAX_ENTRIES,
                        FILESYSTEM_SCAN_TIMEOUT.min(filesystem_scan_remaining),
                    )
                } else {
                    (0, 0, false)
                };
            let commit_ms = if is_primary {
                0
            } else {
                commit_time_ms(seed, &worktree.head)
            };
            let last_activity_ms = latest_file_ms.max(commit_ms);
            let age_days = now_ms.saturating_sub(last_activity_ms) / DAY_MS;
            let (ahead, behind) = if is_primary {
                (None, None)
            } else {
                default_ref
                    .as_deref()
                    .and_then(|reference| ahead_behind(seed, reference, &worktree.head))
                    .map(|(ahead, behind)| (Some(ahead), Some(behind)))
                    .unwrap_or((None, None))
            };
            let merged = (!is_primary)
                .then(|| {
                    default_ref
                        .as_deref()
                        .and_then(|reference| merged_into(seed, &worktree.head, reference))
                })
                .flatten();
            let (mut removal_eligible, metadata_prune_eligible, mut review_reasons) = classify(
                is_primary,
                exists,
                age_days,
                min_age_days,
                dirty,
                worktree.locked_reason.is_some(),
                worktree.detached,
                worktree.branch.is_some(),
                ahead,
                merged,
                default_ref.is_some(),
                worktree.prunable_reason.is_some(),
            );
            if filesystem_scanned && !filesystem_scan_complete {
                removal_eligible = false;
                review_reasons.push("filesystem-evidence-incomplete".into());
                review_reasons.sort();
            }
            worktrees.push(WorktreeCandidate {
                repository_common_dir: common_dir.to_string_lossy().into_owned(),
                path: worktree.path.to_string_lossy().into_owned(),
                head: worktree.head,
                branch: worktree.branch,
                default_ref: default_ref.clone(),
                is_primary,
                detached: worktree.detached,
                exists,
                dirty,
                locked_reason: worktree.locked_reason,
                prunable_reason: worktree.prunable_reason,
                ahead,
                behind,
                merged_into_default: merged,
                last_activity_ms,
                age_days,
                allocated_bytes: bytes,
                filesystem_scanned,
                filesystem_scan_complete,
                removal_eligible,
                metadata_prune_eligible,
                review_reasons,
            });
        }
    }
    worktrees.sort_by(|left, right| {
        right
            .removal_eligible
            .cmp(&left.removal_eligible)
            .then_with(|| right.allocated_bytes.cmp(&left.allocated_bytes))
            .then_with(|| left.path.cmp(&right.path))
    });
    let potentially_reclaimable_bytes = worktrees
        .iter()
        .filter(|worktree| worktree.removal_eligible)
        .map(|worktree| worktree.allocated_bytes)
        .sum();
    WorktreeReport {
        scanned_root: root.to_string_lossy().into_owned(),
        generated_at_ms: now_ms,
        min_age_days,
        search_max_depth: REPOSITORY_SEARCH_MAX_DEPTH,
        repository_count: registered_worktree_repository_count,
        worktrees,
        potentially_reclaimable_bytes,
        scan_issues,
        notices: vec![
            "read-only-no-worktree-removal-or-prune".into(),
            "local-default-ref-only-no-fetch".into(),
            "allocated-bytes-are-an-estimate".into(),
            "large-filesystem-evidence-is-time-and-entry-bounded".into(),
            "filesystem-evidence-has-a-global-scan-time-budget".into(),
            "repository-search-depth-is-bounded-select-a-nearer-root-for-deeper-repositories"
                .into(),
            "repository-discovery-has-time-and-directory-budgets".into(),
            "repository-git-evidence-has-a-global-time-budget".into(),
            "repositories-without-linked-worktree-metadata-are-skipped".into(),
            "dirty-locked-detached-unmerged-or-ahead-worktrees-fail-closed".into(),
        ],
    }
}

#[cfg(not(coverage))]
pub fn system_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_porcelain_records_and_flags() {
        let value = "worktree /repo\nHEAD abc\nbranch refs/heads/main\n\nworktree /repo/wt\nHEAD def\ndetached\nlocked in use\n\nworktree /missing\nHEAD 123\nbranch refs/heads/old\nprunable gitdir file points to non-existent location\n";
        let parsed = parse_worktree_porcelain(value);
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0].branch.as_deref(), Some("main"));
        assert!(parsed[1].detached);
        assert_eq!(parsed[1].locked_reason.as_deref(), Some("in use"));
        assert!(parsed[2].prunable_reason.is_some());
    }

    #[test]
    fn clean_old_merged_linked_worktree_is_the_only_removal_eligible_shape() {
        let eligible = classify(
            false,
            true,
            90,
            30,
            Some(false),
            false,
            false,
            true,
            Some(0),
            Some(true),
            true,
            false,
        );
        assert!(eligible.0);
        assert!(!eligible.1);
        for variation in [
            classify(
                true,
                true,
                90,
                30,
                Some(false),
                false,
                false,
                true,
                Some(0),
                Some(true),
                true,
                false,
            ),
            classify(
                false,
                true,
                90,
                30,
                Some(true),
                false,
                false,
                true,
                Some(0),
                Some(true),
                true,
                false,
            ),
            classify(
                false,
                true,
                90,
                30,
                Some(false),
                true,
                false,
                true,
                Some(0),
                Some(true),
                true,
                false,
            ),
            classify(
                false,
                true,
                90,
                30,
                Some(false),
                false,
                true,
                true,
                Some(0),
                Some(true),
                true,
                false,
            ),
            classify(
                false,
                true,
                90,
                30,
                Some(false),
                false,
                false,
                false,
                Some(0),
                Some(true),
                true,
                false,
            ),
            classify(
                false,
                true,
                90,
                30,
                Some(false),
                false,
                false,
                true,
                Some(1),
                Some(false),
                true,
                false,
            ),
            classify(
                false,
                true,
                1,
                30,
                Some(false),
                false,
                false,
                true,
                Some(0),
                Some(true),
                true,
                false,
            ),
        ] {
            assert!(!variation.0);
        }
    }

    #[test]
    fn missing_prunable_metadata_is_separate_from_disk_reclaim() {
        let result = classify(
            false,
            false,
            90,
            30,
            None,
            false,
            false,
            true,
            Some(0),
            Some(true),
            true,
            true,
        );
        assert!(!result.0);
        assert!(result.1);
        assert!(result.2.contains(&"worktree-path-missing".to_string()));
    }

    #[test]
    fn uncertain_git_evidence_and_default_ref_fail_closed_with_reasons() {
        let unavailable = classify(
            false,
            true,
            90,
            30,
            None,
            false,
            false,
            true,
            Some(0),
            None,
            true,
            false,
        );
        assert!(!unavailable.0);
        assert!(unavailable
            .2
            .contains(&"git-status-unavailable".to_string()));
        assert!(unavailable
            .2
            .contains(&"merge-status-unavailable".to_string()));

        let no_default = classify(
            false,
            true,
            90,
            30,
            Some(false),
            false,
            false,
            true,
            Some(0),
            None,
            false,
            false,
        );
        assert!(!no_default.0);
        assert!(no_default.2.contains(&"default-ref-unresolved".to_string()));
        assert!(!no_default
            .2
            .contains(&"merge-status-unavailable".to_string()));
    }

    #[test]
    fn repository_pruning_policy_covers_generated_and_user_directories() {
        for name in [
            ".git",
            "node_modules",
            "target",
            ".venv",
            "venv",
            "__pycache__",
            "Library",
            "System",
            "Applications",
            ".Trash",
            ".cache",
            "Caches",
        ] {
            assert!(prune_repository_search(name), "{name}");
        }
        assert!(!prune_repository_search("src"));
    }

    #[cfg(not(coverage))]
    #[test]
    fn allocated_size_and_activity_ignore_git_metadata_and_symlinks() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join(".git")).unwrap();
        std::fs::write(tmp.path().join(".git/large"), vec![0u8; 1024 * 1024]).unwrap();
        std::fs::write(tmp.path().join("payload"), vec![1u8; 4096]).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(tmp.path().join("payload"), tmp.path().join("link")).unwrap();
        let (bytes, latest, complete) = filesystem_evidence_with_limits(
            tmp.path(),
            FILESYSTEM_SCAN_MAX_ENTRIES,
            FILESYSTEM_SCAN_TIMEOUT,
        );
        assert!(bytes > 0);
        assert!(bytes < 1024 * 1024);
        assert!(latest > 0);
        assert!(complete);
    }

    #[cfg(not(coverage))]
    #[test]
    fn filesystem_evidence_reports_incomplete_when_entry_budget_is_exhausted() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("one"), b"one").unwrap();
        std::fs::write(tmp.path().join("two"), b"two").unwrap();
        let (_, _, complete) =
            filesystem_evidence_with_limits(tmp.path(), 1, Duration::from_secs(30));
        assert!(!complete);
    }

    #[cfg(not(coverage))]
    #[test]
    fn non_repository_tree_returns_empty_report() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("ordinary")).unwrap();
        let report = inventory(tmp.path(), 30, system_now_ms());
        assert_eq!(report.repository_count, 0);
        assert!(report.worktrees.is_empty());
        assert!(report.scan_issues.is_empty());
        assert_eq!(report.potentially_reclaimable_bytes, 0);
    }

    #[cfg(not(coverage))]
    #[test]
    fn repository_discovery_reports_an_exhausted_directory_budget() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("first")).unwrap();
        let (_, issues) = repository_seeds_with_limits(
            tmp.path(),
            REPOSITORY_SEARCH_MAX_DEPTH,
            1,
            Duration::from_secs(30),
        );
        assert!(issues.iter().any(|issue| {
            issue.operation == "discover-repositories"
                && issue.reason == "directory-budget-exhausted-1"
        }));
    }

    #[cfg(not(coverage))]
    #[test]
    fn real_git_worktree_inventory_deduplicates_repository_and_protects_primary() {
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let repository = tmp.path().join("repository");
        let linked = tmp.path().join("linked");
        std::fs::create_dir(&repository).unwrap();
        let git = |cwd: &Path, args: &[&str]| {
            let output = Command::new("git")
                .arg("-C")
                .arg(cwd)
                .args(args)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "git {:?}: {}",
                args,
                String::from_utf8_lossy(&output.stderr)
            );
        };
        git(&repository, &["init"]);
        git(
            &repository,
            &["config", "user.email", "test@example.invalid"],
        );
        git(&repository, &["config", "user.name", "DiskSage Test"]);
        std::fs::write(repository.join("tracked.txt"), b"content").unwrap();
        git(&repository, &["add", "tracked.txt"]);
        git(&repository, &["commit", "-m", "initial"]);
        git(&repository, &["branch", "-M", "main"]);
        git(&repository, &["branch", "old"]);
        git(
            &repository,
            &["worktree", "add", linked.to_str().unwrap(), "old"],
        );
        let linked_identity = std::fs::canonicalize(&linked).unwrap();
        assert_eq!(
            common_dir_from_marker(&repository).unwrap(),
            common_dir_from_marker(&linked).unwrap()
        );

        let report = inventory(tmp.path(), 0, system_now_ms());
        assert_eq!(report.repository_count, 1);
        assert_eq!(report.worktrees.len(), 2);
        assert!(report.worktrees.iter().any(|worktree| {
            worktree.is_primary
                && !worktree.filesystem_scanned
                && !worktree.removal_eligible
                && worktree
                    .review_reasons
                    .contains(&"primary-worktree-protected".to_string())
        }));
        assert!(
            report.worktrees.iter().any(|worktree| {
                Path::new(&worktree.path) == linked_identity
                    && worktree.filesystem_scanned
                    && worktree.filesystem_scan_complete
                    && worktree.branch.as_deref() == Some("old")
                    && worktree.dirty == Some(false)
                    && worktree.ahead == Some(0)
                    && worktree.merged_into_default == Some(true)
                    && worktree.removal_eligible
            }),
            "{report:#?}"
        );
    }
}
