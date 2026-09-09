//! Bounded, ontology-backed macOS orphan planning.
//!
//! The planner inspects only bounded installed-app metadata plus directory/file metadata for
//! candidate Library trees. Public evidence is path-free; the absolute path is retained only in
//! the in-process plan so the explicit Trash operation can revalidate the exact candidate. An
//! advisory model can label a candidate, but deterministic eligibility and the human approval
//! phrase remain authoritative.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
#[cfg(all(target_os = "macos", not(test)))]
use std::process::{Command, Stdio};
#[cfg(target_os = "macos")]
use std::time::Instant;
#[cfg(all(target_os = "macos", not(test)))]
use std::time::Duration;

pub const ORPHAN_SCHEMA_VERSION: u32 = 1;
const ORPHAN_SCHEMA_KIND: &str = "disksage.orphan-plan/v1";
const ONTOLOGY_NAMESPACE: &str = "https://disksage.app/ontology#";
const PLAN_BUDGET_MS: u64 = 5_000;
const MAX_CANDIDATES: usize = 256;
const MAX_MANIFEST_RECORDS: usize = 100_000;
const MAX_BUNDLE_SCAN_DEPTH: usize = 3;
const MAX_MANIFEST_DEPTH: usize = 64;
const EXECUTION_MANIFEST_REVALIDATION_BUDGET_MS: u64 = 5_000;
const MAX_BUNDLE_METADATA_BYTES: u64 = 1_048_576;
#[cfg(all(target_os = "macos", not(test)))]
const MAX_LAUNCH_SERVICES_OUTPUT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrphanRelationEvidence {
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrphanCandidate {
    pub candidate_id: String,
    pub kind: String,
    pub bundle_id: Option<String>,
    pub bytes: u64,
    pub files: u64,
    pub skipped: u64,
    pub scan_complete: bool,
    pub object_id: String,
    pub metadata_fingerprint: String,
    pub ontology_class: String,
    pub confidence: String,
    pub active_use_evidence_complete: bool,
    pub active_use: bool,
    pub relations: Vec<OrphanRelationEvidence>,
    pub review_reasons: Vec<String>,
    pub auto_trash_eligible: bool,
    /// The path is an execution-only binding and is never serialized to the UI or Agent.
    #[serde(skip)]
    path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrphanPlan {
    pub schema_kind: String,
    pub schema_version: u32,
    pub generated_at_ms: u64,
    pub plan_fingerprint: String,
    pub candidate_count: usize,
    pub candidate_bytes: u64,
    pub scan_complete: bool,
    pub candidates: Vec<OrphanCandidate>,
    pub notices: Vec<String>,
    pub local_paths_included: bool,
    pub mutation_performed: bool,
    pub exact_approval_phrase: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrphanCleanupRequest {
    pub candidate_id: String,
    pub metadata_fingerprint: String,
    pub bytes: u64,
    pub files: u64,
    pub skipped: u64,
    pub scan_complete: bool,
    pub object_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrphanCleanupItemResult {
    pub candidate_id: String,
    pub bytes: u64,
    pub attempted: bool,
    pub moved_to_trash: bool,
    pub error: Option<String>,
    /// Post-move audit or staging notice. This is separate from a failed move.
    #[serde(default)]
    pub warning: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrphanCleanupResult {
    pub schema_kind: String,
    pub schema_version: u32,
    pub plan_fingerprint: String,
    pub requested_count: usize,
    pub moved_count: usize,
    pub filesystem_mutation_executed: bool,
    pub items: Vec<OrphanCleanupItemResult>,
    pub notices: Vec<String>,
}

#[derive(Default)]
struct Manifest {
    bytes: u64,
    files: u64,
    skipped: u64,
    complete: bool,
    records: Vec<String>,
}

#[cfg(target_os = "macos")]
fn planner_home_scope_is_safe(canonical_home: &Path) -> bool {
    if canonical_home.parent().is_none() {
        return false;
    }
    let library = canonical_home.join("Library");
    let cache_root = library.join("Caches");
    let support_root = library.join("Application Support");
    !crate::safety::is_protected(&cache_root) && !crate::safety::is_protected(&support_root)
}

#[cfg(target_os = "macos")]
/// Build a path-free orphan plan for the current user's Library.
pub fn plan(home: &Path, now_ms: u64) -> Result<OrphanPlan, String> {
    if !home.is_absolute()
        || home
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("orphan-home-path-invalid".into());
    }
    let supplied_home =
        std::fs::symlink_metadata(home).map_err(|_| "orphan-home-unavailable".to_string())?;
    if supplied_home.file_type().is_symlink() || !supplied_home.is_dir() {
        return Err("orphan-home-unsafe".into());
    }
    let canonical_home =
        std::fs::canonicalize(home).map_err(|_| "orphan-home-unavailable".to_string())?;
    let library = canonical_home.join("Library");
    if !canonical_home.is_dir() || !library.is_dir() || !planner_home_scope_is_safe(&canonical_home)
    {
        return Err("orphan-home-unsafe".into());
    }
    let watched = [
        (library.join("Caches"), "cache"),
        (library.join("Application Support"), "application-support"),
    ];
    let application_roots = [
        PathBuf::from("/Applications"),
        canonical_home.join("Applications"),
        PathBuf::from("/System/Applications"),
    ];
    plan_for_roots(&canonical_home, &watched, &application_roots, now_ms)
}

#[cfg(not(target_os = "macos"))]
pub fn plan(_home: &Path, _now_ms: u64) -> Result<OrphanPlan, String> {
    Err("orphan-plan-macos-only".into())
}

fn validate_cleanup_requests<'a>(
    plan: &'a OrphanPlan,
    requests: &[OrphanCleanupRequest],
) -> Result<Vec<&'a OrphanCandidate>, String> {
    if requests.len() > MAX_CANDIDATES {
        return Err("orphan-candidate-count-exceeded".into());
    }
    let mut seen = BTreeSet::new();
    let mut prepared = Vec::with_capacity(requests.len());
    for request in requests {
        if !seen.insert(request.candidate_id.clone()) {
            return Err("orphan-candidate-duplicate".into());
        }
        let candidate = plan
            .candidates
            .iter()
            .find(|candidate| candidate.candidate_id == request.candidate_id)
            .ok_or_else(|| "orphan-candidate-not-in-plan".to_string())?;
        if candidate.metadata_fingerprint != request.metadata_fingerprint
            || candidate.bytes != request.bytes
            || candidate.files != request.files
            || candidate.skipped != request.skipped
            || candidate.scan_complete != request.scan_complete
            || candidate.object_id != request.object_id
            || !candidate.auto_trash_eligible
            || candidate.kind != "cache"
            || !candidate.scan_complete
            || candidate.skipped != 0
            || candidate.active_use
            || !candidate.active_use_evidence_complete
        {
            return Err("orphan-candidate-safety-gate-blocked".into());
        }
        prepared.push(candidate);
    }
    Ok(prepared)
}

/// Move only deterministic, fully scanned cache candidates to the OS Trash.
///
/// The caller must have just rebuilt `plan`; this function still rechecks every request against
/// that plan and rejects Application Support, broken links, incomplete manifests, duplicate
/// requests, and any model-only approval. The complete request batch is validated before the first
/// filesystem mutation so a later stale request cannot create an unreported partial operation.
pub fn move_to_trash(
    plan: &OrphanPlan,
    requests: &[OrphanCleanupRequest],
    confirmation_phrase: &str,
    rationale: &str,
    journal_path: &Path,
    now_ms: u64,
) -> Result<OrphanCleanupResult, String> {
    if plan.schema_kind != ORPHAN_SCHEMA_KIND
        || plan.schema_version != ORPHAN_SCHEMA_VERSION
        || plan.local_paths_included
        || plan.mutation_performed
        || !plan.scan_complete
    {
        return Err("orphan-plan-not-authoritative".into());
    }
    if confirmation_phrase != plan.exact_approval_phrase {
        return Err("orphan-confirmation-mismatch".into());
    }
    if rationale.trim().is_empty()
        || rationale != rationale.trim()
        || rationale.chars().count() > 1_000
        || rationale.chars().any(char::is_control)
    {
        return Err("orphan-rationale-invalid".into());
    }
    let prepared = validate_cleanup_requests(plan, requests)?;
    #[cfg(target_os = "macos")]
    {
        let deadline = std::time::Instant::now()
            .checked_add(std::time::Duration::from_millis(
                EXECUTION_MANIFEST_REVALIDATION_BUDGET_MS,
            ))
            .ok_or_else(|| "orphan-manifest-revalidation-deadline-overflow".to_string())?;
        for candidate in &prepared {
            if !candidate_manifest_is_unchanged(candidate, deadline) {
                return Err("orphan-candidate-metadata-changed".into());
            }
        }
    }
    let mut items = Vec::with_capacity(prepared.len());
    let mut moved_count = 0usize;
    for candidate in prepared {
        let result = match crate::safety::trash_delete_if_identity_with_outcome(
            &candidate.path,
            &candidate.object_id,
            candidate.bytes,
            journal_path,
            now_ms,
        ) {
            Ok(outcome) if outcome.moved_to_trash => {
                moved_count += 1;
                OrphanCleanupItemResult {
                    candidate_id: candidate.candidate_id.clone(),
                    bytes: candidate.bytes,
                    attempted: true,
                    moved_to_trash: true,
                    error: None,
                    warning: crate::safety::trash_delete_outcome_warning(&outcome),
                }
            }
            Ok(_) => OrphanCleanupItemResult {
                candidate_id: candidate.candidate_id.clone(),
                bytes: candidate.bytes,
                attempted: true,
                moved_to_trash: false,
                error: Some("orphan-trash-operation-failed".into()),
                warning: None,
            },
            Err(_) => OrphanCleanupItemResult {
                candidate_id: candidate.candidate_id.clone(),
                bytes: candidate.bytes,
                attempted: true,
                moved_to_trash: false,
                error: Some("orphan-trash-operation-failed".into()),
                warning: None,
            },
        };
        items.push(result);
    }
    Ok(OrphanCleanupResult {
        schema_kind: "disksage.orphan-cleanup-result/v1".into(),
        schema_version: ORPHAN_SCHEMA_VERSION,
        plan_fingerprint: plan.plan_fingerprint.clone(),
        requested_count: requests.len(),
        moved_count,
        filesystem_mutation_executed: moved_count > 0,
        items,
        notices: vec![
            "os-trash-is-reversible-until-emptied".into(),
            "llm-output-is-not-cleanup-authority".into(),
            "rationale-is-audit-context-only".into(),
        ],
    })
}

#[cfg(target_os = "macos")]
fn candidate_manifest_is_unchanged(candidate: &OrphanCandidate, deadline: std::time::Instant) -> bool {
    // A disappeared path is left to the identity-bound trash boundary, which reports the
    // operation failure without exposing a local path. Existing directories must still match the
    // reviewed metadata manifest before any batch mutation begins.
    let Ok(metadata) = std::fs::symlink_metadata(&candidate.path) else {
        return true;
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return false;
    }
    let manifest = bounded_manifest(&candidate.path, deadline);
    if !manifest.complete
        || manifest.skipped != 0
        || manifest.bytes != candidate.bytes
        || manifest.files != candidate.files
    {
        return false;
    }
    let fingerprint = digest_values(
        &manifest
            .records
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
    );
    fingerprint == candidate.metadata_fingerprint
}

#[cfg(target_os = "macos")]
pub fn plan_for_roots(
    _home: &Path,
    watched: &[(PathBuf, &str)],
    application_roots: &[PathBuf],
    now_ms: u64,
) -> Result<OrphanPlan, String> {
    let deadline = std::time::Instant::now()
        .checked_add(std::time::Duration::from_millis(PLAN_BUDGET_MS))
        .ok_or_else(|| "orphan-plan-deadline-overflow".to_string())?;
    let (installed, installed_complete) = installed_bundle_ids(application_roots, deadline);
    let mut candidates = Vec::new();
    let mut scan_complete = installed_complete;
    let mut notices = vec![
        "metadata-only: Library candidate file contents are never read; installed app Info.plist metadata is bounded".into(),
        "application-support-is-review-only".into(),
        "containers-mobile-documents-mail-preferences-and-keychains-are-excluded".into(),
        "public-evidence-contains-no-local-paths".into(),
    ];
    for (root, kind) in watched {
        if std::time::Instant::now() >= deadline || candidates.len() >= MAX_CANDIDATES {
            scan_complete = false;
            notices.push("orphan-scan-budget-exhausted".into());
            break;
        }
        let dir_entries = match std::fs::read_dir(root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => {
                scan_complete = false;
                notices.push("orphan-root-read-failed".into());
                continue;
            }
        };
        let mut entries = Vec::new();
        for entry in dir_entries {
            match entry {
                Ok(entry) => entries.push(entry),
                Err(_) => {
                    scan_complete = false;
                    notices.push("orphan-root-entry-read-failed".into());
                }
            }
        }
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            if std::time::Instant::now() >= deadline || candidates.len() >= MAX_CANDIDATES {
                scan_complete = false;
                notices.push("orphan-scan-budget-exhausted".into());
                break;
            }
            let path = entry.path();
            let file_type = match entry.file_type() {
                Ok(file_type) => file_type,
                Err(_) => {
                    scan_complete = false;
                    continue;
                }
            };
            if file_type.is_symlink() || !file_type.is_dir() {
                continue;
            }
            let Some(bundle_id) = bundle_id_from_name(&path) else {
                continue;
            };
            if installed.contains(&bundle_id) {
                continue;
            }
            let manifest = bounded_manifest(&path, deadline);
            let active_use = crate::cloud_local_eviction::observe_path_active_use_until(&path, deadline);
            let candidate = directory_candidate(
                &path,
                kind,
                bundle_id,
                manifest,
                installed_complete,
                active_use,
            );
            scan_complete &= candidate.scan_complete;
            candidates.push(candidate);
        }
    }
    candidates.sort_by(|left, right| left.candidate_id.cmp(&right.candidate_id));
    let candidate_bytes = candidates.iter().fold(0u64, |total, candidate| {
        total.saturating_add(candidate.bytes)
    });
    let plan_fingerprint = plan_fingerprint(&candidates);
    let exact_approval_phrase = format!("DiskSage orphan cleanup 승인 {plan_fingerprint}");
    Ok(OrphanPlan {
        schema_kind: ORPHAN_SCHEMA_KIND.into(),
        schema_version: ORPHAN_SCHEMA_VERSION,
        generated_at_ms: now_ms,
        plan_fingerprint,
        candidate_count: candidates.len(),
        candidate_bytes,
        scan_complete,
        candidates,
        notices,
        local_paths_included: false,
        mutation_performed: false,
        exact_approval_phrase,
    })
}

#[cfg(target_os = "macos")]
fn valid_bundle_id(value: &str) -> bool {
    fn valid_part(part: &str) -> bool {
        !part.is_empty()
            && part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    }

    let mut parts = value.split('.');
    let Some(first) = parts.next() else {
        return false;
    };
    let Some(second) = parts.next() else {
        return false;
    };
    valid_part(first) && valid_part(second) && parts.all(valid_part)
}

#[cfg(target_os = "macos")]
fn bundle_id_from_name(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    valid_bundle_id(name).then(|| name.to_owned())
}

#[cfg(target_os = "macos")]
fn installed_bundle_ids(
    roots: &[PathBuf],
    deadline: std::time::Instant,
) -> (BTreeSet<String>, bool) {
    let mut ids = BTreeSet::new();
    let mut complete = true;
    for root in roots {
        complete &= collect_bundle_ids(root, 0, deadline, &mut ids);
    }
    let (launch_services_ids, launch_services_complete) = launch_services_bundle_ids(deadline);
    ids.extend(launch_services_ids);
    complete &= launch_services_complete;
    (ids, complete)
}

#[cfg(all(target_os = "macos", not(test)))]
fn kill_launch_services_group(child: &mut std::process::Child) {
    #[cfg(unix)]
    unsafe {
        let _ = libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL);
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(all(target_os = "macos", not(test)))]
fn launch_services_bundle_ids(deadline: Instant) -> (BTreeSet<String>, bool) {
    let mut command = Command::new("/usr/bin/mdfind");
    command
        .args(["-0", "kMDItemContentType == 'com.apple.application-bundle'"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Keep descendants in a private group so a deadline cannot leave a writer holding the
        // bounded stdout reader open after the direct mdfind process is killed.
        unsafe {
            command.pre_exec(|| {
                if libc::setpgid(0, 0) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(_) => return (BTreeSet::new(), false),
    };
    let Some(stdout) = child.stdout.take() else {
        kill_launch_services_group(&mut child);
        return (BTreeSet::new(), false);
    };
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let read_ok = stdout
            .take((MAX_LAUNCH_SERVICES_OUTPUT_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .is_ok();
        let truncated = !read_ok || bytes.len() > MAX_LAUNCH_SERVICES_OUTPUT_BYTES;
        (bytes, truncated)
    });
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(25));
            }
            Ok(None) => {
                timed_out = true;
                kill_launch_services_group(&mut child);
                break None;
            }
            Err(_) => {
                kill_launch_services_group(&mut child);
                break None;
            }
        }
    };
    let Ok((output, truncated)) = reader.join() else {
        return (BTreeSet::new(), false);
    };
    let Some(status) = status else {
        return (BTreeSet::new(), false);
    };
    if timed_out || truncated || !status.success() {
        return (BTreeSet::new(), false);
    }
    let mut ids = BTreeSet::new();
    let mut complete = true;
    for raw_path in output.split(|byte| *byte == 0).filter(|value| !value.is_empty()) {
        let Ok(path_text) = std::str::from_utf8(raw_path) else {
            complete = false;
            continue;
        };
        let path = Path::new(path_text);
        if !path.is_absolute()
            || path.extension().and_then(|value| value.to_str()) != Some("app")
        {
            continue;
        }
        if let Some(id) = read_bundle_id(path) {
            ids.insert(id);
        } else {
            complete = false;
        }
    }
    (ids, complete)
}

#[cfg(all(target_os = "macos", test))]
fn launch_services_bundle_ids(_deadline: Instant) -> (BTreeSet<String>, bool) {
    // Unit tests use temporary application roots; querying the host Launch Services database
    // would make their eligibility depend on unrelated installed applications.
    (BTreeSet::new(), true)
}

#[cfg(target_os = "macos")]
fn collect_bundle_ids(
    root: &Path,
    depth: usize,
    deadline: std::time::Instant,
    ids: &mut BTreeSet<String>,
) -> bool {
    if depth > MAX_BUNDLE_SCAN_DEPTH || std::time::Instant::now() >= deadline {
        // An unvisited subtree cannot prove that the installed-app inventory is complete.
        return false;
    }
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return true,
        Err(_) => return false,
    };
    let mut complete = true;
    for entry in entries {
        if std::time::Instant::now() >= deadline {
            complete = false;
            break;
        }
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => {
                complete = false;
                continue;
            }
        };
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            complete = false;
            continue;
        };
        if file_type.is_symlink() || !file_type.is_dir() {
            continue;
        }
        if path.extension().and_then(|extension| extension.to_str()) == Some("app") {
            if let Some(id) = read_bundle_id(&path) {
                ids.insert(id);
            } else {
                complete = false;
            }
        } else {
            complete &= collect_bundle_ids(&path, depth + 1, deadline, ids);
        }
    }
    complete
}

#[cfg(target_os = "macos")]
fn read_bundle_id(app: &Path) -> Option<String> {
    let plist_path = app.join("Contents/Info.plist");
    let metadata = std::fs::metadata(&plist_path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_BUNDLE_METADATA_BYTES {
        return None;
    }
    let mut bytes = Vec::new();
    File::open(plist_path)
        .ok()?
        .take(MAX_BUNDLE_METADATA_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_BUNDLE_METADATA_BYTES {
        return None;
    }
    let value = plist::Value::from_reader(std::io::Cursor::new(bytes)).ok()?;
    let id = value
        .as_dictionary()?
        .get("CFBundleIdentifier")?
        .as_string()?
        .to_owned();
    valid_bundle_id(&id).then_some(id)
}

#[cfg(target_os = "macos")]
fn bounded_manifest(root: &Path, deadline: std::time::Instant) -> Manifest {
    let mut manifest = Manifest {
        complete: true,
        ..Manifest::default()
    };
    collect_manifest(root, root, deadline, 0, &mut manifest);
    manifest.records.sort_unstable();
    manifest
}

#[cfg(target_os = "macos")]
fn collect_manifest(
    root: &Path,
    directory: &Path,
    deadline: std::time::Instant,
    depth: usize,
    manifest: &mut Manifest,
) {
    if depth > MAX_MANIFEST_DEPTH
        || std::time::Instant::now() >= deadline
        || manifest.records.len() >= MAX_MANIFEST_RECORDS
    {
        manifest.complete = false;
        manifest.skipped = manifest.skipped.saturating_add(1);
        return;
    }
    let read_entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(_) => {
            manifest.skipped = manifest.skipped.saturating_add(1);
            manifest.complete = false;
            return;
        }
    };
    let mut entries = Vec::new();
    for entry in read_entries {
        match entry {
            Ok(entry) => entries.push(entry),
            Err(_) => {
                manifest.skipped = manifest.skipped.saturating_add(1);
                manifest.complete = false;
            }
        }
    }
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        if std::time::Instant::now() >= deadline || manifest.records.len() >= MAX_MANIFEST_RECORDS {
            manifest.complete = false;
            manifest.skipped = manifest.skipped.saturating_add(1);
            return;
        }
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(_) => {
                manifest.skipped = manifest.skipped.saturating_add(1);
                manifest.complete = false;
                continue;
            }
        };
        if file_type.is_symlink() {
            manifest.skipped = manifest.skipped.saturating_add(1);
            manifest.complete = false;
            continue;
        }
        if file_type.is_dir() {
            manifest.records.push(format!("D:{relative}"));
            collect_manifest(root, &path, deadline, depth.saturating_add(1), manifest);
            continue;
        }
        if !file_type.is_file() {
            manifest.skipped = manifest.skipped.saturating_add(1);
            manifest.complete = false;
            continue;
        }
        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(_) => {
                manifest.skipped = manifest.skipped.saturating_add(1);
                manifest.complete = false;
                continue;
            }
        };
        let modified = metadata
            .modified()
            .ok()
            .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|value| format!("{}:{}", value.as_secs(), value.subsec_nanos()));
        let Some(modified) = modified else {
            manifest.skipped = manifest.skipped.saturating_add(1);
            manifest.complete = false;
            continue;
        };
        manifest.bytes = manifest.bytes.saturating_add(metadata.len());
        manifest.files = manifest.files.saturating_add(1);
        manifest
            .records
            .push(format!("F:{relative}:{}:{modified}", metadata.len()));
    }
}

#[cfg(target_os = "macos")]
fn directory_candidate(
    path: &Path,
    kind: &str,
    bundle_id: String,
    manifest: Manifest,
    installed_complete: bool,
    active_use: crate::cloud_local_eviction::ActiveUseEvidence,
) -> OrphanCandidate {
    let metadata_fingerprint = digest_values(
        &manifest
            .records
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
    );
    let object_id = crate::safety::filesystem_object_id(path).unwrap_or_default();
    let candidate_id = digest_values(&[kind, &bundle_id, &metadata_fingerprint, &object_id]);
    let ontology_class = if kind == "cache" {
        format!("{ONTOLOGY_NAMESPACE}RegenerableCache")
    } else {
        format!("{ONTOLOGY_NAMESPACE}ApplicationSupport")
    };
    let subject = format!("urn:disksage:orphan:{candidate_id}");
    let app = format!("urn:bundle:{bundle_id}");
    let mut review_reasons = vec!["bundle-id-not-present-in-installed-applications".into()];
    if kind == "cache" {
        review_reasons.push("cache-is-regenerable-but-still-requires-confirmation".into());
    } else {
        review_reasons.push("application-support-may-contain-user-data".into());
    }
    if !installed_complete {
        review_reasons.push("installed-application-inventory-incomplete".into());
    }
    if !manifest.complete || manifest.skipped != 0 {
        review_reasons.push("metadata-manifest-incomplete".into());
    }
    if active_use.error.is_some() {
        review_reasons.push("active-use-evidence-error".into());
    }
    if !active_use.evidence_complete || active_use.active || active_use.results_truncated {
        review_reasons.push("active-use-evidence-not-clear".into());
    }
    if object_id.is_empty() {
        review_reasons.push("filesystem-object-identity-unavailable".into());
    }
    let auto_trash_eligible = kind == "cache"
        && installed_complete
        && manifest.complete
        && manifest.skipped == 0
        && active_use.evidence_complete
        && !active_use.active
        && !active_use.results_truncated
        && active_use.error.is_none()
        && !object_id.is_empty();
    OrphanCandidate {
        candidate_id: candidate_id.clone(),
        kind: kind.into(),
        bundle_id: Some(bundle_id),
        bytes: manifest.bytes,
        files: manifest.files,
        skipped: manifest.skipped,
        scan_complete: manifest.complete,
        object_id,
        metadata_fingerprint,
        ontology_class: ontology_class.clone(),
        confidence: if auto_trash_eligible {
            "high"
        } else {
            "review"
        }
        .into(),
        active_use_evidence_complete: active_use.evidence_complete,
        active_use: active_use.active,
        relations: vec![
            OrphanRelationEvidence {
                subject: subject.clone(),
                predicate: format!("{ONTOLOGY_NAMESPACE}instanceOf"),
                object: format!("{ONTOLOGY_NAMESPACE}OrphanCandidate"),
                source: "bounded-metadata".into(),
            },
            OrphanRelationEvidence {
                subject: subject.clone(),
                predicate: format!("{ONTOLOGY_NAMESPACE}locatedIn"),
                object: ontology_class,
                source: "macOS-library-domain".into(),
            },
            OrphanRelationEvidence {
                subject: subject.clone(),
                predicate: format!("{ONTOLOGY_NAMESPACE}uninstalledApplicationOf"),
                object: app,
                source: "installed-bundle-inventory".into(),
            },
        ],
        review_reasons,
        auto_trash_eligible,
        path: path.to_path_buf(),
    }
}

fn digest_values(values: &[&str]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"disksage-orphan-v1\0");
    for value in values {
        hasher.update(&(value.len() as u64).to_le_bytes());
        hasher.update(value.as_bytes());
    }
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(target_os = "macos")]
fn plan_fingerprint(candidates: &[OrphanCandidate]) -> String {
    let mut values = vec!["plan"];
    values.extend(
        candidates
            .iter()
            .map(|candidate| candidate.candidate_id.as_str()),
    );
    digest_values(&values)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn safe_candidate(candidate_id: &str) -> OrphanCandidate {
        OrphanCandidate {
            candidate_id: candidate_id.into(),
            kind: "cache".into(),
            bundle_id: Some("com.example.old".into()),
            bytes: 1,
            files: 1,
            skipped: 0,
            scan_complete: true,
            object_id: format!("object-{candidate_id}"),
            metadata_fingerprint: format!("metadata-{candidate_id}"),
            ontology_class: format!("{ONTOLOGY_NAMESPACE}RegenerableCache"),
            confidence: "high".into(),
            active_use_evidence_complete: true,
            active_use: false,
            relations: Vec::new(),
            review_reasons: Vec::new(),
            auto_trash_eligible: true,
            path: PathBuf::from(format!("/private/nonexistent/disksage-{candidate_id}")),
        }
    }

    fn request_for(candidate: &OrphanCandidate) -> OrphanCleanupRequest {
        OrphanCleanupRequest {
            candidate_id: candidate.candidate_id.clone(),
            metadata_fingerprint: candidate.metadata_fingerprint.clone(),
            bytes: candidate.bytes,
            files: candidate.files,
            skipped: candidate.skipped,
            scan_complete: candidate.scan_complete,
            object_id: candidate.object_id.clone(),
        }
    }

    fn authoritative_plan(candidate: OrphanCandidate) -> OrphanPlan {
        OrphanPlan {
            schema_kind: ORPHAN_SCHEMA_KIND.into(),
            schema_version: ORPHAN_SCHEMA_VERSION,
            generated_at_ms: 1,
            plan_fingerprint: "b".repeat(64),
            candidate_count: 1,
            candidate_bytes: candidate.bytes,
            scan_complete: true,
            candidates: vec![candidate],
            notices: Vec::new(),
            local_paths_included: false,
            mutation_performed: false,
            exact_approval_phrase: "phrase".into(),
        }
    }

    #[test]
    fn digest_is_domain_separated_and_fixed_width() {
        let digest = digest_values(&["cache", "com.example.old", "manifest"]);
        assert_eq!(digest.len(), 64);
        assert_ne!(
            digest,
            digest_values(&["cache", "com.example.other", "manifest"])
        );
    }

    #[test]
    fn move_to_trash_rejects_missing_authority_before_touching_filesystem() {
        let plan = OrphanPlan {
            schema_kind: ORPHAN_SCHEMA_KIND.into(),
            schema_version: ORPHAN_SCHEMA_VERSION,
            generated_at_ms: 1,
            plan_fingerprint: "b".repeat(64),
            candidate_count: 0,
            candidate_bytes: 0,
            scan_complete: false,
            candidates: Vec::new(),
            notices: Vec::new(),
            local_paths_included: false,
            mutation_performed: false,
            exact_approval_phrase: "phrase".into(),
        };
        let error = move_to_trash(
            &plan,
            &[],
            "phrase",
            "operator review",
            Path::new("/private/nonexistent/journal"),
            2,
        )
        .unwrap_err();
        assert_eq!(error, "orphan-plan-not-authoritative");
    }

    #[test]
    fn cleanup_batch_is_preflighted_before_mutation() {
        let candidate = safe_candidate("candidate");
        let valid = request_for(&candidate);
        let mut stale = valid.clone();
        stale.candidate_id = "not-in-plan".into();
        let plan = authoritative_plan(candidate);

        let error = validate_cleanup_requests(&plan, &[valid, stale]).unwrap_err();
        assert_eq!(error, "orphan-candidate-not-in-plan");
    }

    #[test]
    fn move_to_trash_rejects_candidate_identity_mismatch_without_mutation() {
        let candidate = safe_candidate("candidate");
        let plan = authoritative_plan(candidate);
        let request = OrphanCleanupRequest {
            candidate_id: "candidate".into(),
            metadata_fingerprint: "metadata-candidate".into(),
            bytes: 1,
            files: 1,
            skipped: 0,
            scan_complete: true,
            object_id: "replacement-object".into(),
        };
        let error = move_to_trash(
            &plan,
            &[request],
            "phrase",
            "operator review",
            Path::new("/private/nonexistent/journal"),
            2,
        )
        .unwrap_err();
        assert_eq!(error, "orphan-candidate-safety-gate-blocked");
        let serialized = serde_json::to_string(&plan.candidates[0]).unwrap();
        assert!(!serialized.contains("/private/nonexistent"));
        assert!(serialized.contains("object-candidate"));
    }

    #[test]
    fn move_to_trash_redacts_native_failure_detail() {
        let candidate = safe_candidate("candidate");
        let plan = authoritative_plan(candidate);
        let request = request_for(&plan.candidates[0]);
        let result = move_to_trash(
            &plan,
            &[request],
            "phrase",
            "operator review",
            Path::new("/private/nonexistent/journal"),
            2,
        )
        .unwrap();
        assert_eq!(result.moved_count, 0);
        assert_eq!(
            result.items[0].error.as_deref(),
            Some("orphan-trash-operation-failed")
        );
        let serialized = serde_json::to_string(&result).unwrap();
        assert!(!serialized.contains("/private/nonexistent"));
    }

    #[test]
    fn serialized_plan_has_no_home_scope_field() {
        let plan = OrphanPlan {
            schema_kind: ORPHAN_SCHEMA_KIND.into(),
            schema_version: ORPHAN_SCHEMA_VERSION,
            generated_at_ms: 1,
            plan_fingerprint: "b".repeat(64),
            candidate_count: 0,
            candidate_bytes: 0,
            scan_complete: true,
            candidates: Vec::new(),
            notices: Vec::new(),
            local_paths_included: false,
            mutation_performed: false,
            exact_approval_phrase: "phrase".into(),
        };
        let serialized = serde_json::to_string(&plan).unwrap();
        assert!(!serialized.contains("root_fingerprint"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn planner_home_scope_allows_user_home_but_blocks_system_roots() {
        assert!(planner_home_scope_is_safe(Path::new("/Users/example")));
        assert!(!planner_home_scope_is_safe(Path::new("/")));
        assert!(!planner_home_scope_is_safe(Path::new("/System")));
        assert!(!planner_home_scope_is_safe(Path::new("/Library")));
        assert!(!planner_home_scope_is_safe(Path::new("/Applications")));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn bundle_id_validation_accepts_general_reverse_dns_shape() {
        assert!(valid_bundle_id("dev.example.editor"));
        assert!(valid_bundle_id("co.example.editor"));
        assert!(valid_bundle_id("com.example.editor"));
        assert!(!valid_bundle_id("com"));
        assert!(!valid_bundle_id(".example.editor"));
        assert!(!valid_bundle_id("com..editor"));
        assert!(!valid_bundle_id("com.example/editor"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn deep_installed_application_inventory_is_incomplete() {
        let tmp = tempfile::tempdir().unwrap();
        let deep = tmp
            .path()
            .join("one/two/three/four/com.example.installed.app/Contents");
        std::fs::create_dir_all(&deep).unwrap();
        let mut ids = BTreeSet::new();
        assert!(!collect_bundle_ids(
            tmp.path(),
            0,
            std::time::Instant::now() + std::time::Duration::from_secs(1),
            &mut ids,
        ));
        assert!(!ids.contains("com.example.installed"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn deep_cache_manifest_is_incomplete() {
        let tmp = tempfile::tempdir().unwrap();
        let mut manifest = Manifest::default();
        collect_manifest(
            tmp.path(),
            tmp.path(),
            std::time::Instant::now() + std::time::Duration::from_secs(1),
            MAX_MANIFEST_DEPTH + 1,
            &mut manifest,
        );
        assert!(!manifest.complete);
        assert_eq!(manifest.skipped, 1);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn plan_relations_are_path_free_and_application_support_is_review_only() {
        let tmp = tempfile::tempdir().unwrap();
        let support = tmp
            .path()
            .join("Library/Application Support/com.example.old");
        let caches = tmp.path().join("Library/Caches/com.example.old");
        std::fs::create_dir_all(&support).unwrap();
        std::fs::create_dir_all(&caches).unwrap();
        std::fs::write(caches.join("cache.bin"), b"cache").unwrap();
        let application_roots = [tmp.path().join("Applications")];
        std::fs::create_dir_all(&application_roots[0]).unwrap();
        let plan = plan_for_roots(
            tmp.path(),
            &[
                (tmp.path().join("Library/Caches"), "cache"),
                (
                    tmp.path().join("Library/Application Support"),
                    "application-support",
                ),
            ],
            &application_roots,
            42,
        )
        .unwrap();
        assert_eq!(plan.generated_at_ms, 42);
        assert!(!plan.local_paths_included);
        let serialized_plan = serde_json::to_string(&plan).unwrap();
        assert!(!serialized_plan.contains(&tmp.path().to_string_lossy().to_string()));
        assert!(!serialized_plan.contains("root_fingerprint"));
        assert!(plan.candidates.iter().all(|candidate| candidate
            .relations
            .iter()
            .all(|relation| !relation.subject.contains("/"))));
        let support_candidate = plan
            .candidates
            .iter()
            .find(|candidate| candidate.kind == "application-support")
            .unwrap();
        assert!(!support_candidate.auto_trash_eligible);
        let cache_candidate = plan
            .candidates
            .iter()
            .find(|candidate| candidate.kind == "cache")
            .unwrap();
        assert!(cache_candidate.auto_trash_eligible);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn execution_rejects_metadata_change_before_trash() {
        let tmp = tempfile::tempdir().unwrap();
        let caches = tmp.path().join("Library/Caches/com.example.old");
        std::fs::create_dir_all(&caches).unwrap();
        std::fs::write(caches.join("cache.bin"), b"cache").unwrap();
        let application_roots = [tmp.path().join("Applications")];
        std::fs::create_dir_all(&application_roots[0]).unwrap();
        let plan = plan_for_roots(
            tmp.path(),
            &[(tmp.path().join("Library/Caches"), "cache")],
            &application_roots,
            42,
        )
        .unwrap();
        let candidate = plan.candidates.first().unwrap();
        let request = request_for(candidate);
        std::fs::write(caches.join("cache.bin"), b"changed-cache").unwrap();
        assert_eq!(
            move_to_trash(
                &plan,
                &[request],
                &plan.exact_approval_phrase,
                "operator review",
                &tmp.path().join("journal.jsonl"),
                43,
            )
            .unwrap_err(),
            "orphan-candidate-metadata-changed"
        );
        assert!(caches.exists());
    }

    #[test]
    fn completed_orphan_move_serializes_warning_without_failure() {
        let result = OrphanCleanupItemResult {
            candidate_id: "candidate-1".into(),
            bytes: 4096,
            attempted: true,
            moved_to_trash: true,
            error: None,
            warning: Some("terminal audit record unavailable".into()),
        };
        let value = serde_json::to_value(result).unwrap();

        assert_eq!(value["moved_to_trash"], true);
        assert!(value["error"].is_null());
        assert_eq!(value["warning"], "terminal audit record unavailable");
    }
}
