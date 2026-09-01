//! Exact-head GitHub PR evidence for reclaiming independent, inactive Git clones.

use crate::git_worktree::{
    active_use_evidence, run_bounded_command, size_evidence, GitWorktreeActiveUseEvidence,
    GitWorktreeSizeEvidence,
};
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;

const SCHEMA_VERSION: u32 = 1;
const COMMAND_TIMEOUT_MS: u64 = 60_000;
const SIZE_TIMEOUT_MS: u64 = 60_000;
const MAX_ENTRIES: u64 = 2_000_000;
const MAX_OPEN_AGE_DAYS: u64 = 3_650;
const MAX_BATCH_REPOSITORIES: usize = 10_000;
const MAX_BATCH_CONCURRENCY: usize = 32;
const MAX_BATCH_DEPTH: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PullRequestEvidence {
    pub number: u64,
    pub state: String,
    #[serde(rename = "headRefName")]
    pub head_ref_name: String,
    #[serde(rename = "headRefOid")]
    pub head_ref_oid: String,
    #[serde(rename = "createdAtMs")]
    pub created_at_ms: u64,
    pub url: String,
    pub association_method: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StaleGitClonePlan {
    pub schema_kind: &'static str,
    pub schema_version: u32,
    pub ontology_class: &'static str,
    pub path: String,
    pub repository: String,
    pub branch: String,
    pub head: String,
    pub pull_request: Option<PullRequestEvidence>,
    pub status_clean: bool,
    pub active_use: GitWorktreeActiveUseEvidence,
    pub size: GitWorktreeSizeEvidence,
    pub eligible_after_human_approval: bool,
    pub blockers: Vec<String>,
    pub plan_fingerprint: String,
    pub exact_approval_phrase: Option<String>,
    pub filesystem_mutation_executed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StaleGitCloneRemoval {
    pub schema_version: u32,
    pub ontology_class: &'static str,
    pub plan_fingerprint: String,
    pub repository: String,
    pub branch: String,
    pub head: String,
    pub pull_request_number: u64,
    pub removed_allocated_bytes_upper_bound: u64,
    pub rationale: String,
    pub executed_at_ms: u64,
    pub filesystem_mutation_executed: bool,
    pub path_absence_verified: bool,
    pub recoverability: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StaleGitCloneBatchEntry {
    pub path: String,
    pub plan: Option<StaleGitClonePlan>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StaleGitCloneBatchPlan {
    pub schema_kind: &'static str,
    pub schema_version: u32,
    pub ontology_class: &'static str,
    pub scan_root: String,
    pub max_depth: usize,
    pub concurrency: usize,
    pub repository_count: usize,
    pub eligible_count: usize,
    pub eligible_allocated_bytes: u64,
    pub evidence_gap_count: usize,
    pub entries: Vec<StaleGitCloneBatchEntry>,
    pub filesystem_mutation_executed: bool,
}

pub(crate) fn bounded_parallel_map<T, F>(
    paths: Vec<PathBuf>,
    concurrency: usize,
    worker: F,
) -> Vec<T>
where
    T: Send + 'static,
    F: Fn(PathBuf) -> T + Send + Sync + 'static,
{
    let pending = Arc::new(Mutex::new(
        paths.into_iter().enumerate().collect::<Vec<_>>(),
    ));
    let results = Arc::new(Mutex::new(Vec::new()));
    let worker = Arc::new(worker);
    thread::scope(|scope| {
        for _ in 0..concurrency {
            let pending = Arc::clone(&pending);
            let results = Arc::clone(&results);
            let worker = Arc::clone(&worker);
            scope.spawn(move || loop {
                let Some((index, path)) = pending.lock().expect("batch queue poisoned").pop()
                else {
                    break;
                };
                let value = worker(path);
                results
                    .lock()
                    .expect("batch results poisoned")
                    .push((index, value));
            });
        }
    });
    let results = match Arc::try_unwrap(results) {
        Ok(results) => results,
        Err(_) => unreachable!("batch result workers still referenced"),
    };
    let mut results = results.into_inner().expect("batch results poisoned");
    results.sort_by_key(|(index, _)| *index);
    results.into_iter().map(|(_, value)| value).collect()
}

pub fn plan_stale_git_clones(
    requested_root: &Path,
    open_age_days: u64,
    now_ms: u64,
    max_repositories: usize,
    concurrency: usize,
    max_depth: usize,
) -> Result<StaleGitCloneBatchPlan, String> {
    if !requested_root.is_absolute()
        || max_repositories == 0
        || max_repositories > MAX_BATCH_REPOSITORIES
        || concurrency == 0
        || concurrency > MAX_BATCH_CONCURRENCY
        || max_depth == 0
        || max_depth > MAX_BATCH_DEPTH
    {
        return Err("stale-git-clone-batch-options-invalid".into());
    }
    let root = fs::canonicalize(requested_root)
        .map_err(|_| "stale-git-clone-batch-root-unavailable".to_string())?;
    let repositories = discover_independent_repositories(&root, max_repositories, max_depth)?;
    let worker_count = concurrency.min(repositories.len().max(1));
    let entries = bounded_parallel_map(repositories, worker_count, move |path| {
        let path_string = path.to_string_lossy().into_owned();
        match plan_stale_git_clone(&path, open_age_days, now_ms) {
            Ok(plan) => StaleGitCloneBatchEntry {
                path: path_string,
                plan: Some(plan),
                error: None,
            },
            Err(error) => StaleGitCloneBatchEntry {
                path: path_string,
                plan: None,
                error: Some(error),
            },
        }
    });
    let eligible_count = entries
        .iter()
        .filter(|entry| {
            entry
                .plan
                .as_ref()
                .is_some_and(|plan| plan.eligible_after_human_approval)
        })
        .count();
    let eligible_allocated_bytes = entries
        .iter()
        .filter_map(|entry| entry.plan.as_ref())
        .filter(|plan| plan.eligible_after_human_approval)
        .map(|plan| plan.size.allocated_bytes)
        .sum();
    let evidence_gap_count = entries
        .iter()
        .filter(|entry| {
            entry.error.is_some()
                || entry
                    .plan
                    .as_ref()
                    .is_some_and(|plan| !plan.blockers.is_empty())
        })
        .count();
    Ok(StaleGitCloneBatchPlan {
        schema_kind: "disksage.stale-git-clone-batch-plan",
        schema_version: SCHEMA_VERSION,
        ontology_class: "https://disksage.app/ontology#GitClone",
        scan_root: root.to_string_lossy().into_owned(),
        max_depth,
        concurrency: worker_count,
        repository_count: entries.len(),
        eligible_count,
        eligible_allocated_bytes,
        evidence_gap_count,
        entries,
        filesystem_mutation_executed: false,
    })
}

fn discover_independent_repositories(
    root: &Path,
    max_repositories: usize,
    max_depth: usize,
) -> Result<Vec<PathBuf>, String> {
    let mut pending = vec![(root.to_path_buf(), 0_usize)];
    let mut repositories = Vec::new();
    while let Some((directory, depth)) = pending.pop() {
        let mut children = fs::read_dir(&directory)
            .map_err(|_| "stale-git-clone-batch-discovery-incomplete".to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "stale-git-clone-batch-discovery-incomplete".to_string())?;
        children.sort_by_key(|entry| entry.file_name());
        for entry in children.into_iter().rev() {
            let path = entry.path();
            if matches!(
                entry.file_name().to_str(),
                Some(
                    "node_modules"
                        | "target"
                        | ".venv"
                        | "venv"
                        | ".cache"
                        | "vendor"
                        | "dist"
                        | "build"
                )
            ) {
                continue;
            }
            let metadata = fs::symlink_metadata(&path)
                .map_err(|_| "stale-git-clone-batch-discovery-incomplete".to_string())?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                continue;
            }
            let child_depth = depth + 1;
            let independent_git = fs::symlink_metadata(path.join(".git"))
                .ok()
                .is_some_and(|git| git.is_dir() && !git.file_type().is_symlink());
            if independent_git {
                repositories.push(path);
                if repositories.len() > max_repositories {
                    return Err("stale-git-clone-batch-repository-limit-exceeded".into());
                }
            } else if child_depth < max_depth {
                pending.push((path, child_depth));
            }
        }
    }
    repositories.sort();
    if repositories.len() > max_repositories {
        return Err("stale-git-clone-batch-repository-limit-exceeded".into());
    }
    Ok(repositories)
}

fn command_text(program: &str, args: &[&str], cwd: &Path, reason: &str) -> Result<String, String> {
    let args = args.iter().map(OsString::from).collect::<Vec<_>>();
    let result = run_bounded_command(program, &args, cwd, COMMAND_TIMEOUT_MS)?;
    if result.timed_out || result.stdout_truncated || result.stderr_truncated {
        return Err(format!("{reason}-incomplete"));
    }
    if result.status_code != Some(0) {
        return Err(format!("{reason}-failed"));
    }
    String::from_utf8(result.stdout).map_err(|_| format!("{reason}-not-utf8"))
}

fn tracked_files_clean(cwd: &Path) -> Result<bool, String> {
    let args = ["diff-index", "--quiet", "HEAD", "--"]
        .iter()
        .map(OsString::from)
        .collect::<Vec<_>>();
    let result = run_bounded_command("git", &args, cwd, COMMAND_TIMEOUT_MS)?;
    if result.timed_out || result.stdout_truncated || result.stderr_truncated {
        return Err("git-diff-index-incomplete".into());
    }
    match result.status_code {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err("git-diff-index-failed".into()),
    }
}

fn local_only_files(cwd: &Path) -> Result<String, String> {
    command_text(
        "git",
        &[
            "ls-files",
            "--others",
            "--exclude-standard",
            "--directory",
            "--no-empty-directory",
        ],
        cwd,
        "git-local-only",
    )
}

fn github_repository(remote: &str) -> Option<String> {
    let value = remote
        .strip_prefix("https://github.com/")
        .or_else(|| remote.strip_prefix("git@github.com:"))?
        .strip_suffix(".git")
        .unwrap_or_else(|| {
            remote
                .strip_prefix("https://github.com/")
                .or_else(|| remote.strip_prefix("git@github.com:"))
                .unwrap_or("")
        });
    let mut parts = value.split('/');
    let owner = parts.next()?;
    let repository = parts.next()?;
    if owner.is_empty() || repository.is_empty() || parts.next().is_some() {
        return None;
    }
    Some(format!("{owner}/{repository}"))
}

fn fingerprint(
    path: &str,
    repository: &str,
    branch: &str,
    head: &str,
    pull_request: &PullRequestEvidence,
    allocated_bytes: u64,
) -> String {
    let mut hasher = blake3::Hasher::new();
    for value in [
        "disksage.stale-git-clone.v1",
        path,
        repository,
        branch,
        head,
        &pull_request.number.to_string(),
        &pull_request.state,
        &pull_request.created_at_ms.to_string(),
        &pull_request.association_method,
        &allocated_bytes.to_string(),
    ] {
        hasher.update(&(value.len() as u64).to_le_bytes());
        hasher.update(value.as_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

fn pr_is_stale(pr: &PullRequestEvidence, now_ms: u64, open_age_days: u64) -> bool {
    matches!(pr.state.as_str(), "MERGED" | "CLOSED")
        || (pr.state == "OPEN"
            && now_ms.saturating_sub(pr.created_at_ms) >= open_age_days.saturating_mul(86_400_000))
}

fn unique_branch_association(
    pull_requests: Vec<PullRequestEvidence>,
    branch: &str,
) -> Option<PullRequestEvidence> {
    let mut matches = pull_requests
        .into_iter()
        .filter(|pr| pr.head_ref_name == branch);
    let found = matches.next()?;
    matches.next().is_none().then_some(found)
}

fn should_collect_size(
    status_clean: bool,
    active_use: &GitWorktreeActiveUseEvidence,
    pull_request: Option<&PullRequestEvidence>,
    now_ms: u64,
    open_age_days: u64,
) -> bool {
    status_clean
        && active_use.assessed
        && active_use.evidence_complete
        && !active_use.active
        && pull_request.is_some_and(|pr| pr_is_stale(pr, now_ms, open_age_days))
}

fn registered_worktree_count(output: &str) -> usize {
    output
        .lines()
        .filter(|line| line.starts_with("worktree "))
        .count()
}

pub fn plan_stale_git_clone(
    requested_path: &Path,
    open_age_days: u64,
    now_ms: u64,
) -> Result<StaleGitClonePlan, String> {
    if now_ms == 0 || open_age_days == 0 || open_age_days > MAX_OPEN_AGE_DAYS {
        return Err("stale-git-clone-options-invalid".into());
    }
    if !requested_path.is_absolute()
        || requested_path
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err("stale-git-clone-path-invalid".into());
    }
    let metadata = fs::symlink_metadata(requested_path)
        .map_err(|_| "stale-git-clone-path-unavailable".to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("stale-git-clone-path-unsafe".into());
    }
    let path = fs::canonicalize(requested_path)
        .map_err(|_| "stale-git-clone-path-unavailable".to_string())?;
    if std::env::current_dir()
        .ok()
        .and_then(|cwd| fs::canonicalize(cwd).ok())
        .is_some_and(|cwd| cwd.starts_with(&path))
    {
        return Err("stale-git-clone-actor-cwd-inside".into());
    }
    let git_dir = fs::symlink_metadata(path.join(".git"))
        .map_err(|_| "stale-git-clone-not-independent".to_string())?;
    if git_dir.file_type().is_symlink() || !git_dir.is_dir() {
        return Err("stale-git-clone-not-independent".into());
    }
    let root = command_text("git", &["rev-parse", "--show-toplevel"], &path, "git-root")?;
    let canonical_root = fs::canonicalize(root.trim()).map_err(|_| "git-root-invalid")?;
    if canonical_root != path {
        return Err("stale-git-clone-root-mismatch".into());
    }
    let worktrees = command_text(
        "git",
        &["worktree", "list", "--porcelain"],
        &path,
        "git-worktree-list",
    )?;
    if registered_worktree_count(&worktrees) != 1 {
        return Err("stale-git-clone-has-linked-worktrees".into());
    }
    let remote = command_text("git", &["remote", "get-url", "origin"], &path, "git-origin")?;
    let repository = github_repository(remote.trim())
        .ok_or_else(|| "stale-git-clone-origin-not-github".to_string())?;
    let branch = command_text(
        "git",
        &["symbolic-ref", "--short", "HEAD"],
        &path,
        "git-branch",
    )?
    .trim()
    .to_string();
    let default_branch = match command_text(
        "git",
        &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
        &path,
        "git-default-branch",
    ) {
        Ok(default_ref) => default_ref
            .trim()
            .strip_prefix("origin/")
            .unwrap_or(default_ref.trim())
            .to_string(),
        Err(_) => {
            let endpoint = format!("repos/{repository}");
            command_text(
                "gh",
                &[
                    "api",
                    "--method",
                    "GET",
                    &endpoint,
                    "--jq",
                    ".default_branch",
                ],
                &path,
                "github-default-branch-rest",
            )?
            .trim()
            .to_string()
        }
    };
    if default_branch == branch {
        return Err("stale-git-clone-default-branch".into());
    }
    let head = command_text("git", &["rev-parse", "HEAD"], &path, "git-head")?
        .trim()
        .to_string();
    let local_only = local_only_files(&path)?;
    let status_clean = tracked_files_clean(&path)? && local_only.is_empty();
    let owner = repository
        .split_once('/')
        .map(|(owner, _)| owner)
        .ok_or_else(|| "github-repository-owner-invalid".to_string())?;
    let endpoint = format!("repos/{repository}/pulls");
    let head_filter = format!("{owner}:{branch}");
    let head_argument = format!("head={head_filter}");
    let query = "[.[] | {number,state:(.state|ascii_upcase),headRefName:.head.ref,headRefOid:.head.sha,createdAtMs:((.created_at|fromdateiso8601)*1000),url:.html_url,association_method:\"exact-head\"}]";
    let pr_json = command_text(
        "gh",
        &[
            "api",
            "--method",
            "GET",
            &endpoint,
            "-f",
            "state=all",
            "-f",
            &head_argument,
            "-f",
            "per_page=100",
            "--paginate",
            "--jq",
            query,
        ],
        &path,
        "github-pr-list-rest",
    )?;
    let pull_requests: Vec<PullRequestEvidence> =
        serde_json::from_str(&pr_json).map_err(|_| "github-pr-json-invalid".to_string())?;
    let exact = pull_requests
        .into_iter()
        .filter(|pr| pr.head_ref_name == branch && pr.head_ref_oid == head)
        .collect::<Vec<_>>();
    let pull_request = if exact.len() == 1 {
        Some(exact[0].clone())
    } else {
        let endpoint = format!("repos/{repository}/commits/{head}/pulls");
        let query = "[.[] | {number,state:(.state|ascii_upcase),headRefName:.head.ref,headRefOid:.head.sha,createdAtMs:((.created_at|fromdateiso8601)*1000),url:.html_url,association_method:\"commit-associated\"}]";
        let json = command_text(
            "gh",
            &[
                "api",
                "--method",
                "GET",
                &endpoint,
                "-f",
                "per_page=100",
                "--jq",
                query,
            ],
            &path,
            "github-commit-pulls-rest",
        )?;
        let associated: Vec<PullRequestEvidence> = serde_json::from_str(&json)
            .map_err(|_| "github-commit-pulls-json-invalid".to_string())?;
        unique_branch_association(associated, &branch)
    };
    let active_use = active_use_evidence(&path, COMMAND_TIMEOUT_MS, 64, true);
    let collect_size = should_collect_size(
        status_clean,
        &active_use,
        pull_request.as_ref(),
        now_ms,
        open_age_days,
    );
    let size = if collect_size {
        size_evidence(&path, MAX_ENTRIES, SIZE_TIMEOUT_MS)
    } else {
        GitWorktreeSizeEvidence {
            method: "skipped-ineligible".into(),
            evidence_complete: false,
            allocated_bytes: 0,
            logical_bytes: 0,
            visited_entries: 0,
            error: Some("size-scan-skipped-ineligible".into()),
        }
    };
    let mut blockers = Vec::new();
    if !status_clean {
        blockers.push("git-clone-dirty".into());
    }
    if !active_use.assessed || !active_use.evidence_complete || active_use.active {
        blockers.push("git-clone-active-use-unresolved".into());
    }
    if collect_size && !size.evidence_complete {
        blockers.push("git-clone-size-evidence-incomplete".into());
    }
    match &pull_request {
        Some(pr) if !pr_is_stale(pr, now_ms, open_age_days) => {
            blockers.push("git-clone-pr-not-stale".into())
        }
        None => blockers.push("git-clone-exact-pr-head-unconfirmed".into()),
        _ => {}
    }
    let path_string = path.to_string_lossy().into_owned();
    let plan_fingerprint = pull_request
        .as_ref()
        .map(|pr| {
            fingerprint(
                &path_string,
                &repository,
                &branch,
                &head,
                pr,
                size.allocated_bytes,
            )
        })
        .unwrap_or_default();
    let eligible_after_human_approval = blockers.is_empty();
    let exact_approval_phrase = eligible_after_human_approval
        .then(|| format!("DiskSage stale Git clone 승인 {plan_fingerprint}"));
    Ok(StaleGitClonePlan {
        schema_kind: "disksage.stale-git-clone-plan",
        schema_version: SCHEMA_VERSION,
        ontology_class: "https://disksage.app/ontology#GitClone",
        path: path_string,
        repository,
        branch,
        head,
        pull_request,
        status_clean,
        active_use,
        size,
        eligible_after_human_approval,
        blockers,
        plan_fingerprint,
        exact_approval_phrase,
        filesystem_mutation_executed: false,
    })
}

pub fn remove_stale_git_clone(
    requested_path: &Path,
    open_age_days: u64,
    expected_fingerprint: &str,
    confirmation_phrase: &str,
    rationale: &str,
    executed_at_ms: u64,
) -> Result<StaleGitCloneRemoval, String> {
    if rationale.trim() != rationale
        || rationale.is_empty()
        || rationale.len() > 1_000
        || rationale.chars().any(char::is_control)
    {
        return Err("stale-git-clone-rationale-invalid".into());
    }
    let plan = plan_stale_git_clone(requested_path, open_age_days, executed_at_ms)?;
    if !plan.eligible_after_human_approval
        || plan.plan_fingerprint != expected_fingerprint
        || plan.exact_approval_phrase.as_deref() != Some(confirmation_phrase)
    {
        return Err("stale-git-clone-approval-mismatch".into());
    }
    let pull_request = plan
        .pull_request
        .as_ref()
        .ok_or_else(|| "stale-git-clone-pr-missing".to_string())?;
    fs::remove_dir_all(&plan.path).map_err(|_| "stale-git-clone-remove-failed".to_string())?;
    let path_absence_verified = !Path::new(&plan.path).exists();
    if !path_absence_verified {
        return Err("stale-git-clone-removal-unverified".into());
    }
    Ok(StaleGitCloneRemoval {
        schema_version: SCHEMA_VERSION,
        ontology_class: plan.ontology_class,
        plan_fingerprint: plan.plan_fingerprint,
        repository: plan.repository,
        branch: plan.branch,
        head: plan.head,
        pull_request_number: pull_request.number,
        removed_allocated_bytes_upper_bound: plan.size.allocated_bytes,
        rationale: rationale.into(),
        executed_at_ms,
        filesystem_mutation_executed: true,
        path_absence_verified,
        recoverability: "remote-reclone-only",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_closed_merged_or_old_open_exact_pr_is_stale() {
        let mut pr = PullRequestEvidence {
            number: 7,
            state: "OPEN".into(),
            head_ref_name: "feature".into(),
            head_ref_oid: "a".repeat(40),
            created_at_ms: 1_000,
            url: "https://github.com/example/repo/pull/7".into(),
            association_method: "exact-head".into(),
        };
        assert!(!pr_is_stale(&pr, 1_000 + 29 * 86_400_000, 30));
        assert!(pr_is_stale(&pr, 1_000 + 30 * 86_400_000, 30));
        pr.state = "CLOSED".into();
        assert!(pr_is_stale(&pr, 1_001, 30));
        pr.state = "MERGED".into();
        assert!(pr_is_stale(&pr, 1_001, 30));

        let active = GitWorktreeActiveUseEvidence {
            method: "test".into(),
            assessed: true,
            evidence_complete: true,
            active: false,
            observed_pids: Vec::new(),
            results_truncated: false,
            error: None,
        };
        assert!(should_collect_size(true, &active, Some(&pr), 1_001, 30));
        assert!(!should_collect_size(false, &active, Some(&pr), 1_001, 30));
    }

    #[test]
    fn commit_association_requires_one_matching_branch() {
        let evidence = PullRequestEvidence {
            number: 7,
            state: "CLOSED".into(),
            head_ref_name: "feature".into(),
            head_ref_oid: "a".repeat(40),
            created_at_ms: 1,
            url: "https://github.com/example/repo/pull/7".into(),
            association_method: "commit-associated".into(),
        };
        assert_eq!(
            unique_branch_association(vec![evidence.clone()], "feature"),
            Some(evidence.clone())
        );
        assert!(unique_branch_association(vec![evidence.clone(), evidence], "feature").is_none());
    }

    #[test]
    fn github_remote_parser_accepts_only_exact_repository_paths() {
        assert_eq!(
            github_repository("https://github.com/ContextualWisdomLab/disksage.git"),
            Some("ContextualWisdomLab/disksage".into())
        );
        assert_eq!(
            github_repository("git@github.com:ContextualWisdomLab/disksage.git"),
            Some("ContextualWisdomLab/disksage".into())
        );
        assert_eq!(github_repository("https://example.com/org/repo.git"), None);
        assert_eq!(github_repository("https://github.com/org/repo/extra"), None);
    }

    #[test]
    fn primary_clone_with_linked_worktrees_is_never_independent() {
        assert_eq!(registered_worktree_count("worktree /repo\nHEAD a\n"), 1);
        assert_eq!(
            registered_worktree_count("worktree /repo\nHEAD a\n\nworktree /repo/pr\nHEAD b\n"),
            2
        );
    }

    #[test]
    fn bounded_parallel_map_preserves_input_order_and_uses_multiple_workers() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::time::Duration;

        let active = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let observed_active = Arc::clone(&active);
        let observed_maximum = Arc::clone(&maximum);
        let paths = (0..8)
            .map(|index| PathBuf::from(index.to_string()))
            .collect();
        let values = bounded_parallel_map(paths, 4, move |path| {
            let current = observed_active.fetch_add(1, Ordering::SeqCst) + 1;
            observed_maximum.fetch_max(current, Ordering::SeqCst);
            thread::sleep(Duration::from_millis(5));
            observed_active.fetch_sub(1, Ordering::SeqCst);
            path.to_string_lossy().parse::<usize>().unwrap()
        });
        assert_eq!(values, (0..8).collect::<Vec<_>>());
        assert!(maximum.load(Ordering::SeqCst) > 1);
    }

    #[test]
    fn recursive_discovery_is_bounded_and_does_not_follow_symlinks_or_nested_repositories() {
        let root = std::env::temp_dir().join(format!(
            "disksage-stale-clone-discovery-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("date/repo/.git")).unwrap();
        fs::create_dir_all(root.join("date/repo/nested/.git")).unwrap();
        fs::create_dir_all(root.join("too/deep/repo/.git")).unwrap();
        fs::create_dir_all(root.join("date/target/generated-repo/.git")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(root.join("date"), root.join("linked-date")).unwrap();

        assert!(discover_independent_repositories(&root, 10, 1)
            .unwrap()
            .is_empty());
        assert_eq!(
            discover_independent_repositories(&root, 10, 2).unwrap(),
            vec![root.join("date/repo")]
        );
        assert!(discover_independent_repositories(&root, 0, 2).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn tracked_file_clean_check_distinguishes_committed_and_modified_content() {
        use std::process::Command;

        let root =
            std::env::temp_dir().join(format!("disksage-stale-clone-clean-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        for args in [
            vec!["init", "-q"],
            vec!["config", "user.email", "test@example.invalid"],
            vec!["config", "user.name", "DiskSage Test"],
        ] {
            assert!(Command::new("git")
                .args(args)
                .current_dir(&root)
                .status()
                .unwrap()
                .success());
        }
        fs::write(root.join("tracked"), "before").unwrap();
        assert!(Command::new("git")
            .args(["add", "tracked"])
            .current_dir(&root)
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .args(["commit", "-qm", "test"])
            .current_dir(&root)
            .status()
            .unwrap()
            .success());
        assert_eq!(tracked_files_clean(&root), Ok(true));
        fs::write(root.join("tracked"), "after").unwrap();
        assert_eq!(tracked_files_clean(&root), Ok(false));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ignored_untracked_files_are_local_data() {
        use std::process::Command;

        let root = std::env::temp_dir().join(format!(
            "disksage-stale-clone-ignored-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        for args in [
            vec!["init", "-q"],
            vec!["config", "user.email", "test@example.invalid"],
            vec!["config", "user.name", "DiskSage Test"],
        ] {
            assert!(Command::new("git")
                .args(args)
                .current_dir(&root)
                .status()
                .unwrap()
                .success());
        }
        fs::write(root.join(".gitignore"), "private.log\n").unwrap();
        fs::write(root.join("tracked"), "tracked").unwrap();
        assert!(Command::new("git")
            .args(["add", ".gitignore", "tracked"])
            .current_dir(&root)
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .args(["commit", "-qm", "test"])
            .current_dir(&root)
            .status()
            .unwrap()
            .success());
        fs::write(root.join("private.log"), "customer-only data").unwrap();

        assert_eq!(tracked_files_clean(&root), Ok(true));
        assert!(local_only_files(&root)
            .unwrap()
            .lines()
            .any(|path| path == "private.log"));
        fs::remove_dir_all(root).unwrap();
    }
}
