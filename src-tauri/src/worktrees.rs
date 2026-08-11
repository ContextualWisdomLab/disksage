//! Read-only stale Git worktree audit.
//!
//! The audit never runs `git worktree remove`, `git worktree prune`, or a filesystem delete.
//! It reports the exact local Git evidence needed for a later, explicitly reviewed cleanup.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const MAX_GIT_OUTPUT_BYTES: usize = 4 * 1024 * 1024;
const MAX_GIT_ADMIN_FILE_BYTES: u64 = 4 * 1024;
const GIT_WORKTREE_LIST_TIMEOUT: Duration = Duration::from_secs(5);
const GIT_ADMIN_FILE_READ_TIMEOUT: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct RawWorktree {
    path: PathBuf,
    head: String,
    branch: Option<String>,
    detached: bool,
    locked_reason: Option<String>,
    prunable_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct WorktreeCandidate {
    pub path: String,
    pub head: String,
    pub branch: Option<String>,
    pub is_primary: bool,
    pub detached: bool,
    pub exists: bool,
    pub locked_reason: Option<String>,
    pub prunable_reason: Option<String>,
    pub metadata_prune_eligible: bool,
    pub review_reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct WorktreeAudit {
    pub repository: String,
    pub generated_at_ms: u64,
    /// Digest of the exact repository registration and candidate evidence in this report.
    /// A future metadata-prune operation must re-audit and compare this value first.
    pub registration_fingerprint: String,
    pub evidence_complete: bool,
    pub worktrees: Vec<WorktreeCandidate>,
    pub stale_count: usize,
    pub metadata_prune_eligible_count: usize,
    pub notices: Vec<String>,
}

fn feed_fingerprint(hasher: &mut blake3::Hasher, value: &[u8]) {
    hasher.update(&(value.len() as u64).to_le_bytes());
    hasher.update(value);
}

fn registration_fingerprint(repository: &Path, worktrees: &[WorktreeCandidate]) -> String {
    let mut hasher = blake3::Hasher::new();
    feed_fingerprint(&mut hasher, repository.to_string_lossy().as_bytes());
    for worktree in worktrees {
        feed_fingerprint(&mut hasher, worktree.path.as_bytes());
        feed_fingerprint(&mut hasher, worktree.head.as_bytes());
        feed_fingerprint(
            &mut hasher,
            worktree
                .branch
                .as_deref()
                .unwrap_or("<no-branch>")
                .as_bytes(),
        );
        for flag in [
            worktree.is_primary,
            worktree.detached,
            worktree.exists,
            worktree.metadata_prune_eligible,
        ] {
            hasher.update(&[u8::from(flag)]);
        }
        feed_fingerprint(
            &mut hasher,
            worktree
                .locked_reason
                .as_deref()
                .unwrap_or("<unlocked>")
                .as_bytes(),
        );
        feed_fingerprint(
            &mut hasher,
            worktree
                .prunable_reason
                .as_deref()
                .unwrap_or("<not-prunable>")
                .as_bytes(),
        );
        for reason in &worktree.review_reasons {
            feed_fingerprint(&mut hasher, reason.as_bytes());
        }
        hasher.update(&[0xff]);
    }
    hasher.finalize().to_hex().to_string()
}

/// Parse Git's porcelain worktree records without interpreting arbitrary paths as commands.
fn parse_worktree_porcelain(input: &str) -> Vec<RawWorktree> {
    input
        .split("\n\n")
        .filter_map(|block| {
            let mut record = RawWorktree::default();
            for line in block.lines() {
                if let Some(value) = line.strip_prefix("worktree ") {
                    record.path = PathBuf::from(value);
                } else if let Some(value) = line.strip_prefix("HEAD ") {
                    record.head = value.to_string();
                } else if let Some(value) = line.strip_prefix("branch ") {
                    record.branch = Some(value.to_string());
                } else if line == "detached" {
                    record.detached = true;
                } else if line == "locked" {
                    record.locked_reason = Some(String::new());
                } else if let Some(value) = line.strip_prefix("locked ") {
                    record.locked_reason = Some(value.to_string());
                } else if line == "prunable" {
                    record.prunable_reason = Some(String::new());
                } else if let Some(value) = line.strip_prefix("prunable ") {
                    record.prunable_reason = Some(value.to_string());
                }
            }
            (!record.path.as_os_str().is_empty()).then_some(record)
        })
        .collect()
}

/// Run Git's worktree listing with a hard timeout. A malformed registration can otherwise make
/// `git worktree list` wait indefinitely while trying to resolve a missing worktree gitdir.
fn run_git_worktree_list(repository: &Path) -> Result<String, String> {
    let repository_string = repository.to_string_lossy().into_owned();
    let mut child = Command::new("git")
        .args([
            "-C",
            repository_string.as_str(),
            "worktree",
            "list",
            "--porcelain",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("git 실행 실패: {error}"))?;
    let deadline = Instant::now() + GIT_WORKTREE_LIST_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(25));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("git-worktree-list-timeout".into());
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("git-worktree-list-wait-failed".into());
            }
        }
    }
    let output = child
        .wait_with_output()
        .map_err(|error| format!("git 출력 수집 실패: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git worktree list 실패(exit={})",
            output.status.code().unwrap_or(-1)
        ));
    }
    if output.stdout.len() > MAX_GIT_OUTPUT_BYTES {
        return Err(format!(
            "git worktree 출력이 제한을 초과했습니다({MAX_GIT_OUTPUT_BYTES} bytes)"
        ));
    }
    String::from_utf8(output.stdout).map_err(|_| "git worktree 출력이 UTF-8이 아닙니다".into())
}

fn read_bounded_text(path: &Path) -> Result<String, String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| format!("worktree-admin-file-missing:{}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!("worktree-admin-file-unsafe:{}", path.display()));
    }
    if metadata.len() > MAX_GIT_ADMIN_FILE_BYTES {
        return Err(format!("worktree-admin-file-too-large:{}", path.display()));
    }
    let expected_len = metadata.len();
    let path = path.to_path_buf();
    let display_path = path.display().to_string();
    let (sender, receiver) = mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("disksage-git-admin-read".into())
        .spawn(move || {
            let result = (|| {
                let file = std::fs::File::open(&path).map_err(|_| {
                    format!("worktree-admin-file-open-failed:{}", path.display())
                })?;
                let mut bytes = Vec::new();
                file.take(MAX_GIT_ADMIN_FILE_BYTES + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|_| format!("worktree-admin-file-read-failed:{}", path.display()))?;
                if bytes.len() as u64 != expected_len
                    || bytes.len() as u64 > MAX_GIT_ADMIN_FILE_BYTES
                {
                    return Err(format!("worktree-admin-file-changed:{}", path.display()));
                }
                String::from_utf8(bytes)
                    .map_err(|_| format!("worktree-admin-file-not-utf8:{}", path.display()))
            })();
            let _ = sender.send(result);
        })
        .map_err(|_| format!("worktree-admin-file-reader-spawn-failed:{display_path}"))?;
    receiver
        .recv_timeout(GIT_ADMIN_FILE_READ_TIMEOUT)
        .map_err(|_| format!("worktree-admin-file-read-timeout:{display_path}"))?
}

fn resolve_relative_git_path(base: &Path, value: &str) -> PathBuf {
    let path = PathBuf::from(value.trim());
    if path.is_absolute() {
        path
    } else {
        base.join(path)
    }
}

fn parse_head_content(content: &str) -> (String, Option<String>, bool) {
    let head = content.lines().next().unwrap_or_default().trim().to_string();
    if let Some(branch) = head.strip_prefix("ref: ") {
        let branch = branch.trim().to_string();
        (head, Some(branch), false)
    } else {
        (head, None, true)
    }
}

fn primary_git_dir(repository: &Path, common_dir: &Path) -> PathBuf {
    let dot_git = repository.join(".git");
    if dot_git.is_dir() {
        return dot_git;
    }
    if let Ok(content) = read_bounded_text(&dot_git) {
        if let Some(value) = content.trim().strip_prefix("gitdir: ") {
            return resolve_relative_git_path(repository, value);
        }
    }
    common_dir.to_path_buf()
}

fn raw_from_git_admin(repository: &Path) -> Result<Vec<RawWorktree>, String> {
    let common_output = Command::new("git")
        .args(["-C", &repository.to_string_lossy(), "rev-parse", "--git-common-dir"])
        .output()
        .map_err(|_| "git-common-dir-command-failed".to_string())?;
    if !common_output.status.success() {
        return Err("git-common-dir-command-failed".into());
    }
    let common_value = String::from_utf8(common_output.stdout)
        .map_err(|_| "git-common-dir-output-not-utf8".to_string())?;
    let common_dir = resolve_relative_git_path(&repository, common_value.trim());
    let primary_dir = primary_git_dir(&repository, &common_dir);
    let primary_head = read_bounded_text(&primary_dir.join("HEAD"))
        .unwrap_or_default();
    let (primary_head, primary_branch, primary_detached) = parse_head_content(&primary_head);
    let mut records = vec![RawWorktree {
        path: repository.to_path_buf(),
        head: primary_head,
        branch: primary_branch,
        detached: primary_detached,
        locked_reason: None,
        prunable_reason: None,
    }];

    let admin_dir = common_dir.join("worktrees");
    let admin_metadata = std::fs::symlink_metadata(&admin_dir)
        .map_err(|_| "git-worktree-admin-directory-missing".to_string())?;
    if admin_metadata.file_type().is_symlink() || !admin_metadata.is_dir() {
        return Err("git-worktree-admin-directory-unsafe".into());
    }
    let mut entries = std::fs::read_dir(&admin_dir)
        .map_err(|_| "git-worktree-admin-directory-unreadable".to_string())?
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let entry_path = entry.path();
        let entry_metadata = match std::fs::symlink_metadata(&entry_path) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => metadata,
            _ => continue,
        };
        let _ = entry_metadata;
        let entry_name = entry.file_name().to_string_lossy().into_owned();
        let gitdir_path = entry_path.join("gitdir");
        let mut record = RawWorktree {
            path: entry_path.clone(),
            head: String::new(),
            branch: None,
            detached: true,
            locked_reason: None,
            prunable_reason: None,
        };
        match read_bounded_text(&gitdir_path) {
            Ok(value) if !value.trim().is_empty() => {
                let gitdir_target = resolve_relative_git_path(&entry_path, value.trim());
                record.path = gitdir_target
                    .file_name()
                    .and_then(|name| (name == ".git").then_some(gitdir_target.parent()))
                    .flatten()
                    .map(Path::to_path_buf)
                    .unwrap_or(gitdir_target.clone());
                if !gitdir_target.exists() {
                    record.prunable_reason = Some("gitdir-target-missing".into());
                }
            }
            Ok(_) => {
                record.path = PathBuf::from(format!("<worktree-admin:{entry_name}>"));
                record.prunable_reason = Some("gitdir-file-empty".into());
            }
            Err(error) => {
                record.path = PathBuf::from(format!("<worktree-admin:{entry_name}>"));
                record.prunable_reason = Some(error);
            }
        }
        if let Ok(head) = read_bounded_text(&entry_path.join("HEAD")) {
            let (head, branch, detached) = parse_head_content(&head);
            record.head = head;
            record.branch = branch;
            record.detached = detached;
        } else {
            record.prunable_reason.get_or_insert_with(|| "worktree-head-missing".into());
        }
        if let Ok(reason) = read_bounded_text(&entry_path.join("locked")) {
            record.locked_reason = Some(reason.trim().to_string());
        }
        if let Ok(reason) = read_bounded_text(&entry_path.join("prunable")) {
            record.prunable_reason = Some(if reason.trim().is_empty() {
                "git-prunable-marker".into()
            } else {
                reason.trim().to_string()
            });
        }
        if record.path.as_os_str().is_empty() {
            record.path = PathBuf::from(format!("<worktree-admin:{entry_name}>"));
        }
        records.push(record);
    }
    Ok(records)
}

fn build_audit(
    repository: &Path,
    generated_at_ms: u64,
    raw: Vec<RawWorktree>,
    mut notices: Vec<String>,
    evidence_complete: bool,
) -> WorktreeAudit {
    let mut stale_count = 0usize;
    let mut metadata_prune_eligible_count = 0usize;
    let worktrees: Vec<WorktreeCandidate> = raw
        .into_iter()
        .enumerate()
        .map(|(index, record)| {
            let exists = record.path.is_dir();
            let stale = record.prunable_reason.is_some() || !exists;
            let metadata_prune_eligible = stale && record.locked_reason.is_none();
            let mut review_reasons = Vec::new();
            if stale {
                stale_count += 1;
                if record.prunable_reason.is_some() {
                    review_reasons.push("git-registration-prunable".to_string());
                }
                if !exists {
                    review_reasons.push("worktree-path-missing".to_string());
                }
            } else {
                review_reasons.push("worktree-registration-present".to_string());
            }
            if record.locked_reason.is_some() {
                review_reasons.push("worktree-locked".to_string());
            }
            if metadata_prune_eligible {
                metadata_prune_eligible_count += 1;
            }
            WorktreeCandidate {
                path: record.path.to_string_lossy().into_owned(),
                head: record.head,
                branch: record.branch,
                is_primary: index == 0,
                detached: record.detached,
                exists,
                locked_reason: record.locked_reason,
                prunable_reason: record.prunable_reason,
                metadata_prune_eligible,
                review_reasons,
            }
        })
        .collect();
    notices.extend([
        "git-worktree-remove-not-invoked".into(),
        "git-worktree-prune-not-invoked".into(),
        "metadata-prune-requires-explicit-review".into(),
        "registration-fingerprint-required-for-prune".into(),
    ]);
    WorktreeAudit {
        repository: repository.to_string_lossy().into_owned(),
        generated_at_ms,
        registration_fingerprint: registration_fingerprint(repository, &worktrees),
        evidence_complete,
        worktrees,
        stale_count,
        metadata_prune_eligible_count,
        notices,
    }
}

pub fn system_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

/// Build a bounded, read-only audit for one repository.
pub fn audit(repository: &Path, generated_at_ms: u64) -> Result<WorktreeAudit, String> {
    if !repository.is_dir() {
        return Err(format!(
            "저장소 경로가 디렉터리가 아닙니다: {}",
            repository.display()
        ));
    }
    let repository = repository
        .canonicalize()
        .map_err(|error| format!("저장소 경로를 확인할 수 없습니다: {error}"))?;
    match run_git_worktree_list(&repository) {
        Ok(output) => Ok(build_audit(
            &repository,
            generated_at_ms,
            parse_worktree_porcelain(&output),
            vec!["read-only-git-worktree-list".into()],
            true,
        )),
        Err(error) if error == "git-worktree-list-timeout" => {
            let raw = raw_from_git_admin(&repository)?;
            Ok(build_audit(
                &repository,
                generated_at_ms,
                raw,
                vec![
                    "read-only-git-admin-fallback".into(),
                    "git-worktree-list-timeout".into(),
                ],
                false,
            ))
        }
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_prunable_and_locked_records() {
        let records = parse_worktree_porcelain(
            "worktree /repo\nHEAD abc\nbranch refs/heads/main\n\nworktree /gone\nHEAD def\ndetached\nprunable gitdir file points to non-existent location\n\nworktree /locked\nHEAD ghi\nlocked maintainer\n",
        );
        assert_eq!(records.len(), 3);
        assert_eq!(records[0].branch.as_deref(), Some("refs/heads/main"));
        assert!(records[1].detached);
        assert!(records[1].prunable_reason.is_some());
        assert_eq!(records[2].locked_reason.as_deref(), Some("maintainer"));
    }

    #[test]
    fn empty_blocks_are_ignored() {
        assert!(parse_worktree_porcelain("\n\n").is_empty());
    }

    #[test]
    fn parses_symbolic_and_detached_head_contents() {
        let (head, branch, detached) = parse_head_content("ref: refs/heads/main\n");
        assert_eq!(head, "ref: refs/heads/main");
        assert_eq!(branch.as_deref(), Some("refs/heads/main"));
        assert!(!detached);

        let (head, branch, detached) = parse_head_content("abc123\n");
        assert_eq!(head, "abc123");
        assert_eq!(branch, None);
        assert!(detached);
    }

    fn candidate(path: &str, head: &str) -> WorktreeCandidate {
        WorktreeCandidate {
            path: path.into(),
            head: head.into(),
            branch: Some("refs/heads/topic".into()),
            is_primary: false,
            detached: false,
            exists: false,
            locked_reason: None,
            prunable_reason: Some("missing gitdir".into()),
            metadata_prune_eligible: true,
            review_reasons: vec!["git-registration-prunable".into()],
        }
    }

    #[test]
    fn registration_fingerprint_binds_repository_and_worktree_state() {
        let worktrees = vec![candidate("/gone", "abc")];
        let first = registration_fingerprint(Path::new("/repo"), &worktrees);
        assert_eq!(
            first,
            registration_fingerprint(Path::new("/repo"), &worktrees)
        );

        let mut changed = worktrees.clone();
        changed[0].head = "def".into();
        assert_ne!(
            first,
            registration_fingerprint(Path::new("/repo"), &changed)
        );
        assert_ne!(
            first,
            registration_fingerprint(Path::new("/other"), &worktrees)
        );
    }
}
