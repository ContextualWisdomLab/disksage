//! Exact-evidence cleanup for provider-owned, regenerable macOS caches.
//!
//! Only provider-owned caches with exact regeneration evidence are eligible. The active Podman raw
//! disk, Edge's installed/current versions, and OneDrive temporary data while its client is running
//! or cannot be observed are never candidates. Every purge is re-planned, explicitly approved,
//! active-use checked, identity-bound, journaled, and preceded by an immutable private receipt.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const SCHEMA_VERSION: u32 = 1;
const MAX_MANIFEST_ENTRIES: usize = 200_000;
const MANIFEST_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_RATIONALE_CHARS: usize = 1_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderCacheKind {
    PodmanMachineSeed,
    EdgeSupersededInstalledCopy,
    EdgeCrxCache,
    OneDriveTemporaryCache,
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

fn onedrive_runtime_safe(
    snapshot: &crate::provider_client_runtime::ProviderClientRuntimeSnapshot,
) -> bool {
    crate::provider_client_runtime::validate_provider_client_runtime_snapshot(snapshot).is_ok()
        && snapshot.provider == crate::cloud::CloudProvider::Onedrive
        && snapshot.process_observation_complete
        && snapshot.runtime_observed == Some(false)
        && snapshot.state == crate::provider_client_runtime::ProviderClientRuntimeState::NotObserved
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
        evidence_fingerprint: hex(&hasher.finalize()),
        active_use,
        recreation_source,
    })
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

fn podman_recreation_source(podman_bin: &Path, machine: &str) -> Result<String, String> {
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
    if name != Some(machine) || !image_path.as_ref().is_some_and(|path| path.is_file()) {
        return Err("podman-recreation-active-image-unconfirmed".into());
    }
    Ok(format!(
        "{}|{}|{}|{}",
        podman_bin.display(),
        version.trim(),
        machine,
        crate::safety::filesystem_object_id(image_path.as_ref().unwrap())
            .map_err(|_| "podman-recreation-active-image-identity-unavailable")?
    ))
}

/// Plan with explicit platform roots, used by the desktop command and audited headless CLI.
pub fn plan_with_runtime(
    home: &Path,
    applications: &Path,
    podman_bin: &Path,
    observed_at_ms: u64,
) -> ProviderCacheReclaimPlan {
    let onedrive_cache = home.join("Library/Application Support/OneDrive/tmp");
    #[cfg(not(coverage))]
    let onedrive_runtime = onedrive_cache.is_dir().then(|| {
        crate::provider_client_runtime::collect_provider_client_runtime(
            crate::cloud::CloudProvider::Onedrive,
            observed_at_ms,
        )
    });
    #[cfg(coverage)]
    let onedrive_runtime = None;
    plan_with_runtime_evidence(
        home,
        applications,
        podman_bin,
        observed_at_ms,
        onedrive_runtime.as_ref(),
    )
}

fn plan_with_runtime_evidence(
    home: &Path,
    applications: &Path,
    podman_bin: &Path,
    observed_at_ms: u64,
    onedrive_runtime: Option<&crate::provider_client_runtime::ProviderClientRuntimeSnapshot>,
) -> ProviderCacheReclaimPlan {
    let mut candidates = Vec::new();
    let mut issues = Vec::new();
    let onedrive_cache = home.join("Library/Application Support/OneDrive/tmp");
    if onedrive_cache.is_dir() {
        match onedrive_runtime.filter(|snapshot| onedrive_runtime_safe(snapshot)) {
            Some(snapshot) => match candidate(
                ProviderCacheKind::OneDriveTemporaryCache,
                &onedrive_cache,
                format!(
                    "onedrive-client-not-observed:{}",
                    snapshot.snapshot_fingerprint_sha256
                ),
                None,
            ) {
                Ok(value) => candidates.push(value),
                Err(error) => issues.push(format!("onedrive-temporary-cache:{error}")),
            },
            None => issues
                .push("onedrive-temporary-cache:provider-client-active-or-evidence-gap".into()),
        }
    }
    let edge_info = applications.join("Microsoft Edge.app/Contents/Info.plist");
    let installed_edge_version = plist_version(&edge_info);
    let edge_root = home.join("Library/Application Support/Microsoft/EdgeUpdater");
    if let Some(installed) = installed_edge_version.as_deref() {
        let versions = edge_root.join("apps/msedge-stable");
        if let Ok(entries) = fs::read_dir(&versions) {
            for entry in entries.flatten() {
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
        }
        let crx = edge_root.join("crx_cache");
        if crx.is_dir() {
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
    } else if edge_root.exists() {
        issues.push("edge-installed-version-unavailable".into());
    }

    let podman_source =
        podman_recreation_source(podman_bin, crate::podman_reclaim::DEFAULT_PODMAN_MACHINE);
    let podman_recreation_source = podman_source.as_ref().ok().cloned();
    if let Ok(source) = &podman_source {
        let cache = home.join(".local/share/containers/podman/machine/applehv/cache");
        if let Ok(entries) = fs::read_dir(cache) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().into_owned();
                let Some(key) = name.strip_suffix(".raw.zst") else {
                    continue;
                };
                if key.len() != 64 || !key.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                    issues.push(format!("podman-seed-cache-key-invalid:{name}"));
                    continue;
                }
                match file_sha256(&path).and_then(|digest| {
                    candidate(
                        ProviderCacheKind::PodmanMachineSeed,
                        &path,
                        source.clone(),
                        Some(digest),
                    )
                }) {
                    Ok(value) => candidates.push(value),
                    Err(error) => issues.push(format!("podman-seed-cache:{name}:{error}")),
                }
            }
        }
    } else if home
        .join(".local/share/containers/podman/machine/applehv/cache")
        .exists()
    {
        issues.push(podman_source.unwrap_err());
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
    fs::create_dir_all(receipt_dir).map_err(|_| "provider-cache-receipt-directory-failed")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(receipt_dir, fs::Permissions::from_mode(0o700))
            .map_err(|_| "provider-cache-receipt-directory-permissions-failed")?;
    }
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
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&path)
        .map_err(|_| "provider-cache-receipt-create-new-failed")?;
    use std::io::Write;
    file.write_all(&body)
        .and_then(|()| file.sync_all())
        .map_err(|_| "provider-cache-receipt-write-failed")?;
    Ok(path)
}

fn permanently_purge_exact(
    candidate: &ProviderCacheCandidate,
    journal_path: &Path,
    now_ms: u64,
) -> Result<Option<String>, String> {
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
    let result = if crate::safety::filesystem_object_id(&staged).ok().as_deref()
        != Some(&candidate.object_id)
    {
        let _ = fs::rename(&staged, path);
        Err("provider-cache-staged-identity-changed".to_string())
    } else {
        let metadata = fs::symlink_metadata(&staged)
            .map_err(|_| "provider-cache-staged-metadata-unavailable")?;
        if metadata.is_dir() {
            fs::rename(&staged, path)
                .map_err(|_| "provider-cache-permanent-directory-purge-restore-failed")?;
            Err("provider-cache-permanent-directory-purge-disabled".to_string())
        } else {
            fs::remove_file(&staged).map_err(|_| {
                if fs::rename(&staged, path).is_err() {
                    "provider-cache-permanent-delete-failed-rollback-failed".to_string()
                } else {
                    "provider-cache-permanent-delete-failed".to_string()
                }
            })
        }
    };
    journal.outcome = if result.is_ok() { "ok" } else { "error" }.into();
    finish_purge_result(
        result,
        crate::safety::journal_append(journal_path, &journal).map_err(|error| error.to_string()),
    )
}

fn finish_purge_result(
    deletion: Result<(), String>,
    audit: Result<(), String>,
) -> Result<Option<String>, String> {
    deletion.map(|()| audit.err())
}

/// Re-plan and permanently remove only explicitly approved, exact regenerable caches.
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
    if requested.is_empty() || !valid_rationale(rationale) || executed_at_ms == 0 {
        return Err("provider-cache-cleanup-request-invalid".into());
    }
    let current = plan_with_runtime(home, applications, podman_bin, executed_at_ms);
    let expected_phrase = match mode {
        ProviderCacheCleanupMode::Trash => current.trash_approval_phrase.as_deref(),
        ProviderCacheCleanupMode::PermanentPurge => current.exact_approval_phrase.as_deref(),
    };
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
        mode,
        confirmation_phrase,
        executed_at_ms,
    )?;
    let mut items = Vec::with_capacity(selected.len());
    for candidate in selected {
        let active =
            crate::cloud_local_eviction::observe_path_active_use(Path::new(&candidate.path));
        #[cfg(not(coverage))]
        let provider_runtime_safe = candidate.kind != ProviderCacheKind::OneDriveTemporaryCache
            || onedrive_runtime_safe(
                &crate::provider_client_runtime::collect_provider_client_runtime(
                    crate::cloud::CloudProvider::Onedrive,
                    executed_at_ms,
                ),
            );
        #[cfg(coverage)]
        let provider_runtime_safe = candidate.kind != ProviderCacheKind::OneDriveTemporaryCache;
        let (outcome, audit_error) = if active_use_safe(&active) && provider_runtime_safe {
            match mode {
                ProviderCacheCleanupMode::Trash => crate::safety::trash_delete_if_identity(
                    Path::new(&candidate.path),
                    &candidate.object_id,
                    candidate.logical_bytes,
                    journal_path,
                    executed_at_ms,
                )
                .map_err(|error| error.to_string())
                .map(|()| None),
                ProviderCacheCleanupMode::PermanentPurge => {
                    permanently_purge_exact(&candidate, journal_path, executed_at_ms)
                }
            }
        } else {
            Err("provider-cache-active-use-or-provider-evidence-gap".into())
        }
        .map_or_else(
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
        mode,
        immutable_receipt_path: receipt.to_string_lossy().into_owned(),
        items,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn onedrive_runtime(
        process_names: Option<&[u8]>,
    ) -> crate::provider_client_runtime::ProviderClientRuntimeSnapshot {
        crate::provider_client_runtime::assess_provider_client_runtime(
            crate::cloud::CloudProvider::Onedrive,
            process_names,
            1,
        )
    }

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

    #[test]
    fn onedrive_temporary_cache_fails_closed_while_client_runs_or_evidence_is_missing() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let cache = home.join("Library/Application Support/OneDrive/tmp");
        fs::create_dir_all(&cache).unwrap();
        fs::write(cache.join("provider.tmp"), b"in-flight").unwrap();
        let apps = temp.path().join("Applications");

        for runtime in [
            onedrive_runtime(Some(b"OneDrive\n")),
            onedrive_runtime(None),
        ] {
            let plan = plan_with_runtime_evidence(
                &home,
                &apps,
                Path::new("/missing/podman"),
                1,
                Some(&runtime),
            );
            assert!(plan.candidates.is_empty());
            assert!(!plan.evidence_complete);
            assert_eq!(
                plan.issues,
                ["onedrive-temporary-cache:provider-client-active-or-evidence-gap"]
            );
            assert!(plan.exact_approval_phrase.is_none());
            assert!(plan.trash_approval_phrase.is_none());
        }
    }

    #[test]
    fn onedrive_temporary_cache_requires_complete_process_absence_evidence() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let cache = home.join("Library/Application Support/OneDrive/tmp");
        fs::create_dir_all(&cache).unwrap();
        fs::write(cache.join("provider.tmp"), b"stable-cache").unwrap();
        let runtime = onedrive_runtime(Some(b"Finder\n"));
        let plan = plan_with_runtime_evidence(
            &home,
            &temp.path().join("Applications"),
            Path::new("/missing/podman"),
            1,
            Some(&runtime),
        );
        assert!(plan.evidence_complete, "{:?}", plan.issues);
        assert_eq!(plan.candidates.len(), 1);
        assert_eq!(
            plan.candidates[0].kind,
            ProviderCacheKind::OneDriveTemporaryCache
        );
        assert!(plan.candidates[0]
            .recreation_source
            .ends_with(&runtime.snapshot_fingerprint_sha256));
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
            ProviderCacheKind::OneDriveTemporaryCache,
            &path,
            "provider-not-observed".into(),
            None,
        )
        .unwrap();
        fs::write(path.join("second.tmp"), b"second").unwrap();
        let second = candidate(
            ProviderCacheKind::OneDriveTemporaryCache,
            &path,
            "provider-not-observed".into(),
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
    fn permanent_directory_purge_is_disabled_before_recursive_delete() {
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
        let error = permanently_purge_exact(&candidate, &journal, 1).unwrap_err();
        assert_eq!(error, "provider-cache-permanent-directory-purge-disabled");
        assert!(cache.is_dir());
        assert_eq!(fs::read(cache.join("first")).unwrap(), b"first");
        assert_eq!(fs::read(cache.join("second")).unwrap(), b"second");
        assert!(!temp
            .path()
            .join(format!(
                ".disksage-provider-cache-purge-1-{}",
                &candidate.evidence_fingerprint[..12]
            ))
            .exists());
    }

    #[test]
    fn receipt_records_the_approval_for_the_selected_mode() {
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
        let value: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert_eq!(value["approval_phrase"], "trash phrase");
        assert!(value.get("exact_approval_phrase").is_none());
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
            "provider-cache-cleanup-plan-stale-or-unapproved"
        );
        assert!(Path::new(&request.path).exists());
        let result = execute(
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
        .unwrap();
        assert_eq!(result.completed_count, 1);
        assert!(Path::new(&result.immutable_receipt_path).is_file());
        assert!(!Path::new(&result.items[0].path).exists());
    }
}
