//! Stable protection reason codes for Orca/dev reclaim decisions.
//!
//! These predicates are intentionally pure and fixture-friendly. Callers supply
//! live evidence (Orca terminal paths, open-PR OIDs, recent-write window) rather
//! than inventing silent defaults. When a recent-write window is used, the caller
//! must pass an explicit duration — there is no hidden library default.

use std::cell::Cell;
use std::collections::BTreeSet;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
#[cfg(windows)]
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Live Orca terminal (or agent) has this worktree bound.
pub const REASON_ORCA_TERMINAL_LIVE: &str = "orca-terminal-live";
/// A process CWD (or recursive open handle) is inside the path.
pub const REASON_PROCESS_CWD_INSIDE: &str = "process-cwd-inside";
/// Worktree directory name is `orchestration-lead-*`.
pub const REASON_ORCHESTRATION_LEAD: &str = "orchestration-lead-worktree";
/// Path or basename is listed in a `LEAD_QUEUE.md`.
pub const REASON_LISTED_IN_LEAD_QUEUE: &str = "listed-in-lead-queue";
/// Branch/OID is the head of an open pull request.
pub const REASON_OPEN_PR_HEAD: &str = "open-pr-head";
/// Tracked files are dirty.
pub const REASON_UNCOMMITTED_CHANGES: &str = "uncommitted-changes";
/// Untracked, non-ignored files are present.
pub const REASON_UNTRACKED_NONIGNORED: &str = "untracked-nonignored";
/// A git stash entry exists.
pub const REASON_STASH_PRESENT: &str = "stash-present";
/// HEAD is not contained in any remote-tracking branch.
pub const REASON_COMMITS_NOT_ON_REMOTE: &str = "commits-not-on-any-remote-branch";
/// A path inside the candidate tree was written inside the caller-supplied recent-write window.
pub const REASON_RECENT_WRITES: &str = "recent-writes-within-window";
/// Recent-write evidence could not be completed inside the bounded no-follow scan.
pub const REASON_RECENT_WRITE_EVIDENCE_INCOMPLETE: &str = "recent-write-evidence-incomplete";
/// Protected analytical/local data directory.
pub const REASON_PROTECTED_DATA_LOCAL: &str = "protected-data-path:local";
/// Protected results directory.
pub const REASON_PROTECTED_DATA_RESULTS: &str = "protected-data-path:results";
/// Credentials / `.env` style secrets path.
pub const REASON_PROTECTED_CREDENTIALS: &str = "protected-credentials-path";
/// Editable install (`pip -e` / maturin) points into this build tree.
pub const REASON_EDITABLE_INSTALL: &str = "editable-install-target";
/// Build/test toolchain process is actively using the path.
pub const REASON_BUILD_TOOL_ACTIVE: &str = "build-tool-process-active";

/// Directory basenames that must never be treated as reclaimable artifacts.
pub const PROTECTED_DATA_DIR_NAMES: &[&str] = &["local", "results"];

/// Credential / secret file basenames that force protection.
pub const PROTECTED_CREDENTIAL_FILE_NAMES: &[&str] =
    &[".env", ".env.local", "credentials.json", "credentials"];

const RECENT_WRITE_SCAN_BUDGET: Duration = Duration::from_millis(250);
const MAX_RECENT_WRITE_SCAN_ENTRIES: usize = 250_000;
#[cfg(windows)]
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtectionContext {
    /// Absolute worktree paths with a live Orca terminal binding.
    pub orca_live_worktree_paths: Vec<PathBuf>,
    /// Absolute paths mentioned in any scanned `LEAD_QUEUE.md`.
    pub lead_queue_worktree_paths: Vec<PathBuf>,
    /// Open PR head commit OIDs (and optionally branch names via separate field).
    pub open_pr_head_oids: Vec<String>,
    pub open_pr_head_branches: Vec<String>,
    /// When `Some`, protect paths whose mtime is newer than `now_secs - window`.
    /// `None` skips the check — callers must not invent a silent default.
    pub recent_write_window_secs: Option<u64>,
    /// Unix epoch seconds used for recent-write comparisons.
    pub now_unix_secs: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtectionAssessment {
    pub protected: bool,
    pub reason_codes: Vec<String>,
}

impl ProtectionAssessment {
    pub fn clear() -> Self {
        Self {
            protected: false,
            reason_codes: Vec::new(),
        }
    }

    pub fn with_reasons(mut reasons: Vec<String>) -> Self {
        reasons.sort();
        reasons.dedup();
        Self {
            protected: !reasons.is_empty(),
            reason_codes: reasons,
        }
    }
}

pub fn is_orchestration_lead_name(name: &str) -> bool {
    name.starts_with("orchestration-lead-")
}

pub fn is_protected_data_dir_name(name: &str) -> bool {
    PROTECTED_DATA_DIR_NAMES
        .iter()
        .any(|protected| *protected == name)
}

pub fn is_protected_credential_file_name(name: &str) -> bool {
    PROTECTED_CREDENTIAL_FILE_NAMES
        .iter()
        .any(|protected| *protected == name)
}

/// Parse `orca terminal list --json` into absolute worktree paths.
///
/// Accepts a bare array, `{terminals:[...]}`, or an Orca `{result:{...}}` wrapper.
pub fn parse_orca_terminal_worktree_paths(json_bytes: &[u8]) -> Result<Vec<PathBuf>, String> {
    let value: serde_json::Value =
        serde_json::from_slice(json_bytes).map_err(|_| "orca-terminal-json-invalid".to_string())?;
    let terminals = extract_terminal_array(&value).ok_or_else(|| "orca-terminal-json-shape".to_string())?;
    let mut paths = BTreeSet::new();
    for terminal in terminals {
        let Some(object) = terminal.as_object() else {
            continue;
        };
        for key in ["worktreePath", "cwd", "workingDirectory"] {
            if let Some(path) = object.get(key).and_then(|value| value.as_str()) {
                if !path.is_empty() {
                    paths.insert(PathBuf::from(path));
                }
            }
        }
    }
    Ok(paths.into_iter().collect())
}

fn extract_terminal_array(value: &serde_json::Value) -> Option<&Vec<serde_json::Value>> {
    if let Some(array) = value.as_array() {
        return Some(array);
    }
    let object = value.as_object()?;
    if let Some(array) = object.get("terminals").and_then(|value| value.as_array()) {
        return Some(array);
    }
    if let Some(array) = object.get("items").and_then(|value| value.as_array()) {
        return Some(array);
    }
    if let Some(result) = object.get("result") {
        return extract_terminal_array(result);
    }
    None
}

/// Extract absolute worktree paths mentioned in a LEAD_QUEUE.md body.
pub fn lead_queue_mentioned_paths(queue_text: &str, workspace_root: &Path) -> Vec<PathBuf> {
    let mut paths = BTreeSet::new();
    for token in queue_text.split_whitespace() {
        let cleaned = token.trim_matches(|ch: char| {
            matches!(ch, '`' | '"' | '\'' | ')' | '(' | '[' | ']' | ',' | '.')
        });
        if let Some(rel) = cleaned.strip_prefix("~/orca/workspaces/") {
            paths.insert(workspace_root.join(rel));
        } else if let Some(rel) = cleaned.strip_prefix("orca/workspaces/") {
            if let Some(home) = workspace_root.parent().and_then(|p| p.parent()) {
                paths.insert(home.join("orca/workspaces").join(rel));
            }
            paths.insert(workspace_root.join(rel));
        } else if cleaned.starts_with('/') && cleaned.contains("/orca/workspaces/") {
            paths.insert(PathBuf::from(cleaned));
        }
    }
    // Exact basename mentions of orchestration-lead-* only (avoid broad false positives).
    for line in queue_text.lines() {
        for word in line.split_whitespace() {
            let cleaned = word.trim_matches(|ch: char| {
                matches!(ch, '`' | '"' | '\'' | ')' | '(' | '[' | ']' | ',' | '.')
            });
            if is_orchestration_lead_name(cleaned) {
                paths.insert(workspace_root.join(cleaned));
            }
        }
    }
    paths.into_iter().collect()
}

pub fn path_is_under_any(candidate: &Path, roots: &[PathBuf]) -> bool {
    roots.iter().any(|root| {
        candidate == root.as_path()
            || candidate.starts_with(root)
            || root.starts_with(candidate)
    })
}

fn modified_time_reason(
    metadata: &fs::Metadata,
    window_secs: u64,
    now_unix_secs: u64,
) -> Result<Option<&'static str>, ()> {
    let modified = metadata.modified().map_err(|_| ())?;
    let mtime_secs = modified.duration_since(UNIX_EPOCH).map_err(|_| ())?.as_secs();
    Ok((now_unix_secs.saturating_sub(mtime_secs) < window_secs).then_some(REASON_RECENT_WRITES))
}

fn root_boundary_incomplete(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        return metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0;
    }
    #[cfg(not(windows))]
    {
        let _ = metadata;
        false
    }
}

fn crosses_filesystem_boundary(root: &fs::Metadata, descendant: &fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        return root.dev() != descendant.dev();
    }
    #[cfg(windows)]
    {
        let _ = root;
        return descendant.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0;
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (root, descendant);
        false
    }
}

fn recent_write_reason_with_limits(
    path: &Path,
    window_secs: u64,
    now_unix_secs: u64,
    deadline: Instant,
    max_entries: usize,
) -> Option<&'static str> {
    if window_secs == 0 {
        return None;
    }
    let root_metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(_) => return Some(REASON_RECENT_WRITE_EVIDENCE_INCOMPLETE),
    };
    if root_metadata.file_type().is_symlink() || root_boundary_incomplete(&root_metadata) {
        return Some(REASON_RECENT_WRITE_EVIDENCE_INCOMPLETE);
    }
    match modified_time_reason(&root_metadata, window_secs, now_unix_secs) {
        Ok(Some(reason)) => return Some(reason),
        Ok(None) => {}
        Err(()) => return Some(REASON_RECENT_WRITE_EVIDENCE_INCOMPLETE),
    }
    if !root_metadata.is_dir() {
        return None;
    }

    let boundary_incomplete = Cell::new(false);
    let mut visited = 0usize;
    let walker = walkdir::WalkDir::new(path)
        .min_depth(1)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            if entry.depth() == 0 || entry.file_type().is_symlink() {
                return true;
            }
            if Instant::now() >= deadline {
                boundary_incomplete.set(true);
                return false;
            }
            let metadata = match fs::symlink_metadata(entry.path()) {
                Ok(metadata) => metadata,
                Err(_) => {
                    boundary_incomplete.set(true);
                    return false;
                }
            };
            if crosses_filesystem_boundary(&root_metadata, &metadata) {
                boundary_incomplete.set(true);
                return false;
            }
            true
        });

    for entry in walker {
        if boundary_incomplete.get() || Instant::now() >= deadline || visited >= max_entries {
            return Some(REASON_RECENT_WRITE_EVIDENCE_INCOMPLETE);
        }
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => return Some(REASON_RECENT_WRITE_EVIDENCE_INCOMPLETE),
        };
        visited = visited.saturating_add(1);
        if entry.file_type().is_symlink() {
            continue;
        }
        let metadata = match fs::symlink_metadata(entry.path()) {
            Ok(metadata) => metadata,
            Err(_) => return Some(REASON_RECENT_WRITE_EVIDENCE_INCOMPLETE),
        };
        if crosses_filesystem_boundary(&root_metadata, &metadata) {
            return Some(REASON_RECENT_WRITE_EVIDENCE_INCOMPLETE);
        }
        match modified_time_reason(&metadata, window_secs, now_unix_secs) {
            Ok(Some(reason)) => return Some(reason),
            Ok(None) => {}
            Err(()) => return Some(REASON_RECENT_WRITE_EVIDENCE_INCOMPLETE),
        }
    }
    boundary_incomplete
        .get()
        .then_some(REASON_RECENT_WRITE_EVIDENCE_INCOMPLETE)
}

pub fn recent_write_reason(
    path: &Path,
    window_secs: u64,
    now_unix_secs: u64,
) -> Option<&'static str> {
    let deadline = Instant::now() + RECENT_WRITE_SCAN_BUDGET;
    recent_write_reason_with_limits(
        path,
        window_secs,
        now_unix_secs,
        deadline,
        MAX_RECENT_WRITE_SCAN_ENTRIES,
    )
}

pub fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

/// Detect editable installs under `venv` / `.venv` that point into `worktree`.
pub fn editable_install_reasons(worktree: &Path) -> Vec<String> {
    let mut reasons = Vec::new();
    for venv_name in [".venv", "venv"] {
        let venv = worktree.join(venv_name);
        if !venv.is_dir() {
            continue;
        }
        if editable_markers_point_into(&venv, worktree) {
            reasons.push(REASON_EDITABLE_INSTALL.to_string());
            break;
        }
    }
    reasons
}

fn editable_markers_point_into(venv: &Path, worktree: &Path) -> bool {
    let mut stack = vec![venv.to_path_buf()];
    let mut visited = 0usize;
    while let Some(dir) = stack.pop() {
        visited += 1;
        if visited > 4_096 {
            break;
        }
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if path.is_dir() {
                if name == "site-packages"
                    || name.starts_with("python")
                    || name == "lib"
                    || name.ends_with(".dist-info")
                    || name.ends_with(".egg-link")
                {
                    stack.push(path);
                }
                continue;
            }
            if name == "direct_url.json" {
                if let Ok(text) = fs::read_to_string(&path) {
                    if text.contains("file://")
                        && text.contains(&worktree.to_string_lossy().to_string())
                    {
                        return true;
                    }
                    if text.contains("\"dir_info\"") && text.contains("\"editable\": true") {
                        // Editable marker without absolute path still protects sibling target/python.
                        return true;
                    }
                }
            }
            if name.ends_with(".pth") || name.ends_with(".egg-link") {
                if let Ok(text) = fs::read_to_string(&path) {
                    if text.contains(&worktree.to_string_lossy().to_string())
                        || text.contains("/target/")
                        || text.contains("/python/")
                    {
                        return true;
                    }
                }
            }
        }
    }
    false
}

pub fn protected_data_reasons_under(worktree: &Path) -> Vec<String> {
    let mut reasons = Vec::new();
    for name in PROTECTED_DATA_DIR_NAMES {
        let path = worktree.join(name);
        if path.is_dir() {
            match *name {
                "local" => reasons.push(REASON_PROTECTED_DATA_LOCAL.to_string()),
                "results" => reasons.push(REASON_PROTECTED_DATA_RESULTS.to_string()),
                _ => {}
            }
        }
    }
    for name in PROTECTED_CREDENTIAL_FILE_NAMES {
        if worktree.join(name).exists() {
            reasons.push(REASON_PROTECTED_CREDENTIALS.to_string());
            break;
        }
    }
    reasons
}

/// Assess worktree-level protections from supplied evidence (no subprocesses).
pub fn assess_worktree_protections(
    worktree: &Path,
    head_oid: Option<&str>,
    branch: Option<&str>,
    context: &ProtectionContext,
    process_cwd_inside: bool,
    build_tool_active: bool,
    status_dirty: Option<(bool, bool)>,
    stash_present: bool,
    commits_not_on_remote: Option<bool>,
    assess_filesystem_protections: bool,
) -> ProtectionAssessment {
    let mut reasons = Vec::new();
    let name = worktree
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_default();

    if path_is_under_any(worktree, &context.orca_live_worktree_paths) {
        reasons.push(REASON_ORCA_TERMINAL_LIVE.to_string());
    }
    if process_cwd_inside {
        reasons.push(REASON_PROCESS_CWD_INSIDE.to_string());
    }
    if build_tool_active {
        reasons.push(REASON_BUILD_TOOL_ACTIVE.to_string());
    }
    if is_orchestration_lead_name(&name) {
        reasons.push(REASON_ORCHESTRATION_LEAD.to_string());
    }
    if path_is_under_any(worktree, &context.lead_queue_worktree_paths)
        || context
            .lead_queue_worktree_paths
            .iter()
            .any(|listed| listed.file_name() == worktree.file_name())
    {
        reasons.push(REASON_LISTED_IN_LEAD_QUEUE.to_string());
    }
    if let Some(oid) = head_oid {
        if context.open_pr_head_oids.iter().any(|value| value == oid) {
            reasons.push(REASON_OPEN_PR_HEAD.to_string());
        }
    }
    if let Some(branch) = branch {
        if context
            .open_pr_head_branches
            .iter()
            .any(|value| value == branch)
        {
            if !reasons.iter().any(|code| code == REASON_OPEN_PR_HEAD) {
                reasons.push(REASON_OPEN_PR_HEAD.to_string());
            }
        }
    }
    if let Some((uncommitted, untracked)) = status_dirty {
        if uncommitted {
            reasons.push(REASON_UNCOMMITTED_CHANGES.to_string());
        }
        if untracked {
            reasons.push(REASON_UNTRACKED_NONIGNORED.to_string());
        }
    }
    if stash_present {
        reasons.push(REASON_STASH_PRESENT.to_string());
    }
    if commits_not_on_remote == Some(true) {
        reasons.push(REASON_COMMITS_NOT_ON_REMOTE.to_string());
    }
    if let Some(window) = context.recent_write_window_secs {
        let now = context.now_unix_secs.unwrap_or_else(now_unix_secs);
        if let Some(code) = recent_write_reason(worktree, window, now) {
            reasons.push(code.to_string());
        }
    }
    if assess_filesystem_protections {
        reasons.extend(protected_data_reasons_under(worktree));
        reasons.extend(editable_install_reasons(worktree));
    }
    ProtectionAssessment::with_reasons(reasons)
}

/// Artifact reclaim protections (rebuildable outputs). Dirty/open-PR alone do not
/// block artifact reclaim; live activity, lead trees, editable installs, and optional
/// recent-write windows do.
pub fn artifact_blocking_reason_codes(assessment: &ProtectionAssessment) -> Vec<String> {
    assessment
        .reason_codes
        .iter()
        .filter(|code| {
            matches!(
                code.as_str(),
                REASON_ORCA_TERMINAL_LIVE
                    | REASON_PROCESS_CWD_INSIDE
                    | REASON_ORCHESTRATION_LEAD
                    | REASON_LISTED_IN_LEAD_QUEUE
                    | REASON_EDITABLE_INSTALL
                    | REASON_RECENT_WRITES
                    | REASON_RECENT_WRITE_EVIDENCE_INCOMPLETE
                    | REASON_BUILD_TOOL_ACTIVE
            )
        })
        .cloned()
        .collect()
}

/// Whole-worktree removal blockers include dirty/PR/unpushed/data protections.
pub fn whole_worktree_blocking_reason_codes(assessment: &ProtectionAssessment) -> Vec<String> {
    assessment.reason_codes.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn orca_terminal_json_extracts_worktree_paths() {
        let json = br#"{"result":{"terminals":[{"worktreePath":"/tmp/a/wt1","title":"x"},{"cwd":"/tmp/a/wt2"}]}}"#;
        let paths = parse_orca_terminal_worktree_paths(json).expect("parse");
        assert_eq!(
            paths,
            vec![PathBuf::from("/tmp/a/wt1"), PathBuf::from("/tmp/a/wt2")]
        );
    }

    #[test]
    fn orchestration_lead_and_protected_names() {
        assert!(is_orchestration_lead_name("orchestration-lead-fmls"));
        assert!(!is_orchestration_lead_name("disk-cleanup-lead"));
        assert!(is_protected_data_dir_name("local"));
        assert!(is_protected_credential_file_name(".env"));
    }

    #[test]
    fn lead_queue_mentions_absolute_and_lead_names() {
        let root = PathBuf::from("/Users/me/orca/workspaces");
        let text = "see `orchestration-lead-fmls` and /Users/me/orca/workspaces/disksage/disk-cleanup-lead\n";
        let paths = lead_queue_mentioned_paths(text, &root);
        assert!(paths.iter().any(|path| path.ends_with("orchestration-lead-fmls")));
        assert!(paths
            .iter()
            .any(|path| path.ends_with("disksage/disk-cleanup-lead")));
    }

    #[test]
    fn recent_write_requires_explicit_window_and_detects_fresh_mtime() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("fresh");
        let mut handle = fs::File::create(&file).unwrap();
        handle.write_all(b"x").unwrap();
        drop(handle);
        let now = now_unix_secs();
        assert_eq!(
            recent_write_reason(dir.path(), 3_600, now),
            Some(REASON_RECENT_WRITES)
        );
        assert_eq!(recent_write_reason(dir.path(), 0, now), None);
    }

    #[test]
    fn recent_write_scan_fails_closed_when_root_metadata_is_missing() {
        let root = tempfile::tempdir().unwrap();
        let missing = root.path().join("missing");
        assert_eq!(
            recent_write_reason(&missing, 3_600, now_unix_secs()),
            Some(REASON_RECENT_WRITE_EVIDENCE_INCOMPLETE)
        );
    }

    #[test]
    fn recent_write_scan_entry_budget_exhaustion_fails_closed() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("existing"), b"x").unwrap();
        let old_now = now_unix_secs().saturating_add(7_200);
        assert_eq!(
            recent_write_reason_with_limits(
                root.path(),
                3_600,
                old_now,
                Instant::now() + Duration::from_secs(1),
                0,
            ),
            Some(REASON_RECENT_WRITE_EVIDENCE_INCOMPLETE)
        );
    }

    #[test]
    fn editable_install_direct_url_marks_protection() {
        let root = tempfile::tempdir().unwrap();
        let worktree = root.path().join("proj");
        let site = worktree
            .join(".venv")
            .join("lib")
            .join("python3.12")
            .join("site-packages")
            .join("pkg.dist-info");
        fs::create_dir_all(&site).unwrap();
        fs::write(
            site.join("direct_url.json"),
            format!(
                "{{\"url\":\"file://{}\",\"dir_info\":{{\"editable\":true}}}}",
                worktree.display()
            ),
        )
        .unwrap();
        let reasons = editable_install_reasons(&worktree);
        assert_eq!(reasons, vec![REASON_EDITABLE_INSTALL.to_string()]);
    }

    #[test]
    fn assess_worktree_collects_stable_reason_codes_red_to_green() {
        let root = tempfile::tempdir().unwrap();
        let worktree = root.path().join("orchestration-lead-demo");
        fs::create_dir_all(worktree.join("local")).unwrap();
        fs::write(worktree.join(".env"), b"SECRET=1").unwrap();

        // RED: many protections fire.
        let context = ProtectionContext {
            orca_live_worktree_paths: vec![worktree.clone()],
            lead_queue_worktree_paths: vec![worktree.clone()],
            open_pr_head_oids: vec!["abc123".into()],
            open_pr_head_branches: vec!["feature".into()],
            recent_write_window_secs: Some(86_400),
            now_unix_secs: Some(now_unix_secs()),
        };
        let red = assess_worktree_protections(
            &worktree,
            Some("abc123"),
            Some("feature"),
            &context,
            true,
            false,
            Some((true, true)),
            true,
            Some(true),
            true,
        );
        assert!(red.protected);
        assert!(red.reason_codes.contains(&REASON_ORCA_TERMINAL_LIVE.to_string()));
        assert!(red.reason_codes.contains(&REASON_ORCHESTRATION_LEAD.to_string()));
        assert!(red.reason_codes.contains(&REASON_OPEN_PR_HEAD.to_string()));
        assert!(red.reason_codes.contains(&REASON_PROTECTED_DATA_LOCAL.to_string()));
        assert!(red
            .reason_codes
            .contains(&REASON_PROTECTED_CREDENTIALS.to_string()));
        assert!(!artifact_blocking_reason_codes(&red).is_empty());

        // GREEN: inactive ordinary worktree with no evidence.
        let quiet = root.path().join("idle-merged");
        fs::create_dir_all(&quiet).unwrap();
        let green = assess_worktree_protections(
            &quiet,
            Some("deadbeef"),
            Some("main"),
            &ProtectionContext::default(),
            false,
            false,
            Some((false, false)),
            false,
            Some(false),
            true,
        );
        assert!(!green.protected);
        assert!(artifact_blocking_reason_codes(&green).is_empty());
        assert!(whole_worktree_blocking_reason_codes(&green).is_empty());
    }
}
