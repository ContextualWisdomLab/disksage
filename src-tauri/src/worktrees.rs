//! Read-only stale Git worktree inventory.
//!
//! The scanner never runs `git fetch`, `git worktree remove`, `git worktree prune`, or a
//! filesystem deletion. It evaluates only local Git evidence and allocated disk usage so a
//! later, explicitly confirmed cleanup flow can fail closed.

#[cfg(not(coverage))]
use std::collections::BTreeMap;
use std::collections::BTreeSet;
#[cfg(not(coverage))]
use std::io::{Read, Write};
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
const GIT_TIMEOUT: Duration = Duration::from_secs(5);
#[cfg(not(coverage))]
const GIT_WORKTREE_LIST_TIMEOUT: Duration = Duration::from_secs(2);
#[cfg(not(coverage))]
const GIT_METADATA_READ_TIMEOUT: Duration = Duration::from_secs(2);
#[cfg(not(coverage))]
const GIT_METADATA_MAX_BYTES: u64 = 4_096;
#[cfg(not(coverage))]
const GIT_COMMAND_MAX_BYTES: usize = 4 * 1024 * 1024;
#[cfg(not(coverage))]
const GIT_CHECK_IGNORE_MAX_INPUT_BYTES: usize = 1024 * 1024;
#[cfg(not(coverage))]
const GIT_CHECK_IGNORE_MAX_OUTPUT_BYTES: usize = 1024 * 1024;
#[cfg(not(coverage))]
const INVENTORY_TIMEOUT: Duration = Duration::from_secs(180);
#[cfg(not(coverage))]
const FILESYSTEM_SCAN_TIMEOUT: Duration = Duration::from_secs(15);
#[cfg(not(coverage))]
const FILESYSTEM_SCAN_TOTAL_TIMEOUT: Duration = Duration::from_secs(120);
#[cfg(not(coverage))]
const FILESYSTEM_SCAN_MAX_ENTRIES: usize = 1_000_000;
#[cfg(not(coverage))]
const REPOSITORY_SEARCH_MAX_DEPTH: usize = 8;
#[cfg(not(coverage))]
const REPOSITORY_SEARCH_TIMEOUT: Duration = Duration::from_secs(60);
#[cfg(not(coverage))]
const REPOSITORY_SEARCH_MAX_DIRECTORIES: usize = 500_000;

#[cfg(not(coverage))]
#[derive(Debug, Clone, Copy)]
struct InventoryBudget {
    started: Instant,
    timeout: Duration,
}

#[cfg(not(coverage))]
impl InventoryBudget {
    fn new(timeout: Duration) -> Self {
        Self {
            started: Instant::now(),
            timeout,
        }
    }

    fn remaining(self) -> Duration {
        self.timeout.saturating_sub(self.started.elapsed())
    }

    fn capped(self, maximum: Duration) -> Option<Duration> {
        let remaining = self.remaining();
        (!remaining.is_zero()).then_some(maximum.min(remaining))
    }

    fn exhausted(self) -> bool {
        self.remaining().is_zero()
    }
}

const GENERATED_ARTIFACT_DIRECTORY_NAMES: &[&str] = &[
    "node_modules",
    "target",
    ".venv",
    "venv",
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
    ".next",
    ".turbo",
];

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
    pub generated_artifact_bytes: u64,
    pub generated_artifacts: Vec<GeneratedArtifact>,
    pub filesystem_scanned: bool,
    pub filesystem_scan_complete: bool,
    pub removal_eligible: bool,
    pub metadata_prune_eligible: bool,
    pub review_reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GeneratedArtifact {
    pub path: String,
    pub kind: String,
    pub allocated_bytes: u64,
    pub gitignore_confirmed: Option<bool>,
    pub gitignore_source: Option<String>,
    pub gitignore_line: Option<u64>,
    pub gitignore_pattern: Option<String>,
    pub gitignore_issue: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct OrphanWorktreeCandidate {
    pub path: String,
    pub missing_git_dir: String,
    pub allocated_bytes: u64,
    pub generated_artifact_bytes: u64,
    pub generated_artifacts: Vec<GeneratedArtifact>,
    pub filesystem_scan_complete: bool,
    pub removal_eligible: bool,
    pub review_reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorktreeReport {
    pub scanned_root: String,
    pub generated_at_ms: u64,
    pub elapsed_ms: u64,
    pub evidence_complete: bool,
    pub min_age_days: u64,
    pub search_max_depth: usize,
    pub repository_count: usize,
    pub worktrees: Vec<WorktreeCandidate>,
    pub orphaned_worktrees: Vec<OrphanWorktreeCandidate>,
    pub potentially_reclaimable_bytes: u64,
    pub reviewable_generated_artifact_bytes: u64,
    #[serde(default)]
    pub ignore_confirmed_generated_artifact_bytes: u64,
    pub scan_issues: Vec<WorktreeScanIssue>,
    pub notices: Vec<String>,
}

#[cfg(not(coverage))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct OrphanWorktreeMarker {
    path: PathBuf,
    missing_git_dir: PathBuf,
}

#[cfg(not(coverage))]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct FilesystemEvidence {
    allocated_bytes: u64,
    latest_ms: u64,
    complete: bool,
    generated_artifacts: Vec<GeneratedArtifact>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorktreeScanIssue {
    pub path: String,
    pub operation: String,
    pub reason: String,
}

#[cfg(not(coverage))]
fn git_output_with_timeout(cwd: &Path, args: &[&str], timeout: Duration) -> Result<Output, String> {
    git_output_with_input_and_timeout(cwd, args, None, timeout, GIT_COMMAND_MAX_BYTES)
}

#[cfg(not(coverage))]
fn read_bounded<R: Read>(mut reader: R, maximum: usize) -> Result<(Vec<u8>, bool), String> {
    let mut retained = Vec::new();
    let mut truncated = false;
    let mut buffer = [0_u8; 8192];
    loop {
        let count = reader
            .read(&mut buffer)
            .map_err(|error| format!("read:{error}"))?;
        if count == 0 {
            return Ok((retained, truncated));
        }
        let remaining = maximum.saturating_sub(retained.len());
        retained.extend_from_slice(&buffer[..count.min(remaining)]);
        truncated |= count > remaining;
    }
}

#[cfg(not(coverage))]
fn git_output_with_input_and_timeout(
    cwd: &Path,
    args: &[&str],
    input: Option<Vec<u8>>,
    timeout: Duration,
    maximum_output_bytes: usize,
) -> Result<Output, String> {
    let mut command = Command::new("git");
    command
        .arg("-c")
        .arg("core.quotePath=false")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|error| format!("spawn:{error}"))?;
    let stdin_writer = input.map(|input| {
        let mut stdin = child.stdin.take().expect("piped stdin must be present");
        std::thread::spawn(move || {
            stdin
                .write_all(&input)
                .map_err(|error| format!("stdin-write:{error}"))
        })
    });
    let stdout = child.stdout.take().expect("piped stdout must be present");
    let stderr = child.stderr.take().expect("piped stderr must be present");
    let stdout_reader = std::thread::spawn(move || read_bounded(stdout, maximum_output_bytes));
    let stderr_reader = std::thread::spawn(move || read_bounded(stderr, maximum_output_bytes));
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < timeout => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("timeout-after-{}ms", timeout.as_millis()));
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("wait:{error}"));
            }
        }
    };
    if let Some(writer) = stdin_writer {
        writer
            .join()
            .map_err(|_| "stdin-writer-panicked".to_string())??;
    }
    let (stdout, stdout_truncated) = stdout_reader
        .join()
        .map_err(|_| "stdout-reader-panicked".to_string())??;
    let (stderr, stderr_truncated) = stderr_reader
        .join()
        .map_err(|_| "stderr-reader-panicked".to_string())??;
    if stdout_truncated || stderr_truncated {
        return Err(format!(
            "git-output-limit-exceeded-{maximum_output_bytes}-bytes"
        ));
    }
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

#[cfg(not(coverage))]
fn git_output_with_budget(
    cwd: &Path,
    args: &[&str],
    budget: InventoryBudget,
    maximum: Duration,
) -> Result<Output, String> {
    let timeout = budget
        .capped(maximum)
        .ok_or_else(|| "inventory-deadline-exhausted".to_string())?;
    git_output_with_timeout(cwd, args, timeout)
}

#[cfg(not(coverage))]
fn git_text_with_budget(cwd: &Path, args: &[&str], budget: InventoryBudget) -> Option<String> {
    let output = git_output_with_budget(cwd, args, budget, GIT_TIMEOUT).ok()?;
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
fn gitfile_target(repository: &Path) -> Result<PathBuf, String> {
    let marker = repository.join(".git");
    let metadata = std::fs::symlink_metadata(&marker).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("git-marker-is-not-regular-file".into());
    }
    if metadata.len() > GIT_METADATA_MAX_BYTES {
        return Err(format!("git-marker-too-large-{}", metadata.len()));
    }
    let marker_value = read_small_text_with_timeout(&marker, GIT_METADATA_READ_TIMEOUT)?;
    let value = marker_value
        .trim()
        .strip_prefix("gitdir:")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "malformed-gitfile".to_string())?;
    let target = PathBuf::from(value);
    Ok(if target.is_absolute() {
        target
    } else {
        repository.join(target)
    })
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
    let gitdir = gitfile_target(repository)?;
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
) -> (
    BTreeMap<PathBuf, PathBuf>,
    Vec<OrphanWorktreeMarker>,
    Vec<WorktreeScanIssue>,
) {
    if !root.is_dir() {
        return (BTreeMap::new(), Vec::new(), Vec::new());
    }
    let mut seeds = BTreeMap::new();
    let mut orphaned = Vec::new();
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
                Err(reason) => {
                    if let Ok(missing_git_dir) = gitfile_target(&path) {
                        if !missing_git_dir.exists() {
                            orphaned.push(OrphanWorktreeMarker {
                                path: path.clone(),
                                missing_git_dir,
                            });
                        }
                    }
                    issues.push(WorktreeScanIssue {
                        path: path.to_string_lossy().into_owned(),
                        operation: "resolve-git-common-dir".into(),
                        reason,
                    });
                }
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
    orphaned.sort_by(|left, right| left.path.cmp(&right.path));
    orphaned.dedup_by(|left, right| left.path == right.path);
    (seeds, orphaned, issues)
}

#[cfg(not(coverage))]
fn local_default_ref(repository: &Path, budget: InventoryBudget) -> Option<String> {
    let symbolic = git_output_with_budget(
        repository,
        &["symbolic-ref", "--quiet", "refs/remotes/origin/HEAD"],
        budget,
        GIT_TIMEOUT,
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
    let refs = git_text_with_budget(repository, &["show-ref"], budget)?;
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
fn dirty_state(path: &Path, budget: InventoryBudget) -> Option<bool> {
    let output = git_output_with_budget(
        path,
        &["status", "--porcelain=v1", "--untracked-files=normal"],
        budget,
        GIT_TIMEOUT,
    )
    .ok()?;
    output.status.success().then(|| !output.stdout.is_empty())
}

#[cfg(not(coverage))]
fn ahead_behind(
    repository: &Path,
    default_ref: &str,
    head: &str,
    budget: InventoryBudget,
) -> Option<(u64, u64)> {
    let range = format!("{default_ref}...{head}");
    let value = git_text_with_budget(
        repository,
        &["rev-list", "--left-right", "--count", &range],
        budget,
    )?;
    let mut fields = value.split_whitespace();
    let behind = fields.next()?.parse().ok()?;
    let ahead = fields.next()?.parse().ok()?;
    Some((ahead, behind))
}

#[cfg(not(coverage))]
fn merged_into(
    repository: &Path,
    head: &str,
    default_ref: &str,
    budget: InventoryBudget,
) -> Option<bool> {
    let output = git_output_with_budget(
        repository,
        &["merge-base", "--is-ancestor", head, default_ref],
        budget,
        GIT_TIMEOUT,
    )
    .ok()?;
    match output.status.code() {
        Some(0) => Some(true),
        Some(1) => Some(false),
        _ => None,
    }
}

#[cfg(not(coverage))]
fn commit_time_ms(repository: &Path, head: &str, budget: InventoryBudget) -> u64 {
    git_text_with_budget(repository, &["show", "-s", "--format=%ct", head], budget)
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

fn generated_artifact_kind(name: &str) -> Option<&'static str> {
    GENERATED_ARTIFACT_DIRECTORY_NAMES
        .iter()
        .copied()
        .find(|candidate| *candidate == name)
}

#[cfg(not(coverage))]
fn filesystem_evidence_with_limits(
    root: &Path,
    max_entries: usize,
    timeout: Duration,
) -> FilesystemEvidence {
    if !root.is_dir() {
        return FilesystemEvidence::default();
    }
    let mut evidence = FilesystemEvidence::default();
    let mut generated_bytes = BTreeMap::<(PathBuf, String), u64>::new();
    let mut inspected = 0usize;
    let started = Instant::now();
    let mut directories = vec![(root.to_path_buf(), None::<(PathBuf, String)>)];
    evidence.complete = true;
    'scan: while let Some((directory, artifact_root)) = directories.pop() {
        if started.elapsed() >= timeout {
            evidence.complete = false;
            break;
        }
        let Ok(entries) = std::fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            inspected = inspected.saturating_add(1);
            if inspected > max_entries || started.elapsed() >= timeout {
                evidence.complete = false;
                break 'scan;
            }
            let path = entry.path();
            let Ok(metadata) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                if entry.file_name() != ".git" {
                    let nested_artifact = artifact_root.clone().or_else(|| {
                        entry.file_name().to_str().and_then(|name| {
                            generated_artifact_kind(name)
                                .map(|kind| (path.clone(), kind.to_string()))
                        })
                    });
                    directories.push((path, nested_artifact));
                }
                continue;
            }
            if metadata.is_file() {
                let allocated = allocated_bytes(&metadata);
                evidence.allocated_bytes = evidence.allocated_bytes.saturating_add(allocated);
                if let Some(artifact) = &artifact_root {
                    let value = generated_bytes.entry(artifact.clone()).or_default();
                    *value = value.saturating_add(allocated);
                }
                let modified_ms = metadata
                    .modified()
                    .ok()
                    .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|duration| duration.as_millis() as u64)
                    .unwrap_or(0);
                evidence.latest_ms = evidence.latest_ms.max(modified_ms);
            }
        }
    }
    evidence.generated_artifacts = generated_bytes
        .into_iter()
        .map(|((path, kind), allocated_bytes)| GeneratedArtifact {
            path: path.to_string_lossy().into_owned(),
            kind,
            allocated_bytes,
            gitignore_confirmed: None,
            gitignore_source: None,
            gitignore_line: None,
            gitignore_pattern: None,
            gitignore_issue: Some("gitignore-evidence-not-collected".into()),
        })
        .collect();
    evidence
}

#[cfg(not(coverage))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct GitignoreMatch {
    source: String,
    line: u64,
    pattern: String,
    path: String,
}

#[cfg(not(coverage))]
fn parse_check_ignore_verbose_z(output: &[u8]) -> Result<Vec<GitignoreMatch>, String> {
    if output.is_empty() {
        return Ok(Vec::new());
    }
    if output.last() != Some(&0) {
        return Err("check-ignore-output-missing-final-nul".into());
    }
    let fields = output[..output.len() - 1]
        .split(|byte| *byte == 0)
        .collect::<Vec<_>>();
    if fields.len() % 4 != 0 {
        return Err(format!(
            "check-ignore-output-field-count-not-divisible-by-four-{}",
            fields.len()
        ));
    }
    fields
        .chunks_exact(4)
        .map(|record| {
            let source = std::str::from_utf8(record[0])
                .map_err(|_| "check-ignore-source-not-utf8".to_string())?;
            let line = std::str::from_utf8(record[1])
                .map_err(|_| "check-ignore-line-not-utf8".to_string())?
                .parse::<u64>()
                .map_err(|_| "check-ignore-line-not-u64".to_string())?;
            let pattern = std::str::from_utf8(record[2])
                .map_err(|_| "check-ignore-pattern-not-utf8".to_string())?;
            let path = std::str::from_utf8(record[3])
                .map_err(|_| "check-ignore-path-not-utf8".to_string())?;
            if source.is_empty() || pattern.is_empty() || path.is_empty() {
                return Err("check-ignore-output-has-empty-field".into());
            }
            Ok(GitignoreMatch {
                source: source.into(),
                line,
                pattern: pattern.into(),
                path: path.replace('\\', "/"),
            })
        })
        .collect()
}

#[cfg(not(coverage))]
fn git_relative_path(root: &Path, path: &Path) -> Result<String, String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| "generated-artifact-outside-worktree".to_string())?;
    let value = relative
        .components()
        .map(|component| {
            component
                .as_os_str()
                .to_str()
                .ok_or_else(|| "generated-artifact-path-not-utf8".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?
        .join("/");
    if value.is_empty() {
        return Err("generated-artifact-path-empty".into());
    }
    Ok(value)
}

#[cfg(not(coverage))]
fn annotate_generated_artifact_gitignore(
    worktree_path: &Path,
    artifacts: &mut [GeneratedArtifact],
    budget: InventoryBudget,
) -> Result<(), String> {
    if artifacts.is_empty() {
        return Ok(());
    }
    let mut requested = BTreeMap::<String, usize>::new();
    let mut input = Vec::new();
    for (index, artifact) in artifacts.iter().enumerate() {
        let relative = git_relative_path(worktree_path, Path::new(&artifact.path))?;
        if requested.insert(relative.clone(), index).is_some() {
            return Err(format!("duplicate-generated-artifact-path-{relative}"));
        }
        input.extend_from_slice(relative.as_bytes());
        input.push(0);
        if input.len() > GIT_CHECK_IGNORE_MAX_INPUT_BYTES {
            return Err(format!(
                "check-ignore-input-limit-exceeded-{GIT_CHECK_IGNORE_MAX_INPUT_BYTES}-bytes"
            ));
        }
    }
    let timeout = budget
        .capped(GIT_TIMEOUT)
        .ok_or_else(|| "inventory-deadline-exhausted".to_string())?;
    let output = git_output_with_input_and_timeout(
        worktree_path,
        &["check-ignore", "-v", "-z", "--stdin"],
        Some(input),
        timeout,
        GIT_CHECK_IGNORE_MAX_OUTPUT_BYTES,
    )?;
    if !matches!(output.status.code(), Some(0 | 1)) {
        return Err(format!(
            "git-exit-{}:{}",
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let matches = parse_check_ignore_verbose_z(&output.stdout)?;
    let mut matched = BTreeSet::new();
    for evidence in matches {
        let Some(index) = requested.get(&evidence.path).copied() else {
            return Err(format!(
                "check-ignore-returned-unrequested-path-{}",
                evidence.path
            ));
        };
        if !matched.insert(index) {
            return Err(format!(
                "check-ignore-returned-duplicate-path-{}",
                evidence.path
            ));
        }
        let artifact = &mut artifacts[index];
        artifact.gitignore_confirmed = Some(!evidence.pattern.starts_with('!'));
        artifact.gitignore_source = Some(evidence.source);
        artifact.gitignore_line = Some(evidence.line);
        artifact.gitignore_pattern = Some(evidence.pattern);
        artifact.gitignore_issue = None;
    }
    for (index, artifact) in artifacts.iter_mut().enumerate() {
        if !matched.contains(&index) {
            artifact.gitignore_confirmed = Some(false);
            artifact.gitignore_source = None;
            artifact.gitignore_line = None;
            artifact.gitignore_pattern = None;
            artifact.gitignore_issue = None;
        }
    }
    Ok(())
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
    inventory_with_timeout(root, min_age_days, now_ms, INVENTORY_TIMEOUT)
}

/// Inventory with a caller-selected total runtime budget.
///
/// The budget covers repository discovery, Git evidence, and filesystem measurement together.
/// When it expires, the report remains read-only and fail-closed with partial evidence plus an
/// `inventory-repositories` scan issue.
#[cfg(not(coverage))]
pub fn inventory_with_timeout(
    root: &Path,
    min_age_days: u64,
    now_ms: u64,
    timeout: Duration,
) -> WorktreeReport {
    let inventory_started = Instant::now();
    let budget = InventoryBudget::new(timeout);
    let discovery_timeout = budget
        .capped(REPOSITORY_SEARCH_TIMEOUT)
        .unwrap_or(Duration::ZERO);
    let (repositories, orphan_markers, mut scan_issues) = repository_seeds_with_limits(
        root,
        REPOSITORY_SEARCH_MAX_DEPTH,
        REPOSITORY_SEARCH_MAX_DIRECTORIES,
        discovery_timeout,
    );
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
        if budget.exhausted() {
            scan_issues.push(WorktreeScanIssue {
                path: root.to_string_lossy().into_owned(),
                operation: "inventory-repositories".into(),
                reason: format!("timeout-after-{}ms", timeout.as_millis()),
            });
            break 'repositories;
        }
        // Prove that the repository can enumerate linked worktrees before spending another
        // timeout window resolving refs. A failed preflight already makes the repository
        // fail-closed, which matters for old macOS workspaces with pathological local metadata.
        let raw = match git_output_with_budget(
            seed,
            &["worktree", "list", "--porcelain"],
            budget,
            GIT_WORKTREE_LIST_TIMEOUT,
        ) {
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
        let default_ref = local_default_ref(seed, budget);
        for (index, worktree) in raw.into_iter().enumerate() {
            if budget.exhausted() {
                scan_issues.push(WorktreeScanIssue {
                    path: root.to_string_lossy().into_owned(),
                    operation: "inventory-repositories".into(),
                    reason: format!("timeout-after-{}ms", timeout.as_millis()),
                });
                break 'repositories;
            }
            let is_primary = index == 0;
            let exists = worktree.path.is_dir();
            let dirty = (exists && !is_primary)
                .then(|| dirty_state(&worktree.path, budget))
                .flatten();
            let filesystem_scanned = exists && !is_primary;
            let filesystem_scan_remaining =
                FILESYSTEM_SCAN_TOTAL_TIMEOUT.saturating_sub(filesystem_scan_started.elapsed());
            let total_remaining = budget.remaining();
            let mut filesystem_evidence = if filesystem_scanned
                && !filesystem_scan_remaining.is_zero()
                && !total_remaining.is_zero()
            {
                filesystem_evidence_with_limits(
                    &worktree.path,
                    FILESYSTEM_SCAN_MAX_ENTRIES,
                    FILESYSTEM_SCAN_TIMEOUT
                        .min(filesystem_scan_remaining)
                        .min(total_remaining),
                )
            } else {
                FilesystemEvidence::default()
            };
            if filesystem_scanned && !filesystem_evidence.generated_artifacts.is_empty() {
                if let Err(reason) = annotate_generated_artifact_gitignore(
                    &worktree.path,
                    &mut filesystem_evidence.generated_artifacts,
                    budget,
                ) {
                    for artifact in &mut filesystem_evidence.generated_artifacts {
                        artifact.gitignore_confirmed = None;
                        artifact.gitignore_source = None;
                        artifact.gitignore_line = None;
                        artifact.gitignore_pattern = None;
                        artifact.gitignore_issue = Some(reason.clone());
                    }
                    scan_issues.push(WorktreeScanIssue {
                        path: worktree.path.to_string_lossy().into_owned(),
                        operation: "check-generated-artifact-ignore".into(),
                        reason,
                    });
                }
            }
            let filesystem_scan_complete = filesystem_evidence.complete;
            let latest_file_ms = filesystem_evidence.latest_ms;
            let generated_artifact_bytes = filesystem_evidence
                .generated_artifacts
                .iter()
                .map(|artifact| artifact.allocated_bytes)
                .sum();
            let commit_ms = commit_time_ms(seed, &worktree.head, budget);
            let last_activity_ms = latest_file_ms.max(commit_ms);
            let age_days = now_ms.saturating_sub(last_activity_ms) / DAY_MS;
            let (ahead, behind) = if is_primary {
                (None, None)
            } else {
                default_ref
                    .as_deref()
                    .and_then(|reference| ahead_behind(seed, reference, &worktree.head, budget))
                    .map(|(ahead, behind)| (Some(ahead), Some(behind)))
                    .unwrap_or((None, None))
            };
            let merged = (!is_primary)
                .then(|| {
                    default_ref
                        .as_deref()
                        .and_then(|reference| merged_into(seed, &worktree.head, reference, budget))
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
                allocated_bytes: filesystem_evidence.allocated_bytes,
                generated_artifact_bytes,
                generated_artifacts: filesystem_evidence.generated_artifacts,
                filesystem_scanned,
                filesystem_scan_complete,
                removal_eligible,
                metadata_prune_eligible,
                review_reasons,
            });
        }
    }
    let mut orphaned_worktrees = Vec::new();
    for orphan in orphan_markers {
        if budget.exhausted() {
            scan_issues.push(WorktreeScanIssue {
                path: root.to_string_lossy().into_owned(),
                operation: "inventory-repositories".into(),
                reason: format!("timeout-after-{}ms", timeout.as_millis()),
            });
            break;
        }
        let filesystem_scan_remaining =
            FILESYSTEM_SCAN_TOTAL_TIMEOUT.saturating_sub(filesystem_scan_started.elapsed());
        let total_remaining = budget.remaining();
        let mut evidence = if filesystem_scan_remaining.is_zero() || total_remaining.is_zero() {
            FilesystemEvidence::default()
        } else {
            filesystem_evidence_with_limits(
                &orphan.path,
                FILESYSTEM_SCAN_MAX_ENTRIES,
                FILESYSTEM_SCAN_TIMEOUT
                    .min(filesystem_scan_remaining)
                    .min(total_remaining),
            )
        };
        for artifact in &mut evidence.generated_artifacts {
            artifact.gitignore_confirmed = None;
            artifact.gitignore_source = None;
            artifact.gitignore_line = None;
            artifact.gitignore_pattern = None;
            artifact.gitignore_issue = Some("git-worktree-metadata-missing".into());
        }
        let generated_artifact_bytes = evidence
            .generated_artifacts
            .iter()
            .map(|artifact| artifact.allocated_bytes)
            .sum();
        let mut review_reasons = vec![
            "git-worktree-metadata-missing".to_string(),
            "git-status-unavailable".to_string(),
            "source-tree-removal-prohibited".to_string(),
        ];
        if !evidence.complete {
            review_reasons.push("filesystem-evidence-incomplete".into());
        }
        orphaned_worktrees.push(OrphanWorktreeCandidate {
            path: orphan.path.to_string_lossy().into_owned(),
            missing_git_dir: orphan.missing_git_dir.to_string_lossy().into_owned(),
            allocated_bytes: evidence.allocated_bytes,
            generated_artifact_bytes,
            generated_artifacts: evidence.generated_artifacts,
            filesystem_scan_complete: evidence.complete,
            removal_eligible: false,
            review_reasons,
        });
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
    let reviewable_generated_artifact_bytes = worktrees
        .iter()
        .filter(|worktree| !worktree.is_primary)
        .map(|worktree| worktree.generated_artifact_bytes)
        .chain(
            orphaned_worktrees
                .iter()
                .map(|worktree| worktree.generated_artifact_bytes),
        )
        .sum();
    let ignore_confirmed_generated_artifact_bytes = worktrees
        .iter()
        .flat_map(|worktree| &worktree.generated_artifacts)
        .filter(|artifact| artifact.gitignore_confirmed == Some(true))
        .map(|artifact| artifact.allocated_bytes)
        .sum();
    let evidence_complete = scan_issues.is_empty()
        && worktrees
            .iter()
            .filter(|worktree| worktree.exists && !worktree.is_primary)
            .all(|worktree| {
                worktree.filesystem_scan_complete
                    && worktree
                        .generated_artifacts
                        .iter()
                        .all(|artifact| artifact.gitignore_confirmed.is_some())
            })
        && orphaned_worktrees.iter().all(|worktree| {
            worktree.filesystem_scan_complete
                && worktree
                    .generated_artifacts
                    .iter()
                    .all(|artifact| artifact.gitignore_confirmed.is_some())
        });
    WorktreeReport {
        scanned_root: root.to_string_lossy().into_owned(),
        generated_at_ms: now_ms,
        elapsed_ms: inventory_started.elapsed().as_millis() as u64,
        evidence_complete,
        min_age_days,
        search_max_depth: REPOSITORY_SEARCH_MAX_DEPTH,
        repository_count: registered_worktree_repository_count,
        worktrees,
        orphaned_worktrees,
        potentially_reclaimable_bytes,
        reviewable_generated_artifact_bytes,
        ignore_confirmed_generated_artifact_bytes,
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
            "caller-selected-total-runtime-budget-covers-discovery-git-and-filesystem-evidence"
                .into(),
            "worktree-list-preflight-precedes-secondary-git-evidence".into(),
            "report-evidence-complete-only-when-all-bounded-probes-finish".into(),
            "orphaned-worktree-source-trees-are-never-removal-eligible".into(),
            "generated-artifacts-are-reported-for-separate-review-only".into(),
            "generated-artifact-gitignore-evidence-is-batched-and-fail-closed".into(),
            "generated-artifact-cleanup-requires-gitignore-confirmed-and-fresh-active-use-check"
                .into(),
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

    #[test]
    fn generated_artifact_policy_is_exact_and_bounded() {
        for name in GENERATED_ARTIFACT_DIRECTORY_NAMES {
            assert_eq!(generated_artifact_kind(name), Some(*name));
        }
        assert_eq!(generated_artifact_kind("node_modules_backup"), None);
        assert_eq!(generated_artifact_kind("build"), None);
        assert_eq!(generated_artifact_kind("dist"), None);
    }

    #[cfg(not(coverage))]
    #[test]
    fn parses_batched_verbose_check_ignore_records() {
        let parsed = parse_check_ignore_verbose_z(
            b".gitignore\012\0target/\0nested/target\0.git/info/exclude\03\0!node_modules/\0node_modules\0",
        )
        .unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].source, ".gitignore");
        assert_eq!(parsed[0].line, 12);
        assert_eq!(parsed[0].pattern, "target/");
        assert_eq!(parsed[0].path, "nested/target");
        assert_eq!(parsed[1].pattern, "!node_modules/");
    }

    #[cfg(not(coverage))]
    #[test]
    fn malformed_verbose_check_ignore_output_fails_closed() {
        assert_eq!(
            parse_check_ignore_verbose_z(b".gitignore\01\0target/\0target").unwrap_err(),
            "check-ignore-output-missing-final-nul"
        );
        assert!(parse_check_ignore_verbose_z(b".gitignore\01\0target/\0")
            .unwrap_err()
            .starts_with("check-ignore-output-field-count-not-divisible-by-four"));
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
        let evidence = filesystem_evidence_with_limits(
            tmp.path(),
            FILESYSTEM_SCAN_MAX_ENTRIES,
            FILESYSTEM_SCAN_TIMEOUT,
        );
        assert!(evidence.allocated_bytes > 0);
        assert!(evidence.allocated_bytes < 1024 * 1024);
        assert!(evidence.latest_ms > 0);
        assert!(evidence.complete);
        assert!(evidence.generated_artifacts.is_empty());
    }

    #[cfg(not(coverage))]
    #[test]
    fn filesystem_evidence_reports_incomplete_when_entry_budget_is_exhausted() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("one"), b"one").unwrap();
        std::fs::write(tmp.path().join("two"), b"two").unwrap();
        let evidence = filesystem_evidence_with_limits(tmp.path(), 1, Duration::from_secs(30));
        assert!(!evidence.complete);
    }

    #[cfg(not(coverage))]
    #[test]
    fn non_repository_tree_returns_empty_report() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("ordinary")).unwrap();
        let report = inventory(tmp.path(), 30, system_now_ms());
        assert_eq!(report.repository_count, 0);
        assert!(report.worktrees.is_empty());
        assert!(report.orphaned_worktrees.is_empty());
        assert!(report.scan_issues.is_empty());
        assert!(report.evidence_complete);
        assert_eq!(report.potentially_reclaimable_bytes, 0);
        assert_eq!(report.reviewable_generated_artifact_bytes, 0);
        assert_eq!(report.ignore_confirmed_generated_artifact_bytes, 0);
    }

    #[cfg(not(coverage))]
    #[test]
    fn caller_deadline_returns_a_fail_closed_partial_report() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("ordinary")).unwrap();
        let report = inventory_with_timeout(tmp.path(), 30, system_now_ms(), Duration::ZERO);
        assert!(!report.evidence_complete);
        assert!(report.worktrees.is_empty());
        assert!(report.scan_issues.iter().any(|issue| {
            issue.operation == "discover-repositories" && issue.reason == "timeout-after-0s"
        }));
        assert!(report.notices.iter().any(|notice| {
            notice
                == "caller-selected-total-runtime-budget-covers-discovery-git-and-filesystem-evidence"
        }));
    }

    #[cfg(not(coverage))]
    #[test]
    fn repository_discovery_reports_an_exhausted_directory_budget() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("first")).unwrap();
        let (_, _, issues) = repository_seeds_with_limits(
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
    fn orphaned_gitfile_reports_generated_artifacts_but_protects_source_tree() {
        let tmp = tempfile::tempdir().unwrap();
        let orphan = tmp.path().join("orphan");
        let generated = orphan.join("frontend/node_modules/package");
        std::fs::create_dir_all(&generated).unwrap();
        std::fs::write(orphan.join("source.txt"), b"preserve me").unwrap();
        std::fs::write(generated.join("index.js"), vec![1_u8; 4096]).unwrap();
        let missing = tmp.path().join("repository/.git/worktrees/orphan");
        std::fs::write(
            orphan.join(".git"),
            format!("gitdir: {}\n", missing.display()),
        )
        .unwrap();

        let report = inventory(tmp.path(), 0, system_now_ms());
        assert_eq!(report.repository_count, 0);
        assert!(report.worktrees.is_empty());
        assert_eq!(report.orphaned_worktrees.len(), 1);
        let candidate = &report.orphaned_worktrees[0];
        assert_eq!(Path::new(&candidate.path), orphan);
        assert_eq!(Path::new(&candidate.missing_git_dir), missing);
        assert!(!candidate.removal_eligible);
        assert!(candidate.filesystem_scan_complete);
        assert!(candidate.allocated_bytes >= candidate.generated_artifact_bytes);
        assert!(candidate.generated_artifact_bytes > 0);
        assert_eq!(candidate.generated_artifacts.len(), 1);
        assert_eq!(candidate.generated_artifacts[0].kind, "node_modules");
        assert_eq!(candidate.generated_artifacts[0].gitignore_confirmed, None);
        assert_eq!(
            candidate.generated_artifacts[0].gitignore_issue.as_deref(),
            Some("git-worktree-metadata-missing")
        );
        assert!(candidate
            .review_reasons
            .contains(&"source-tree-removal-prohibited".to_string()));
        assert_eq!(
            report.reviewable_generated_artifact_bytes,
            candidate.generated_artifact_bytes
        );
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
        std::fs::write(repository.join(".gitignore"), b"target/\n").unwrap();
        std::fs::create_dir(repository.join("node_modules")).unwrap();
        std::fs::write(repository.join("node_modules/tracked.js"), b"tracked").unwrap();
        git(
            &repository,
            &[
                "add",
                "tracked.txt",
                ".gitignore",
                "node_modules/tracked.js",
            ],
        );
        git(&repository, &["commit", "-m", "initial"]);
        git(&repository, &["branch", "-M", "main"]);
        git(&repository, &["branch", "old"]);
        git(
            &repository,
            &["worktree", "add", linked.to_str().unwrap(), "old"],
        );
        std::fs::create_dir(linked.join("target")).unwrap();
        std::fs::write(linked.join("target/output.bin"), vec![2_u8; 4096]).unwrap();
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
                && worktree.last_activity_ms > 0
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
        let linked_report = report
            .worktrees
            .iter()
            .find(|worktree| Path::new(&worktree.path) == linked_identity)
            .unwrap();
        let ignored = linked_report
            .generated_artifacts
            .iter()
            .find(|artifact| artifact.kind == "target")
            .unwrap();
        assert_eq!(ignored.gitignore_confirmed, Some(true));
        assert_eq!(ignored.gitignore_source.as_deref(), Some(".gitignore"));
        assert_eq!(ignored.gitignore_line, Some(1));
        assert_eq!(ignored.gitignore_pattern.as_deref(), Some("target/"));
        assert_eq!(ignored.gitignore_issue, None);
        let tracked = linked_report
            .generated_artifacts
            .iter()
            .find(|artifact| artifact.kind == "node_modules")
            .unwrap();
        assert_eq!(tracked.gitignore_confirmed, Some(false));
        assert_eq!(tracked.gitignore_source, None);
        assert!(report.ignore_confirmed_generated_artifact_bytes > 0);
        assert!(report.evidence_complete, "{report:#?}");
    }
}
