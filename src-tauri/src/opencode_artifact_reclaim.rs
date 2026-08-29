//! Fail-closed reclamation of OpenCode tool-output files that no session part references.
//!
//! OpenCode stores authoritative references in `part.data` at
//! `$.state.metadata.outputPath`. DiskSage queries that native SQLite schema read-only, preserves
//! every referenced or unknown artifact, and moves only exact unreferenced regular files to the
//! OS Trash after fresh identity, database, and active-use revalidation.

use crate::cloud_local_eviction::observe_path_active_use;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs::{File, Metadata};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const SCHEMA_VERSION: u32 = 1;
const SQLITE_PATH: &str = "/usr/bin/sqlite3";
const QUERY_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_QUERY_BYTES: usize = 1_048_576;
const MAX_CANDIDATES: usize = 4_096;
const MAX_JOURNAL_BYTES: u64 = 1_048_576;
const VALID_JSON_MARKER: &str = "__DISKSAGE_PART_DATA_JSON_VALID__";
const INVALID_JSON_MARKER: &str = "__DISKSAGE_PART_DATA_JSON_INVALID__";
const REFERENCE_QUERY: &str = "SELECT CASE WHEN EXISTS(SELECT 1 FROM part WHERE NOT json_valid(data)) THEN '__DISKSAGE_PART_DATA_JSON_INVALID__' ELSE '__DISKSAGE_PART_DATA_JSON_VALID__' END; SELECT DISTINCT json_extract(data,'$.state.metadata.outputPath') FROM part WHERE json_valid(data) AND json_type(data,'$.state.metadata.outputPath')='text' ORDER BY 1;";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileIdentity {
    device: u64,
    inode: u64,
    logical_bytes: u64,
    allocated_bytes: u64,
    modified_ns: u128,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Candidate {
    path: PathBuf,
    identity: FileIdentity,
    sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenCodeArtifactPlan {
    pub schema_version: u32,
    pub schema_kind: String,
    pub observed_at_ms: u64,
    pub database_identity_sha256: String,
    pub referenced_tool_output_count: u64,
    pub candidate_count: u64,
    pub candidate_logical_bytes: u64,
    pub candidate_allocated_bytes: u64,
    pub candidate_set_sha256: String,
    pub exact_approval_phrase: Option<String>,
    pub evidence_complete: bool,
    pub issue: Option<String>,
    #[serde(skip)]
    candidates: Vec<Candidate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenCodeArtifactApproval {
    pub schema_version: u32,
    pub plan_fingerprint: String,
    pub exact_approval_phrase: String,
    pub approved_by: String,
    pub rationale: String,
    pub approved_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenCodeArtifactReceipt {
    pub schema_version: u32,
    pub plan_fingerprint: String,
    pub attempted_count: u64,
    pub trashed_count: u64,
    pub logical_bytes_moved: u64,
    pub allocated_bytes_moved: u64,
    pub before_available_bytes: Option<u64>,
    pub after_available_bytes: Option<u64>,
    pub executed_at_ms: u64,
    pub outcome: String,
}

#[derive(Deserialize)]
struct TrashJournalLine {
    op: String,
    path: String,
    outcome: String,
}

#[cfg(unix)]
fn identity(metadata: &Metadata) -> Result<FileIdentity, String> {
    use std::os::unix::fs::MetadataExt;
    let modified_ns = metadata
        .modified()
        .map_err(|_| "opencode-artifact-modified-time-unavailable".to_string())?
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "opencode-artifact-modified-time-invalid".to_string())?
        .as_nanos();
    Ok(FileIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
        logical_bytes: metadata.len(),
        allocated_bytes: metadata.blocks().saturating_mul(512),
        modified_ns,
    })
}

#[cfg(not(unix))]
fn identity(_metadata: &Metadata) -> Result<FileIdentity, String> {
    Err("opencode-artifact-reclaim-unix-only".into())
}

fn regular_identity(path: &Path) -> Result<FileIdentity, String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| "opencode-artifact-metadata-unavailable".to_string())?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.nlink() != 1 {
        return Err("opencode-artifact-not-exclusive-regular-file".into());
    }
    identity(&metadata)
}

#[cfg(unix)]
trait LinkCount {
    fn nlink(&self) -> u64;
}

#[cfg(unix)]
impl LinkCount for Metadata {
    fn nlink(&self) -> u64 {
        std::os::unix::fs::MetadataExt::nlink(self)
    }
}

#[cfg(not(unix))]
trait LinkCount {
    fn nlink(&self) -> u64;
}

#[cfg(not(unix))]
impl LinkCount for Metadata {
    fn nlink(&self) -> u64 {
        0
    }
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|_| "opencode-artifact-open-failed".to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65_536];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|_| "opencode-artifact-read-failed".to_string())?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(lower_hex(&hasher.finalize()))
}

fn lower_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn root_for(home: &Path) -> Result<PathBuf, String> {
    if !home.is_absolute()
        || home
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err("opencode-home-invalid".into());
    }
    let canonical_home =
        std::fs::canonicalize(home).map_err(|_| "opencode-home-unavailable".to_string())?;
    let current_home = current_user_home()?;
    if canonical_home != current_home {
        return Err("opencode-home-not-current-user".into());
    }
    let root = canonical_home.join(".local/share/opencode");
    let metadata = std::fs::symlink_metadata(&root)
        .map_err(|_| "opencode-data-root-unavailable".to_string())?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("opencode-data-root-unsafe".into());
    }
    Ok(root)
}

#[cfg(unix)]
fn current_user_home() -> Result<PathBuf, String> {
    use std::ffi::CStr;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::MetadataExt;

    // SAFETY: `getpwuid` returns process-owned libc storage. We copy the home bytes before any
    // further libc lookup and reject null/non-absolute/non-current-owner results.
    let uid = unsafe { libc::getuid() };
    let record = unsafe { libc::getpwuid(uid) };
    if record.is_null() {
        return Err("opencode-current-user-home-unavailable".into());
    }
    let directory = unsafe { (*record).pw_dir };
    if directory.is_null() {
        return Err("opencode-current-user-home-unavailable".into());
    }
    let bytes = unsafe { CStr::from_ptr(directory) }.to_bytes().to_vec();
    let path = PathBuf::from(std::ffi::OsStr::from_bytes(&bytes));
    if !path.is_absolute() {
        return Err("opencode-current-user-home-invalid".into());
    }
    let canonical = std::fs::canonicalize(path)
        .map_err(|_| "opencode-current-user-home-unavailable".to_string())?;
    let metadata = std::fs::symlink_metadata(&canonical)
        .map_err(|_| "opencode-current-user-home-unavailable".to_string())?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() || metadata.uid() != uid {
        return Err("opencode-current-user-home-ownership-invalid".into());
    }
    Ok(canonical)
}

#[cfg(not(unix))]
fn current_user_home() -> Result<PathBuf, String> {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .ok_or_else(|| "opencode-current-user-home-unavailable".to_string())?;
    std::fs::canonicalize(home).map_err(|_| "opencode-current-user-home-unavailable".to_string())
}

fn sqlite_is_trusted() -> Result<(), String> {
    let metadata = std::fs::symlink_metadata(SQLITE_PATH)
        .map_err(|_| "opencode-sqlite-unavailable".to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.uid() != 0
            || metadata.permissions().mode() & 0o022 != 0
        {
            return Err("opencode-sqlite-untrusted".into());
        }
    }
    Ok(())
}

fn drain_bounded(mut stream: impl Read) -> std::io::Result<(Vec<u8>, bool)> {
    let mut output = Vec::new();
    let mut buffer = [0u8; 16_384];
    let mut truncated = false;
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Err(error) => return Err(error),
            Ok(count) => {
                if output.len().saturating_add(count) > MAX_QUERY_BYTES {
                    truncated = true;
                } else if !truncated {
                    output.extend_from_slice(&buffer[..count]);
                }
            }
        }
    }
    Ok((output, truncated))
}

fn referenced_paths(database: &Path) -> Result<BTreeSet<PathBuf>, String> {
    sqlite_is_trusted()?;
    let mut child = Command::new(SQLITE_PATH)
        .args(["-readonly"])
        .arg(database)
        .arg(REFERENCE_QUERY)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| "opencode-reference-query-spawn-failed".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "opencode-reference-query-output-missing".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "opencode-reference-query-error-missing".to_string())?;
    let output_reader = thread::spawn(move || drain_bounded(stdout));
    let error_reader = thread::spawn(move || drain_bounded(stderr));
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < QUERY_TIMEOUT => {
                thread::sleep(Duration::from_millis(25))
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("opencode-reference-query-timeout".into());
            }
            Err(_) => return Err("opencode-reference-query-wait-failed".into()),
        }
    };
    let (output, output_truncated) = output_reader
        .join()
        .map_err(|_| "opencode-reference-query-reader-failed".to_string())?
        .map_err(|_| "opencode-reference-query-reader-failed".to_string())?;
    let (error, error_truncated) = error_reader
        .join()
        .map_err(|_| "opencode-reference-query-reader-failed".to_string())?
        .map_err(|_| "opencode-reference-query-reader-failed".to_string())?;
    if !status.success() || output_truncated || error_truncated || !error.is_empty() {
        return Err("opencode-reference-query-incomplete".into());
    }
    let text =
        String::from_utf8(output).map_err(|_| "opencode-reference-query-not-utf8".to_string())?;
    parse_reference_query_output(&text)
}

fn parse_reference_query_output(text: &str) -> Result<BTreeSet<PathBuf>, String> {
    let mut lines = text.lines();
    match lines.next() {
        Some(VALID_JSON_MARKER) => {}
        Some(INVALID_JSON_MARKER) => return Err("opencode-reference-data-invalid-json".into()),
        _ => return Err("opencode-reference-query-protocol-invalid".into()),
    }
    Ok(lines
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect())
}

fn active_use_complete_and_idle(path: &Path) -> Result<(), String> {
    let evidence = observe_path_active_use(path);
    if !evidence.evidence_complete || evidence.results_truncated {
        return Err("opencode-active-use-evidence-incomplete".into());
    }
    if evidence.active {
        return Err("opencode-data-active".into());
    }
    Ok(())
}

fn database_identity(root: &Path) -> Result<String, String> {
    let mut hasher = Sha256::new();
    hasher.update(b"disksage.opencode.database.v1\0");
    for name in ["opencode.db", "opencode.db-wal"] {
        let path = root.join(name);
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                hasher.update(name.as_bytes());
                hasher.update(
                    serde_json::to_vec(&identity(&metadata)?)
                        .map_err(|_| "opencode-identity-encode-failed".to_string())?,
                );
            }
            Ok(_) => return Err("opencode-database-object-unsafe".into()),
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound && name.ends_with("-wal") =>
            {
                hasher.update(b"wal-absent")
            }
            Err(_) => return Err("opencode-database-unavailable".into()),
        }
    }
    Ok(lower_hex(&hasher.finalize()))
}

fn require_unchanged_database_identity(
    expected: &str,
    observed: &str,
    issue: &str,
) -> Result<(), String> {
    if expected == observed {
        Ok(())
    } else {
        Err(issue.into())
    }
}

fn candidate_fingerprint(
    database_identity: &str,
    candidates: &[Candidate],
) -> Result<String, String> {
    let mut hasher = Sha256::new();
    hasher.update(b"disksage.opencode.tool-output-orphans.v1\0");
    hasher.update(database_identity.as_bytes());
    for candidate in candidates {
        hasher.update(candidate.path.as_os_str().to_string_lossy().as_bytes());
        hasher.update(
            serde_json::to_vec(&candidate.identity)
                .map_err(|_| "opencode-identity-encode-failed".to_string())?,
        );
        hasher.update(candidate.sha256.as_bytes());
    }
    Ok(lower_hex(&hasher.finalize()))
}

/// Plan exact unreferenced OpenCode tool-output files without changing any OpenCode data.
pub fn plan(home: &Path, observed_at_ms: u64) -> Result<OpenCodeArtifactPlan, String> {
    let root = root_for(home)?;
    let database = root.join("opencode.db");
    let tool_output = root.join("tool-output");
    active_use_complete_and_idle(&database)?;
    let database_before = database_identity(&root)?;
    let references = referenced_paths(&database)?;
    let database_after = database_identity(&root)?;
    active_use_complete_and_idle(&database)?;
    require_unchanged_database_identity(
        &database_before,
        &database_after,
        "opencode-database-changed-during-plan",
    )?;
    let canonical_tool_output = std::fs::canonicalize(&tool_output)
        .map_err(|_| "opencode-tool-output-unavailable".to_string())?;
    if !canonical_tool_output.starts_with(&root) {
        return Err("opencode-tool-output-outside-root".into());
    }
    let mut candidates = Vec::new();
    for entry in std::fs::read_dir(&canonical_tool_output)
        .map_err(|_| "opencode-tool-output-list-failed".to_string())?
    {
        let entry = entry.map_err(|_| "opencode-tool-output-list-failed".to_string())?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "opencode-tool-output-name-not-utf8".to_string())?;
        if !name.starts_with("tool_") || !name[5..].bytes().all(|byte| byte.is_ascii_alphanumeric())
        {
            return Err("opencode-tool-output-name-invalid".into());
        }
        let path = entry.path();
        let canonical = std::fs::canonicalize(&path)
            .map_err(|_| "opencode-tool-output-canonicalization-failed".to_string())?;
        if canonical.parent() != Some(canonical_tool_output.as_path()) {
            return Err("opencode-tool-output-escape".into());
        }
        if references.contains(&canonical) {
            continue;
        }
        if candidates.len() >= MAX_CANDIDATES {
            return Err("opencode-candidate-count-exceeds-bound".into());
        }
        let before = regular_identity(&canonical)?;
        let digest = sha256_file(&canonical)?;
        let after = regular_identity(&canonical)?;
        if before != after {
            return Err("opencode-candidate-changed-during-plan".into());
        }
        active_use_complete_and_idle(&canonical)?;
        candidates.push(Candidate {
            path: canonical,
            identity: after,
            sha256: digest,
        });
    }
    candidates.sort_by(|left, right| left.path.cmp(&right.path));
    let logical = candidates.iter().try_fold(0u64, |sum, item| {
        sum.checked_add(item.identity.logical_bytes)
            .ok_or_else(|| "opencode-size-overflow".to_string())
    })?;
    let allocated = candidates.iter().try_fold(0u64, |sum, item| {
        sum.checked_add(item.identity.allocated_bytes)
            .ok_or_else(|| "opencode-size-overflow".to_string())
    })?;
    let database_final = database_identity(&root)?;
    active_use_complete_and_idle(&database)?;
    require_unchanged_database_identity(
        &database_after,
        &database_final,
        "opencode-database-changed-during-output-scan",
    )?;
    let fingerprint = candidate_fingerprint(&database_final, &candidates)?;
    Ok(OpenCodeArtifactPlan {
        schema_version: SCHEMA_VERSION,
        schema_kind: "disksage.opencode-tool-output-orphan-plan".into(),
        observed_at_ms,
        database_identity_sha256: database_final,
        referenced_tool_output_count: references.len() as u64,
        candidate_count: candidates.len() as u64,
        candidate_logical_bytes: logical,
        candidate_allocated_bytes: allocated,
        exact_approval_phrase: (!candidates.is_empty())
            .then(|| format!("DiskSage OpenCode orphan output 승인 {fingerprint}")),
        candidate_set_sha256: fingerprint,
        evidence_complete: true,
        issue: None,
        candidates,
    })
}

/// Re-plan and move only the exact still-unreferenced artifacts to OS Trash.
pub fn execute(
    home: &Path,
    approved_fingerprint: &str,
    confirmation: &str,
    approved_by: &str,
    rationale: &str,
    journal_path: &Path,
    record_directory: &Path,
    now_ms: u64,
) -> Result<OpenCodeArtifactReceipt, String> {
    // Path-based Trash APIs cannot atomically prove that the validated inode is the object moved,
    // and the legacy shared journal has no authenticated batch lineage. Keep the complete planner
    // available, but do not expose mutation until an OS-enforced identity-bound move plus durable
    // per-item outcome protocol exists.
    let _ = (
        home,
        approved_fingerprint,
        confirmation,
        approved_by,
        rationale,
        journal_path,
        record_directory,
        now_ms,
    );
    return Err("opencode-trash-execution-disabled".into());
    #[allow(unreachable_code)]
    if approved_by.trim().is_empty() || rationale.trim().is_empty() {
        return Err("opencode-approval-attribution-required".into());
    }
    let fresh = plan(home, now_ms)?;
    let expected = fresh
        .exact_approval_phrase
        .as_deref()
        .ok_or_else(|| "opencode-empty-candidate-set".to_string())?;
    if fresh.candidate_set_sha256 != approved_fingerprint || confirmation != expected {
        return Err("opencode-approval-mismatch".into());
    }
    let root = root_for(home)?;
    let approval = OpenCodeArtifactApproval {
        schema_version: SCHEMA_VERSION,
        plan_fingerprint: fresh.candidate_set_sha256.clone(),
        exact_approval_phrase: confirmation.to_string(),
        approved_by: approved_by.to_string(),
        rationale: rationale.to_string(),
        approved_at_ms: now_ms,
    };
    let approval_name = format!(
        "opencode-approval-{}-{now_ms}.json",
        &fresh.candidate_set_sha256[..16]
    );
    crate::private_evidence::write_private_json_create_new(
        &root,
        &record_directory.join(approval_name),
        &approval,
    )?;
    let before_available = crate::volume_pressure::snapshot_volume(&root, now_ms)
        .ok()
        .map(|item| item.available_bytes);
    let mut logical = 0u64;
    let mut allocated = 0u64;
    let mut trashed = 0u64;
    for candidate in &fresh.candidates {
        if database_identity(&root)? != fresh.database_identity_sha256 {
            return Err("opencode-database-changed-before-mutation".into());
        }
        active_use_complete_and_idle(&root.join("opencode.db"))?;
        active_use_complete_and_idle(&candidate.path)?;
        let before = regular_identity(&candidate.path)?;
        if before != candidate.identity || sha256_file(&candidate.path)? != candidate.sha256 {
            return Err("opencode-candidate-changed-before-mutation".into());
        }
        crate::safety::trash_delete(&candidate.path, before.logical_bytes, journal_path, now_ms)
            .map_err(|_| "opencode-trash-move-failed".to_string())?;
        logical = logical.saturating_add(before.logical_bytes);
        allocated = allocated.saturating_add(before.allocated_bytes);
        trashed = trashed.saturating_add(1);
    }
    let receipt = OpenCodeArtifactReceipt {
        schema_version: SCHEMA_VERSION,
        plan_fingerprint: fresh.candidate_set_sha256,
        attempted_count: fresh.candidate_count,
        trashed_count: trashed,
        logical_bytes_moved: logical,
        allocated_bytes_moved: allocated,
        before_available_bytes: before_available,
        after_available_bytes: crate::volume_pressure::snapshot_volume(&root, now_ms.max(1))
            .ok()
            .map(|item| item.available_bytes),
        executed_at_ms: now_ms,
        outcome: "moved-to-os-trash".into(),
    };
    let result_name = format!(
        "opencode-result-{}-{now_ms}.json",
        &receipt.plan_fingerprint[..16]
    );
    crate::private_evidence::write_private_json_create_new(
        &root,
        &record_directory.join(result_name),
        &receipt,
    )?;
    Ok(receipt)
}

/// Permanently remove only DiskSage-journaled quarantine objects whose current identities
/// reconstruct the exact previously approved candidate fingerprint.
pub fn purge_quarantined(
    home: &Path,
    approved_fingerprint: &str,
    confirmation: &str,
    approved_by: &str,
    rationale: &str,
    journal_path: &Path,
    record_directory: &Path,
    now_ms: u64,
) -> Result<OpenCodeArtifactReceipt, String> {
    // A caller-selected append-only journal is not authentic quarantine authority. Permanent
    // deletion remains disabled until the Trash move emits a create-only, ownership-checked batch
    // manifest and each unlink has replacement-resistant identity plus durable before/after state.
    let _ = (
        home,
        approved_fingerprint,
        confirmation,
        approved_by,
        rationale,
        journal_path,
        record_directory,
        now_ms,
    );
    return Err("opencode-permanent-purge-disabled".into());
    #[allow(unreachable_code)]
    if approved_by.trim().is_empty() || rationale.trim().is_empty() {
        return Err("opencode-purge-approval-attribution-required".into());
    }
    let root = root_for(home)?;
    let database = root.join("opencode.db");
    active_use_complete_and_idle(&database)?;
    let approved_database_identity = database_identity(&root)?;
    let journal_metadata = std::fs::symlink_metadata(journal_path)
        .map_err(|_| "opencode-purge-journal-unavailable".to_string())?;
    if !journal_metadata.is_file()
        || journal_metadata.file_type().is_symlink()
        || journal_metadata.len() > MAX_JOURNAL_BYTES
    {
        return Err("opencode-purge-journal-unsafe".into());
    }
    let journal = std::fs::read_to_string(journal_path)
        .map_err(|_| "opencode-purge-journal-unreadable".to_string())?;
    let source_parent = root.join("tool-output");
    let trash_parent = std::fs::canonicalize(home.join(".Trash"))
        .map_err(|_| "opencode-trash-root-unavailable".to_string())?;
    let mut original_paths = BTreeSet::new();
    for line in journal.lines() {
        let entry: TrashJournalLine = serde_json::from_str(line)
            .map_err(|_| "opencode-purge-journal-invalid".to_string())?;
        if entry.op != "trash_delete" || entry.outcome != "ok" {
            continue;
        }
        let original = PathBuf::from(entry.path);
        if original.parent() == Some(source_parent.as_path()) {
            original_paths.insert(original);
        }
    }
    if original_paths.is_empty() || original_paths.len() > MAX_CANDIDATES {
        return Err("opencode-purge-journal-candidate-set-invalid".into());
    }
    let mut candidates = Vec::new();
    for original in original_paths {
        if original.exists() {
            return Err("opencode-purge-source-reappeared".into());
        }
        let name = original
            .file_name()
            .ok_or_else(|| "opencode-purge-name-invalid".to_string())?;
        let quarantined = std::fs::canonicalize(trash_parent.join(name))
            .map_err(|_| "opencode-quarantine-object-unavailable".to_string())?;
        if quarantined.parent() != Some(trash_parent.as_path()) {
            return Err("opencode-quarantine-object-escape".into());
        }
        let before = regular_identity(&quarantined)?;
        let sha256 = sha256_file(&quarantined)?;
        if regular_identity(&quarantined)? != before {
            return Err("opencode-quarantine-object-changed".into());
        }
        active_use_complete_and_idle(&quarantined)?;
        candidates.push(Candidate {
            path: original,
            identity: before,
            sha256,
        });
    }
    candidates.sort_by(|left, right| left.path.cmp(&right.path));
    let reconstructed = candidate_fingerprint(&approved_database_identity, &candidates)?;
    let expected = format!("DiskSage OpenCode quarantine 영구 삭제 승인 {reconstructed}");
    if reconstructed != approved_fingerprint || confirmation != expected {
        return Err("opencode-purge-approval-mismatch".into());
    }
    let approval = OpenCodeArtifactApproval {
        schema_version: SCHEMA_VERSION,
        plan_fingerprint: reconstructed.clone(),
        exact_approval_phrase: confirmation.to_string(),
        approved_by: approved_by.to_string(),
        rationale: rationale.to_string(),
        approved_at_ms: now_ms,
    };
    crate::private_evidence::write_private_json_create_new(
        &root,
        &record_directory.join(format!("opencode-purge-approval-{}-{now_ms}.json", &reconstructed[..16])),
        &approval,
    )?;
    let before_available = crate::volume_pressure::snapshot_volume(&root, now_ms)
        .ok()
        .map(|item| item.available_bytes);
    let mut logical = 0u64;
    let mut allocated = 0u64;
    let mut removed = 0u64;
    for candidate in &candidates {
        if database_identity(&root)? != approved_database_identity {
            return Err("opencode-database-changed-before-purge".into());
        }
        let name = candidate.path.file_name().ok_or("opencode-purge-name-invalid")?;
        let quarantined = std::fs::canonicalize(trash_parent.join(name))
            .map_err(|_| "opencode-quarantine-object-unavailable".to_string())?;
        active_use_complete_and_idle(&quarantined)?;
        let current = regular_identity(&quarantined)?;
        if current != candidate.identity || sha256_file(&quarantined)? != candidate.sha256 {
            return Err("opencode-quarantine-object-changed-before-purge".into());
        }
        std::fs::remove_file(&quarantined)
            .map_err(|_| "opencode-quarantine-purge-failed".to_string())?;
        logical = logical.saturating_add(current.logical_bytes);
        allocated = allocated.saturating_add(current.allocated_bytes);
        removed = removed.saturating_add(1);
    }
    let receipt = OpenCodeArtifactReceipt {
        schema_version: SCHEMA_VERSION,
        plan_fingerprint: reconstructed,
        attempted_count: candidates.len() as u64,
        trashed_count: removed,
        logical_bytes_moved: logical,
        allocated_bytes_moved: allocated,
        before_available_bytes: before_available,
        after_available_bytes: crate::volume_pressure::snapshot_volume(&root, now_ms.max(1))
            .ok()
            .map(|item| item.available_bytes),
        executed_at_ms: now_ms,
        outcome: "permanently-purged-exact-quarantine".into(),
    };
    crate::private_evidence::write_private_json_create_new(
        &root,
        &record_directory.join(format!("opencode-purge-result-{}-{now_ms}.json", &receipt.plan_fingerprint[..16])),
        &receipt,
    )?;
    Ok(receipt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_fingerprint_binds_database_file_identity_and_content() {
        let candidate = Candidate {
            path: PathBuf::from("/home/test/.local/share/opencode/tool-output/tool_abc"),
            identity: FileIdentity {
                device: 1,
                inode: 2,
                logical_bytes: 3,
                allocated_bytes: 4,
                modified_ns: 5,
            },
            sha256: "a".repeat(64),
        };
        assert_ne!(
            candidate_fingerprint("db-a", &[candidate.clone()]).unwrap(),
            candidate_fingerprint("db-b", &[candidate.clone()]).unwrap()
        );
        let original = candidate.clone();
        let mut changed = candidate;
        changed.sha256 = "b".repeat(64);
        assert_ne!(
            candidate_fingerprint("db-a", &[original]).unwrap(),
            candidate_fingerprint("db-a", &[changed]).unwrap()
        );
    }

    #[test]
    fn mutation_surfaces_remain_fail_closed_before_path_or_journal_use() {
        let missing = Path::new("/definitely/not/a/home");
        assert_eq!(
            execute(missing, "forged", "forged", "human:test", "test", missing, missing, 1)
                .unwrap_err(),
            "opencode-trash-execution-disabled"
        );
        assert_eq!(
            purge_quarantined(
                missing,
                "forged",
                "forged",
                "human:test",
                "test",
                missing,
                missing,
                1,
            )
            .unwrap_err(),
            "opencode-permanent-purge-disabled"
        );
    }

    #[test]
    fn malformed_history_row_marker_fails_closed_before_paths_are_parsed() {
        let output = format!("{INVALID_JSON_MARKER}\n/tool-output/tool_unsafe\n");
        assert_eq!(
            parse_reference_query_output(&output).unwrap_err(),
            "opencode-reference-data-invalid-json"
        );
    }

    #[test]
    fn reference_query_protocol_requires_validity_attestation() {
        assert_eq!(
            parse_reference_query_output("/tool-output/tool_unattested\n").unwrap_err(),
            "opencode-reference-query-protocol-invalid"
        );
        assert_eq!(
            parse_reference_query_output(&format!(
                "{VALID_JSON_MARKER}\n/tool-output/tool_a\n/tool-output/tool_b\n"
            ))
            .unwrap(),
            [
                PathBuf::from("/tool-output/tool_a"),
                PathBuf::from("/tool-output/tool_b"),
            ]
            .into_iter()
            .collect()
        );
    }

    #[test]
    fn reference_query_drain_preserves_stream_read_failure() {
        struct FailingReader {
            emitted: bool,
        }

        impl Read for FailingReader {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                if self.emitted {
                    return Err(std::io::Error::other("fixture-read-failure"));
                }
                self.emitted = true;
                let partial = VALID_JSON_MARKER.as_bytes();
                buffer[..partial.len()].copy_from_slice(partial);
                Ok(partial.len())
            }
        }

        assert!(drain_bounded(FailingReader { emitted: false }).is_err());
    }

    #[test]
    fn database_or_wal_change_during_candidate_scan_fails_closed() {
        let temporary = tempfile::tempdir().unwrap();
        std::fs::write(temporary.path().join("opencode.db"), b"before").unwrap();
        let before = database_identity(temporary.path()).unwrap();
        std::fs::write(temporary.path().join("opencode.db-wal"), b"new-reference").unwrap();
        let after = database_identity(temporary.path()).unwrap();
        assert_eq!(
            require_unchanged_database_identity(
                &before,
                &after,
                "opencode-database-changed-during-output-scan",
            )
            .unwrap_err(),
            "opencode-database-changed-during-output-scan"
        );
    }
}
