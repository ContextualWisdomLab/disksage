//! Exact-evidence cleanup for provider-owned, regenerable macOS caches.
//!
//! Only three provider cache classes are eligible: superseded Edge installed copies, the Edge CRX
//! cache, and content-addressed Podman AppleHV machine seeds that are not the configured VM image.
//! Every purge is re-planned, explicitly approved, active-use checked, identity-bound, journaled,
//! and preceded by an immutable private receipt. Provider roots and inventories fail closed.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const SCHEMA_VERSION: u32 = 1;
const MAX_MANIFEST_ENTRIES: usize = 200_000;
const MAX_CLEANUP_REQUESTS: usize = 1_024;
const MANIFEST_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_RATIONALE_CHARS: usize = 1_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderCacheKind {
    PodmanMachineSeed,
    EdgeSupersededInstalledCopy,
    EdgeCrxCache,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderCacheCleanupMode {
    Trash,
    PermanentPurge,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderCacheCandidate {
    pub kind: ProviderCacheKind,
    pub path: String,
    pub logical_bytes: u64,
    pub allocated_bytes: Option<u64>,
    pub object_id: String,
    pub content_manifest: String,
    pub evidence_fingerprint: String,
    pub active_use: crate::cloud_local_eviction::ActiveUseEvidence,
    pub recreation_source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderCacheReclaimPlan {
    pub schema_version: u32,
    pub platform: String,
    pub observed_at_ms: u64,
    pub installed_edge_version: Option<String>,
    pub podman_machine_present: bool,
    pub podman_recreation_source: Option<String>,
    pub evidence_complete: bool,
    pub candidates: Vec<ProviderCacheCandidate>,
    pub issues: Vec<String>,
    pub plan_fingerprint: String,
    pub exact_approval_phrase: Option<String>,
    pub trash_approval_phrase: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderCacheCleanupRequest {
    pub path: String,
    pub evidence_fingerprint: String,
    pub object_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProviderCacheCleanupItemResult {
    pub path: String,
    pub completed: bool,
    pub error: Option<String>,
    pub audit_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProviderCacheCleanupResult {
    pub plan_fingerprint: String,
    pub requested_count: usize,
    pub completed_count: usize,
    pub executed_at_ms: u64,
    pub rationale: String,
    pub mode: ProviderCacheCleanupMode,
    pub immutable_receipt_path: String,
    pub items: Vec<ProviderCacheCleanupItemResult>,
}

#[derive(Debug, Clone)]
struct PodmanRecreationEvidence {
    source: String,
    active_image_path: PathBuf,
    active_image_object_id: String,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn allocated_bytes(metadata: &fs::Metadata) -> Option<u64> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Some(metadata.blocks().saturating_mul(512))
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        None
    }
}

fn frame(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
}

fn file_sha256(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|_| "provider-cache-file-open-failed")?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 1024 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| "provider-cache-file-read-failed")?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex(&hasher.finalize()))
}

fn tree_manifest(path: &Path) -> Result<(u64, Option<u64>, String), String> {
    let deadline = Instant::now() + MANIFEST_TIMEOUT;
    let mut logical = 0u64;
    let mut allocated = Some(0u64);
    let mut records = Vec::new();
    for entry in walkdir::WalkDir::new(path)
        .follow_links(false)
        .sort_by_file_name()
    {
        if Instant::now() >= deadline || records.len() >= MAX_MANIFEST_ENTRIES {
            return Err("provider-cache-manifest-incomplete".into());
        }
        let entry = entry.map_err(|_| "provider-cache-manifest-entry-unavailable")?;
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|_| "provider-cache-manifest-metadata-unavailable")?;
        if metadata.file_type().is_symlink() {
            return Err("provider-cache-symlink-rejected".into());
        }
        if !(metadata.is_dir() || metadata.is_file()) {
            return Err("provider-cache-object-type-rejected".into());
        }
        if metadata.is_file() {
            logical = logical.saturating_add(metadata.len());
        }
        allocated = allocated
            .zip(allocated_bytes(&metadata))
            .map(|(total, bytes)| total.saturating_add(bytes));
        let relative = entry
            .path()
            .strip_prefix(path)
            .map_err(|_| "provider-cache-manifest-path-escape")?;
        let modified = metadata
            .modified()
            .ok()
            .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|value| value.as_nanos())
            .unwrap_or(0);
        records.push(format!(
            "{}\0{}\0{}\0{}\0{}",
            relative.to_string_lossy(),
            if metadata.is_dir() { "d" } else { "f" },
            metadata.len(),
            modified,
            crate::safety::object_id_from_metadata(&metadata).unwrap_or_default(),
        ));
    }
    let mut hasher = Sha256::new();
    hasher.update(b"disksage.provider-cache-tree.v1");
    for record in records {
        frame(&mut hasher, record.as_bytes());
    }
    Ok((logical, allocated, hex(&hasher.finalize())))
}

fn active_use_safe(evidence: &crate::cloud_local_eviction::ActiveUseEvidence) -> bool {
    evidence.method == "lsof-fp+ps-command"
        && evidence.evidence_complete
        && !evidence.active
        && evidence.observed_pids.is_empty()
        && !evidence.results_truncated
        && evidence.error.is_none()
}

fn candidate(
    kind: ProviderCacheKind,
    path: &Path,
    recreation_source: String,
    content_digest: Option<String>,
) -> Result<ProviderCacheCandidate, String> {
    let metadata = fs::symlink_metadata(path).map_err(|_| "provider-cache-metadata-unavailable")?;
    if metadata.file_type().is_symlink() || !(metadata.is_file() || metadata.is_dir()) {
        return Err("provider-cache-object-type-rejected".into());
    }
    let (logical_bytes, allocated_bytes, manifest) = if metadata.is_dir() {
        tree_manifest(path)?
    } else {
        (
            metadata.len(),
            allocated_bytes(&metadata),
            content_digest.ok_or("provider-cache-content-digest-missing")?,
        )
    };
    let object_id = crate::safety::filesystem_object_id(path)
        .map_err(|_| "provider-cache-object-identity-unavailable")?;
    let active_use = crate::cloud_local_eviction::observe_path_active_use(path);
    if !active_use_safe(&active_use) {
        return Err("provider-cache-active-use-or-evidence-gap".into());
    }
    let mut hasher = Sha256::new();
    hasher.update(b"disksage.provider-cache-candidate.v1");
    frame(&mut hasher, format!("{kind:?}").as_bytes());
    frame(&mut hasher, path.to_string_lossy().as_bytes());
    frame(&mut hasher, &logical_bytes.to_be_bytes());
    frame(
        &mut hasher,
        &allocated_bytes.unwrap_or_default().to_be_bytes(),
    );
    frame(&mut hasher, object_id.as_bytes());
    frame(&mut hasher, manifest.as_bytes());
    frame(&mut hasher, recreation_source.as_bytes());
    Ok(ProviderCacheCandidate {
        kind,
        path: path.to_string_lossy().into_owned(),
        logical_bytes,
        allocated_bytes,
        object_id,
        content_manifest: manifest,
        evidence_fingerprint: hex(&hasher.finalize()),
        active_use,
        recreation_source,
    })
}

fn candidate_content_still_matches(candidate: &ProviderCacheCandidate) -> Result<(), String> {
    let path = Path::new(&candidate.path);
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| "provider-cache-content-recheck-metadata-unavailable")?;
    if metadata.file_type().is_symlink() || !(metadata.is_file() || metadata.is_dir()) {
        return Err("provider-cache-content-recheck-type-changed".into());
    }
    if crate::safety::filesystem_object_id(path).ok().as_deref() != Some(&candidate.object_id) {
        return Err("provider-cache-object-identity-changed".into());
    }
    let current_manifest = if metadata.is_dir() {
        tree_manifest(path).map(|(_, _, manifest)| manifest)?
    } else {
        file_sha256(path)?
    };
    if current_manifest != candidate.content_manifest {
        return Err("provider-cache-content-changed".into());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn plist_version(path: &Path) -> Option<String> {
    plist::Value::from_file(path)
        .ok()?
        .as_dictionary()?
        .get("CFBundleShortVersionString")?
        .as_string()
        .filter(|value| !value.is_empty() && !value.chars().any(char::is_control))
        .map(str::to_owned)
}

#[cfg(not(target_os = "macos"))]
fn plist_version(_path: &Path) -> Option<String> {
    None
}

fn podman_recreation_evidence(
    podman_bin: &Path,
    machine: &str,
) -> Result<PodmanRecreationEvidence, String> {
    let version = crate::podman_reclaim::command_text(
        podman_bin,
        &["--version"],
        crate::podman_reclaim::DEFAULT_PROBE_TIMEOUT,
        "podman-recreation-version",
    )
    .map_err(|error| {
        if error.starts_with("podman-recreation-version-failed:") {
            "podman-recreation-version-failed"
        } else {
            "podman-recreation-version-unavailable"
        }
    })?;
    if version.trim().is_empty() {
        return Err("podman-recreation-version-invalid".into());
    }
    let inspect = crate::podman_reclaim::command_text(
        podman_bin,
        &["machine", "inspect", machine],
        crate::podman_reclaim::DEFAULT_PROBE_TIMEOUT,
        "podman-recreation-machine-inspect",
    )
    .map_err(|error| {
        if error.starts_with("podman-recreation-machine-inspect-failed:") {
            "podman-recreation-machine-missing"
        } else {
            "podman-recreation-machine-inspect-unavailable"
        }
    })?;
    let value: serde_json::Value =
        serde_json::from_str(&inspect).map_err(|_| "podman-recreation-machine-inspect-invalid")?;
    let record = value
        .as_array()
        .and_then(|records| (records.len() == 1).then(|| &records[0]))
        .ok_or("podman-recreation-machine-count-invalid")?;
    let name = record.get("Name").and_then(serde_json::Value::as_str);
    let image_path = record
        .pointer("/ConfigDir/Path")
        .and_then(serde_json::Value::as_str)
        .map(PathBuf::from)
        .and_then(|config| fs::read_to_string(config.join(format!("{machine}.json"))).ok())
        .and_then(|body| serde_json::from_str::<serde_json::Value>(&body).ok())
        .and_then(|config| {
            config
                .pointer("/ImagePath/Path")?
                .as_str()
                .map(PathBuf::from)
        });
    let image_path = image_path.ok_or("podman-recreation-active-image-unconfirmed")?;
    let metadata = fs::symlink_metadata(&image_path)
        .map_err(|_| "podman-recreation-active-image-unconfirmed")?;
    if name != Some(machine) || metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("podman-recreation-active-image-unconfirmed".into());
    }
    let active_image_path = fs::canonicalize(&image_path)
        .map_err(|_| "podman-recreation-active-image-canonicalization-unavailable")?;
    let active_image_object_id = crate::safety::filesystem_object_id(&image_path)
        .map_err(|_| "podman-recreation-active-image-identity-unavailable")?;
    let source = format!(
        "{}|{}|{}|{}",
        podman_bin.display(),
        version.trim(),
        machine,
        active_image_object_id
    );
    Ok(PodmanRecreationEvidence {
        source,
        active_image_path,
        active_image_object_id,
    })
}

fn directory_metadata(
    path: &Path,
    issue_prefix: &str,
    issues: &mut Vec<String>,
) -> Option<fs::Metadata> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == ErrorKind::NotFound => None,
        Err(_) => {
            issues.push(format!("{issue_prefix}-root-metadata-unavailable"));
            None
        }
        Ok(metadata) if metadata.file_type().is_symlink() => {
            issues.push(format!("{issue_prefix}-root-symlink-rejected"));
            None
        }
        Ok(metadata) if !metadata.is_dir() => {
            issues.push(format!("{issue_prefix}-root-type-rejected"));
            None
        }
        Ok(metadata) => Some(metadata),
    }
}

fn directory_entries(
    path: &Path,
    issue_prefix: &str,
    issues: &mut Vec<String>,
) -> Vec<fs::DirEntry> {
    if directory_metadata(path, issue_prefix, issues).is_none() {
        return Vec::new();
    }
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(_) => {
            issues.push(format!("{issue_prefix}-read-dir-failed"));
            return Vec::new();
        }
    };
    let deadline = Instant::now() + MANIFEST_TIMEOUT;
    let mut output = Vec::new();
    for entry in entries {
        if Instant::now() >= deadline {
            issues.push(format!("{issue_prefix}-inventory-timeout"));
            break;
        }
        if output.len() >= MAX_MANIFEST_ENTRIES {
            issues.push(format!("{issue_prefix}-inventory-limit-exceeded"));
            break;
        }
        match entry {
            Ok(entry) => output.push(entry),
            Err(_) => issues.push(format!("{issue_prefix}-entry-unavailable")),
        }
    }
    output
}

/// Plan with explicit platform roots, used by the desktop command and audited headless CLI.
pub fn plan_with_runtime(
    home: &Path,
    applications: &Path,
    podman_bin: &Path,
    observed_at_ms: u64,
) -> ProviderCacheReclaimPlan {
    plan_with_runtime_evidence(home, applications, podman_bin, observed_at_ms)
}

fn plan_with_runtime_evidence(
    home: &Path,
    applications: &Path,
    podman_bin: &Path,
    observed_at_ms: u64,
) -> ProviderCacheReclaimPlan {
    let mut candidates = Vec::new();
    let mut issues = Vec::new();

    let edge_info = applications.join("Microsoft Edge.app/Contents/Info.plist");
    let installed_edge_version = plist_version(&edge_info);
    let edge_root = home.join("Library/Application Support/Microsoft/EdgeUpdater");
    let edge_root_safe = directory_metadata(&edge_root, "edge-updater-cache", &mut issues).is_some();
    if let Some(installed) = installed_edge_version.as_deref() {
        if edge_root_safe {
            let versions = edge_root.join("apps/msedge-stable");
            for entry in directory_entries(&versions, "edge-stale-cache", &mut issues) {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().into_owned();
                let cached_info = path.join("Microsoft Edge.app/Contents/Info.plist");
                if name == installed || plist_version(&cached_info).as_deref() != Some(&name) {
                    continue;
                }
                match candidate(
                    ProviderCacheKind::EdgeSupersededInstalledCopy,
                    &path,
                    format!("installed-edge-version:{installed}"),
                    None,
                ) {
                    Ok(value) => candidates.push(value),
                    Err(error) => issues.push(format!("edge-stale-cache:{name}:{error}")),
                }
            }
            let crx = edge_root.join("crx_cache");
            if directory_metadata(&crx, "edge-crx-cache", &mut issues).is_some() {
                match candidate(
                    ProviderCacheKind::EdgeCrxCache,
                    &crx,
                    format!("edge-updater-installed-version:{installed}"),
                    None,
                ) {
                    Ok(value) => candidates.push(value),
                    Err(error) => issues.push(format!("edge-crx-cache:{error}")),
                }
            }
        }
    } else if edge_root_safe {
        issues.push("edge-installed-version-unavailable".into());
    }

    let podman_evidence =
        podman_recreation_evidence(podman_bin, crate::podman_reclaim::DEFAULT_PODMAN_MACHINE);
    let podman_recreation_source = podman_evidence.as_ref().ok().map(|value| value.source.clone());
    let cache = home.join(".local/share/containers/podman/machine/applehv/cache");
    let cache_safe = directory_metadata(&cache, "podman-seed-cache", &mut issues).is_some();
    match (&podman_evidence, cache_safe) {
        (Ok(evidence), true) => {
            for entry in directory_entries(&cache, "podman-seed-cache", &mut issues) {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().into_owned();
                let Some(key) = name.strip_suffix(".raw.zst") else {
                    continue;
                };
                if key.len() != 64 || !key.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                    issues.push(format!("podman-seed-cache-key-invalid:{name}"));
                    continue;
                }
                let object_id = match crate::safety::filesystem_object_id(&path) {
                    Ok(value) => value,
                    Err(_) => {
                        issues.push(format!(
                            "podman-seed-cache:{name}:provider-cache-object-identity-unavailable"
                        ));
                        continue;
                    }
                };
                let canonical_path = match fs::canonicalize(&path) {
                    Ok(value) => value,
                    Err(_) => {
                        issues.push(format!("podman-seed-cache:{name}:canonical-path-unavailable"));
                        continue;
                    }
                };
                if object_id == evidence.active_image_object_id
                    || canonical_path == evidence.active_image_path
                {
                    issues.push("podman-seed-cache-configured-image-excluded".into());
                    continue;
                }
                let digest = match file_sha256(&path) {
                    Ok(value) => value,
                    Err(error) => {
                        issues.push(format!("podman-seed-cache:{name}:{error}"));
                        continue;
                    }
                };
                if !digest.eq_ignore_ascii_case(key) {
                    issues.push(format!("podman-seed-cache-digest-mismatch:{name}"));
                    continue;
                }
                match candidate(
                    ProviderCacheKind::PodmanMachineSeed,
                    &path,
                    evidence.source.clone(),
                    Some(digest),
                ) {
                    Ok(value) => candidates.push(value),
                    Err(error) => issues.push(format!("podman-seed-cache:{name}:{error}")),
                }
            }
        }
        (Err(error), true) => issues.push(error.clone()),
        _ => {}
    }

    candidates.sort_by(|left, right| left.path.cmp(&right.path));
    issues.sort();
    issues.dedup();
    let mut hasher = Sha256::new();
    hasher.update(b"disksage.provider-cache-plan.v1");
    frame(
        &mut hasher,
        installed_edge_version.as_deref().unwrap_or("").as_bytes(),
    );
    frame(
        &mut hasher,
        podman_recreation_source.as_deref().unwrap_or("").as_bytes(),
    );
    for value in &candidates {
        frame(&mut hasher, value.evidence_fingerprint.as_bytes());
    }
    for issue in &issues {
        frame(&mut hasher, issue.as_bytes());
    }
    let plan_fingerprint = hex(&hasher.finalize());
    let evidence_complete = issues.is_empty();
    let exact_approval_phrase = (evidence_complete && !candidates.is_empty())
        .then(|| format!("DiskSage provider cache permanent cleanup 승인 {plan_fingerprint}"));
    let trash_approval_phrase = (evidence_complete && !candidates.is_empty())
        .then(|| format!("DiskSage provider cache trash 승인 {plan_fingerprint}"));
    ProviderCacheReclaimPlan {
        schema_version: SCHEMA_VERSION,
        platform: std::env::consts::OS.into(),
        observed_at_ms,
        installed_edge_version,
        podman_machine_present: podman_recreation_source.is_some(),
        podman_recreation_source,
        evidence_complete,
        candidates,
        issues,
        plan_fingerprint,
        exact_approval_phrase,
        trash_approval_phrase,
    }
}

/// Inspect exact provider cache candidates. This is read-only.
pub fn plan(home: &Path, observed_at_ms: u64) -> ProviderCacheReclaimPlan {
    plan_with_runtime(
        home,
        Path::new("/Applications"),
        Path::new("/opt/homebrew/bin/podman"),
        observed_at_ms,
    )
}

fn valid_rationale(value: &str) -> bool {
    !value.is_empty()
        && value == value.trim()
        && value.chars().count() <= MAX_RATIONALE_CHARS
        && !value.chars().any(char::is_control)
}

fn write_immutable_receipt(
    receipt_dir: &Path,
    plan: &ProviderCacheReclaimPlan,
    requested: &[ProviderCacheCleanupRequest],
    rationale: &str,
    mode: ProviderCacheCleanupMode,
    approval_phrase: &str,
    executed_at_ms: u64,
) -> Result<PathBuf, String> {
    let mode_label = match mode {
        ProviderCacheCleanupMode::Trash => "trash",
        ProviderCacheCleanupMode::PermanentPurge => "permanent-purge",
    };
    let path = receipt_dir.join(format!(
        "provider-cache-{mode_label}-{}-{executed_at_ms}.json",
        plan.plan_fingerprint
    ));
    let body = serde_json::to_vec_pretty(&serde_json::json!({
        "schema_kind": "disksage.provider-cache-cleanup-receipt",
        "schema_version": SCHEMA_VERSION,
        "plan_fingerprint": plan.plan_fingerprint,
        "approval_phrase": approval_phrase,
        "requested": requested,
        "rationale": rationale,
        "authorized_at_ms": executed_at_ms,
        "mode": mode,
    }))
    .map_err(|_| "provider-cache-receipt-serialization-failed")?;

    #[cfg(unix)]
    {
        crate::private_evidence::write_object_bound_bytes_create_new(&path, &body, 0o400, None)
            .map_err(|_| "provider-cache-receipt-object-bound-publication-failed".to_string())?;
        Ok(path)
    }

    #[cfg(not(unix))]
    {
        let _ = (path, body);
        Err("provider-cache-receipt-object-bound-publication-unsupported".into())
    }
}

#[cfg(test)]
fn restore_staged_file_without_replacement(staged: &Path, original: &Path) -> Result<(), String> {
    match fs::hard_link(staged, original) {
        Ok(()) => fs::remove_file(staged)
            .map_err(|_| "provider-cache-permanent-file-purge-restore-unlink-failed".to_string()),
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {
            Err("provider-cache-permanent-file-purge-restore-destination-exists".into())
        }
        Err(_) => Err("provider-cache-permanent-file-purge-restore-failed".into()),
    }
}

#[cfg(test)]
fn permanently_purge_exact(
    candidate: &ProviderCacheCandidate,
    journal_path: &Path,
    now_ms: u64,
) -> Result<Option<String>, String> {
    permanently_purge_exact_with_after_stage(candidate, journal_path, now_ms, |_, _| Ok(()))
}

#[cfg(test)]
fn permanently_purge_exact_with_after_stage<F>(
    candidate: &ProviderCacheCandidate,
    journal_path: &Path,
    now_ms: u64,
    after_stage: F,
) -> Result<Option<String>, String>
where
    F: FnOnce(&Path, &Path) -> Result<(), String>,
{
    let path = Path::new(&candidate.path);
    if crate::safety::filesystem_object_id(path).ok().as_deref() != Some(&candidate.object_id) {
        return Err("provider-cache-object-identity-changed".into());
    }
    let parent = path.parent().ok_or("provider-cache-parent-unavailable")?;
    let staged = parent.join(format!(
        ".disksage-provider-cache-purge-{now_ms}-{}",
        &candidate.evidence_fingerprint[..12]
    ));
    if staged.exists() {
        return Err("provider-cache-staging-collision".into());
    }
    let mut journal = crate::safety::JournalEntry {
        ts_ms: now_ms,
        op: "permanent_provider_cache_delete".into(),
        path: candidate.path.clone(),
        bytes: candidate.logical_bytes,
        outcome: "pending".into(),
    };
    crate::safety::journal_append(journal_path, &journal).map_err(|error| error.to_string())?;
    fs::rename(path, &staged).map_err(|_| "provider-cache-atomic-stage-failed")?;
    let result = (|| -> Result<(), String> {
        if let Err(error) = after_stage(&staged, path) {
            let metadata = fs::symlink_metadata(&staged)
                .map_err(|_| "provider-cache-staged-metadata-unavailable")?;
            if metadata.is_file() {
                restore_staged_file_without_replacement(&staged, path)?;
            } else {
                fs::rename(&staged, path)
                    .map_err(|_| "provider-cache-permanent-directory-purge-restore-failed")?;
            }
            return Err(error);
        }
        if crate::safety::filesystem_object_id(&staged).ok().as_deref()
            != Some(&candidate.object_id)
        {
            let metadata = fs::symlink_metadata(&staged)
                .map_err(|_| "provider-cache-staged-metadata-unavailable")?;
            if metadata.is_file() {
                restore_staged_file_without_replacement(&staged, path)?;
            } else {
                fs::rename(&staged, path)
                    .map_err(|_| "provider-cache-permanent-directory-purge-restore-failed")?;
            }
            return Err("provider-cache-staged-identity-changed".to_string());
        }
        let metadata = fs::symlink_metadata(&staged)
            .map_err(|_| "provider-cache-staged-metadata-unavailable")?;
        if metadata.is_dir() {
            let staged_manifest = tree_manifest(&staged).map(|(_, _, manifest)| manifest);
            if staged_manifest.as_ref().ok().map(String::as_str)
                != Some(candidate.content_manifest.as_str())
            {
                fs::rename(&staged, path)
                    .map_err(|_| "provider-cache-permanent-directory-purge-restore-failed")?;
                Err("provider-cache-staged-manifest-changed".to_string())
            } else {
                fs::remove_dir_all(&staged).map_err(|_| {
                    "provider-cache-permanent-directory-delete-partial-failure-staged".to_string()
                })
            }
        } else {
            match file_sha256(&staged) {
                Ok(digest) if digest == candidate.content_manifest => {
                    match fs::remove_file(&staged) {
                        Ok(()) => Ok(()),
                        Err(_) => match restore_staged_file_without_replacement(&staged, path) {
                            Ok(()) => Err("provider-cache-permanent-delete-failed".to_string()),
                            Err(error) => Err(error),
                        },
                    }
                }
                _ => match restore_staged_file_without_replacement(&staged, path) {
                    Ok(()) => Err("provider-cache-staged-content-changed".to_string()),
                    Err(error) => Err(error),
                },
            }
        }
    })();
    journal.outcome = if result.is_ok() { "ok" } else { "error" }.into();
    finish_purge_result(
        result,
        crate::safety::journal_append(journal_path, &journal).map_err(|error| error.to_string()),
    )
}

#[cfg(test)]
fn finish_purge_result(
    deletion: Result<(), String>,
    audit: Result<(), String>,
) -> Result<Option<String>, String> {
    deletion.map(|()| audit.err())
}

/// Re-plan and remove only explicitly approved, exact regenerable caches.
pub fn execute(
    home: &Path,
    applications: &Path,
    podman_bin: &Path,
    requested: &[ProviderCacheCleanupRequest],
    approved_plan_fingerprint: &str,
    confirm_plan_fingerprint: &str,
    confirmation_phrase: &str,
    rationale: &str,
    journal_path: &Path,
    receipt_dir: &Path,
    mode: ProviderCacheCleanupMode,
    executed_at_ms: u64,
) -> Result<ProviderCacheCleanupResult, String> {
    if requested.len() > MAX_CLEANUP_REQUESTS {
        return Err("provider-cache-cleanup-request-count-exceeds-limit".into());
    }
    if requested.is_empty() || !valid_rationale(rationale) || executed_at_ms == 0 {
        return Err("provider-cache-cleanup-request-invalid".into());
    }
    if mode == ProviderCacheCleanupMode::PermanentPurge {
        return Err("provider-cache-identity-bound-permanent-delete-unavailable".into());
    }
    let current = plan_with_runtime(home, applications, podman_bin, executed_at_ms);
    let expected_phrase = current.trash_approval_phrase.as_deref();
    if !current.evidence_complete
        || current.plan_fingerprint != approved_plan_fingerprint
        || current.plan_fingerprint != confirm_plan_fingerprint
        || expected_phrase != Some(confirmation_phrase)
    {
        return Err("provider-cache-cleanup-plan-stale-or-unapproved".into());
    }
    let mut seen = BTreeSet::new();
    let mut selected = Vec::with_capacity(requested.len());
    for request in requested {
        if !seen.insert(&request.path) {
            return Err("provider-cache-cleanup-duplicate-request".into());
        }
        let candidate = current
            .candidates
            .iter()
            .find(|candidate| {
                candidate.path == request.path
                    && candidate.evidence_fingerprint == request.evidence_fingerprint
                    && candidate.object_id == request.object_id
            })
            .ok_or("provider-cache-cleanup-candidate-changed")?;
        selected.push(candidate.clone());
    }
    let receipt = write_immutable_receipt(
        receipt_dir,
        &current,
        requested,
        rationale,
        ProviderCacheCleanupMode::Trash,
        confirmation_phrase,
        executed_at_ms,
    )?;
    let mut items = Vec::with_capacity(selected.len());
    for candidate in selected {
        let result = candidate_content_still_matches(&candidate).and_then(|()| {
            let active =
                crate::cloud_local_eviction::observe_path_active_use(Path::new(&candidate.path));
            if !active_use_safe(&active) {
                return Err("provider-cache-active-use-or-provider-evidence-gap".into());
            }
            candidate_content_still_matches(&candidate)?;
            crate::safety::trash_delete_if_identity(
                Path::new(&candidate.path),
                &candidate.object_id,
                candidate.logical_bytes,
                journal_path,
                executed_at_ms,
            )
            .map_err(|error| error.to_string())
            .map(|()| None)
        });
        let (outcome, audit_error) = result.map_or_else(
            |error| (Err(error), None),
            |audit_error| (Ok(()), audit_error),
        );
        items.push(ProviderCacheCleanupItemResult {
            path: candidate.path,
            completed: outcome.is_ok(),
            error: outcome.err(),
            audit_error,
        });
    }
    let completed_count = items.iter().filter(|item| item.completed).count();
    Ok(ProviderCacheCleanupResult {
        plan_fingerprint: current.plan_fingerprint,
        requested_count: requested.len(),
        completed_count,
        executed_at_ms,
        rationale: rationale.into(),
        mode: ProviderCacheCleanupMode::Trash,
        immutable_receipt_path: receipt.to_string_lossy().into_owned(),
        items,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_use_requires_the_complete_empty_bounded_observation() {
        let mut evidence = crate::cloud_local_eviction::ActiveUseEvidence {
            method: "lsof-fp+ps-command".into(),
            evidence_complete: true,
            active: false,
            observed_pids: Vec::new(),
            results_truncated: false,
            error: None,
        };
        assert!(active_use_safe(&evidence));
        evidence.observed_pids.push(42);
        assert!(!active_use_safe(&evidence));
        evidence.observed_pids.clear();
        evidence.method = "unrecognized".into();
        assert!(!active_use_safe(&evidence));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn edge_plan_retains_installed_version_and_separates_crx_cache() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let apps = temp.path().join("Applications");
        for (base, version) in [
            (apps.join("Microsoft Edge.app"), "2.0"),
            (
                home.join("Library/Application Support/Microsoft/EdgeUpdater/apps/msedge-stable/1.0/Microsoft Edge.app"),
                "1.0",
            ),
            (
                home.join("Library/Application Support/Microsoft/EdgeUpdater/apps/msedge-stable/2.0/Microsoft Edge.app"),
                "2.0",
            ),
        ] {
            fs::create_dir_all(base.join("Contents")).unwrap();
            let mut dictionary = plist::Dictionary::new();
            dictionary.insert("CFBundleShortVersionString".into(), version.into());
            plist::Value::Dictionary(dictionary)
                .to_file_xml(base.join("Contents/Info.plist"))
                .unwrap();
        }
        let crx = home.join("Library/Application Support/Microsoft/EdgeUpdater/crx_cache");
        fs::create_dir_all(&crx).unwrap();
        fs::write(crx.join("payload"), b"cache").unwrap();
        let plan = plan_with_runtime(&home, &apps, Path::new("/missing/podman"), 1);
        assert_eq!(
            plan.candidates
                .iter()
                .filter(|item| item.kind == ProviderCacheKind::EdgeSupersededInstalledCopy)
                .count(),
            1
        );
        assert_eq!(
            plan.candidates
                .iter()
                .filter(|item| item.kind == ProviderCacheKind::EdgeCrxCache)
                .count(),
            1
        );
        assert!(plan
            .candidates
            .iter()
            .all(|item| !item.path.contains("/2.0/")));
    }

    #[test]
    fn content_or_identity_change_changes_candidate_fingerprint() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("seed.raw.zst");
        fs::write(&path, b"first").unwrap();
        let first = candidate(
            ProviderCacheKind::PodmanMachineSeed,
            &path,
            "source".into(),
            Some(file_sha256(&path).unwrap()),
        )
        .unwrap();
        fs::write(&path, b"second").unwrap();
        let second = candidate(
            ProviderCacheKind::PodmanMachineSeed,
            &path,
            "source".into(),
            Some(file_sha256(&path).unwrap()),
        )
        .unwrap();
        assert_ne!(first.evidence_fingerprint, second.evidence_fingerprint);
    }

    #[test]
    fn directory_activity_changes_candidate_fingerprint_before_execution() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("provider-cache");
        fs::create_dir(&path).unwrap();
        fs::write(path.join("first.tmp"), b"first").unwrap();
        let first = candidate(
            ProviderCacheKind::EdgeCrxCache,
            &path,
            "provider-cache".into(),
            None,
        )
        .unwrap();
        fs::write(path.join("second.tmp"), b"second").unwrap();
        let second = candidate(
            ProviderCacheKind::EdgeCrxCache,
            &path,
            "provider-cache".into(),
            None,
        )
        .unwrap();
        assert_ne!(first.evidence_fingerprint, second.evidence_fingerprint);
    }

    #[test]
    fn completed_purge_surfaces_post_delete_audit_failure_separately() {
        assert_eq!(
            finish_purge_result(Ok(()), Err("journal-full".into())),
            Ok(Some("journal-full".into()))
        );
        assert_eq!(
            finish_purge_result(Err("delete-failed".into()), Ok(())),
            Err("delete-failed".into())
        );
    }

    #[cfg(unix)]
    #[test]
    fn permanent_directory_purge_rechecks_manifest_before_recursive_delete() {
        let temp = tempfile::tempdir().unwrap();
        let cache = temp.path().join("cache");
        fs::create_dir(&cache).unwrap();
        fs::write(cache.join("first"), b"first").unwrap();
        fs::write(cache.join("second"), b"second").unwrap();
        let candidate = candidate(
            ProviderCacheKind::EdgeCrxCache,
            &cache,
            "recreation-source".into(),
            None,
        )
        .unwrap();
        let journal = temp.path().join("journal.jsonl");
        assert_eq!(permanently_purge_exact(&candidate, &journal, 1), Ok(None));
        assert!(!cache.exists());
        assert!(!temp
            .path()
            .join(format!(
                ".disksage-provider-cache-purge-1-{}",
                &candidate.evidence_fingerprint[..12]
            ))
            .exists());
    }

    #[cfg(unix)]
    #[test]
    fn permanent_directory_purge_restores_cache_when_manifest_changed() {
        let temp = tempfile::tempdir().unwrap();
        let cache = temp.path().join("cache");
        fs::create_dir(&cache).unwrap();
        fs::write(cache.join("first"), b"first").unwrap();
        let candidate = candidate(
            ProviderCacheKind::EdgeCrxCache,
            &cache,
            "recreation-source".into(),
            None,
        )
        .unwrap();
        fs::write(cache.join("changed"), b"changed").unwrap();
        assert_eq!(
            permanently_purge_exact(&candidate, &temp.path().join("journal.jsonl"), 1),
            Err("provider-cache-staged-manifest-changed".into())
        );
        assert!(cache.join("first").is_file());
        assert!(cache.join("changed").is_file());
    }

    #[cfg(unix)]
    #[test]
    fn receipt_records_the_approval_for_the_selected_mode_as_read_only_create_new() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let plan = ProviderCacheReclaimPlan {
            schema_version: SCHEMA_VERSION,
            platform: "test".into(),
            observed_at_ms: 1,
            installed_edge_version: None,
            podman_machine_present: false,
            podman_recreation_source: None,
            evidence_complete: true,
            candidates: Vec::new(),
            issues: Vec::new(),
            plan_fingerprint: "fingerprint".into(),
            exact_approval_phrase: Some("permanent phrase".into()),
            trash_approval_phrase: Some("trash phrase".into()),
        };
        let path = write_immutable_receipt(
            temp.path(),
            &plan,
            &[],
            "verified cache",
            ProviderCacheCleanupMode::Trash,
            "trash phrase",
            1,
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(value["approval_phrase"], "trash phrase");
        assert!(value.get("exact_approval_phrase").is_none());
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o400);
        assert_eq!(
            write_immutable_receipt(
                temp.path(),
                &plan,
                &[],
                "verified cache",
                ProviderCacheCleanupMode::Trash,
                "trash phrase",
                1,
            ),
            Err("provider-cache-receipt-object-bound-publication-failed".into())
        );
    }

    #[cfg(unix)]
    #[test]
    fn permanent_purge_requires_double_fingerprint_and_writes_receipt() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let apps = temp.path().join("Applications");
        let config = temp.path().join("podman-config");
        let active_raw = temp.path().join("active.raw");
        fs::create_dir_all(&config).unwrap();
        fs::write(&active_raw, b"active").unwrap();
        fs::write(
            config.join("podman-machine-default.json"),
            format!(r#"{{"ImagePath":{{"Path":"{}"}}}}"#, active_raw.display()),
        )
        .unwrap();
        let podman = temp.path().join("podman");
        fs::write(
            &podman,
            format!(
                "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'podman version test'; else printf '[{{\"Name\":\"podman-machine-default\",\"ConfigDir\":{{\"Path\":\"{}\"}}}}]'; fi\n",
                config.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&podman, fs::Permissions::from_mode(0o700)).unwrap();
        let seed_cache = home.join(".local/share/containers/podman/machine/applehv/cache");
        fs::create_dir_all(&seed_cache).unwrap();
        let seed_bytes = b"recreatable-podman-machine-seed";
        let seed_source = temp.path().join("seed.raw.zst");
        fs::write(&seed_source, seed_bytes).unwrap();
        let seed_digest = file_sha256(&seed_source).unwrap();
        fs::write(
            seed_cache.join(format!("{seed_digest}.raw.zst")),
            seed_bytes,
        )
        .unwrap();
        #[cfg(target_os = "macos")]
        {
            for (base, version) in [
                (apps.join("Microsoft Edge.app"), "2.0"),
                (
                    home.join("Library/Application Support/Microsoft/EdgeUpdater/apps/msedge-stable/1.0/Microsoft Edge.app"),
                    "1.0",
                ),
            ] {
                fs::create_dir_all(base.join("Contents")).unwrap();
                let mut dictionary = plist::Dictionary::new();
                dictionary.insert("CFBundleShortVersionString".into(), version.into());
                plist::Value::Dictionary(dictionary)
                    .to_file_xml(base.join("Contents/Info.plist"))
                    .unwrap();
            }
        }
        let plan = plan_with_runtime(&home, &apps, &podman, 1);
        assert!(plan.evidence_complete, "{:?}", plan.issues);
        let candidate = plan.candidates.first().unwrap();
        let request = ProviderCacheCleanupRequest {
            path: candidate.path.clone(),
            evidence_fingerprint: candidate.evidence_fingerprint.clone(),
            object_id: candidate.object_id.clone(),
        };
        let data = temp.path().join("data");
        fs::create_dir_all(&data).unwrap();
        assert_eq!(
            execute(
                &home,
                &apps,
                &podman,
                std::slice::from_ref(&request),
                &plan.plan_fingerprint,
                "wrong",
                plan.exact_approval_phrase.as_deref().unwrap(),
                "verified regenerable cache",
                &data.join("journal.jsonl"),
                &data.join("receipts"),
                ProviderCacheCleanupMode::PermanentPurge,
                2,
            )
            .unwrap_err(),
            "provider-cache-identity-bound-permanent-delete-unavailable"
        );
        assert!(Path::new(&request.path).exists());
        assert_eq!(
            execute(
                &home,
                &apps,
                &podman,
                &[request],
                &plan.plan_fingerprint,
                &plan.plan_fingerprint,
                plan.exact_approval_phrase.as_deref().unwrap(),
                "verified regenerable cache",
                &data.join("journal.jsonl"),
                &data.join("receipts"),
                ProviderCacheCleanupMode::PermanentPurge,
                3,
            )
            .unwrap_err(),
            "provider-cache-identity-bound-permanent-delete-unavailable"
        );
        assert!(fs::read_dir(data.join("receipts")).is_err());
        assert!(Path::new(&request.path).exists());
    }

    #[cfg(unix)]
    #[test]
    fn permanent_file_rollback_does_not_replace_recreated_provider_path() {
        let temp = tempfile::tempdir().unwrap();
        let seed = temp.path().join("seed.raw.zst");
        fs::write(&seed, b"approved-seed").unwrap();
        let candidate = ProviderCacheCandidate {
            kind: ProviderCacheKind::PodmanMachineSeed,
            path: seed.to_string_lossy().into_owned(),
            logical_bytes: fs::metadata(&seed).unwrap().len(),
            allocated_bytes: allocated_bytes(&fs::metadata(&seed).unwrap()),
            object_id: crate::safety::filesystem_object_id(&seed).unwrap(),
            content_manifest: file_sha256(&seed).unwrap(),
            evidence_fingerprint: "a".repeat(64),
            active_use: crate::cloud_local_eviction::ActiveUseEvidence {
                method: "lsof-fp+ps-command".into(),
                evidence_complete: true,
                active: false,
                observed_pids: Vec::new(),
                results_truncated: false,
                error: None,
            },
            recreation_source: "test-fixture".into(),
        };
        let staged = temp.path().join(format!(
            ".disksage-provider-cache-purge-9-{}",
            &candidate.evidence_fingerprint[..12]
        ));
        let journal = temp.path().join("journal.jsonl");
        let result = permanently_purge_exact_with_after_stage(
            &candidate,
            &journal,
            9,
            |actual_staged, original| {
                assert_eq!(actual_staged, staged);
                fs::write(actual_staged, b"tampered-staged-seed").unwrap();
                fs::write(original, b"provider-recreated-seed").unwrap();
                Ok(())
            },
        );
        assert_eq!(
            result,
            Err("provider-cache-permanent-file-purge-restore-destination-exists".into())
        );
        assert_eq!(fs::read(&seed).unwrap(), b"provider-recreated-seed");
        assert_eq!(fs::read(&staged).unwrap(), b"tampered-staged-seed");
        let outcomes: Vec<String> = fs::read_to_string(journal)
            .unwrap()
            .lines()
            .map(|line| {
                serde_json::from_str::<serde_json::Value>(line).unwrap()["outcome"]
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect();
        assert_eq!(outcomes, ["pending", "error"]);
    }
}
