//! Local provider-client runtime evidence for fail-closed cloud copies.
//!
//! A readable File Provider root is not proof that its vendor client is currently running.
//! This module observes only bounded process names and emits no command line, local path, account
//! identifier, token, remote-capacity claim, or synchronization claim.

use crate::cloud::CloudProvider;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[cfg(not(coverage))]
use std::io::Read;
#[cfg(not(coverage))]
use std::io::Write;
#[cfg(not(coverage))]
use std::process::{Command, Stdio};
#[cfg(not(coverage))]
use std::time::{Duration, Instant};

const SNAPSHOT_VERSION: u32 = 1;
const SNAPSHOT_SCHEMA_KIND: &str = "disksage.provider-client-runtime";
pub const PROVIDER_CLIENT_RUNTIME_EVIDENCE_DIRECTORY: &str = "provider-client-runtime-evidence";
const MAX_PERSISTED_RUNTIME_SNAPSHOTS: usize = 128;
const MAX_PERSISTED_RUNTIME_SNAPSHOT_BYTES: usize = 64 * 1024;
#[cfg(not(coverage))]
const PROCESS_OUTPUT_LIMIT: u64 = 64 * 1024;
#[cfg(not(coverage))]
const PROCESS_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderClientRuntimeEvidenceKind {
    SystemFileProvider,
    BoundedProcessList,
    Unavailable,
}

impl ProviderClientRuntimeEvidenceKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::SystemFileProvider => "system-file-provider",
            Self::BoundedProcessList => "bounded-process-list",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderClientRuntimeState {
    ManagedBySystem,
    Running,
    NotObserved,
    EvidenceUnavailable,
}

impl ProviderClientRuntimeState {
    fn as_str(self) -> &'static str {
        match self {
            Self::ManagedBySystem => "managed-by-system",
            Self::Running => "running",
            Self::NotObserved => "not-observed",
            Self::EvidenceUnavailable => "evidence-unavailable",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderClientRuntimeSnapshot {
    pub version: u32,
    pub schema_kind: String,
    pub provider: CloudProvider,
    pub observed_at_ms: u64,
    pub evidence_kind: ProviderClientRuntimeEvidenceKind,
    pub state: ProviderClientRuntimeState,
    pub process_observation_complete: bool,
    pub runtime_observed: Option<bool>,
    /// This is one local prerequisite only. It is not an overall cloud-copy readiness claim.
    pub copy_prerequisite_met: bool,
    pub blocker: Option<String>,
    pub notices: Vec<String>,
    pub snapshot_fingerprint_sha256: String,
    pub raw_process_names_included: bool,
    pub local_paths_included: bool,
    pub remote_capacity_verified: bool,
    pub remote_sync_attested: bool,
    pub cloud_write_executed: bool,
}

fn snapshot_fingerprint(snapshot: &ProviderClientRuntimeSnapshot) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"disksage.provider-client-runtime\0v1\0");
    hasher.update(snapshot.provider.as_str().as_bytes());
    hasher.update([0]);
    hasher.update(snapshot.observed_at_ms.to_le_bytes());
    hasher.update(snapshot.evidence_kind.as_str().as_bytes());
    hasher.update([0]);
    hasher.update(snapshot.state.as_str().as_bytes());
    hasher.update([
        snapshot.process_observation_complete as u8,
        snapshot.runtime_observed.unwrap_or_default() as u8,
        snapshot.runtime_observed.is_some() as u8,
        snapshot.copy_prerequisite_met as u8,
    ]);
    if let Some(blocker) = &snapshot.blocker {
        hasher.update((blocker.len() as u64).to_le_bytes());
        hasher.update(blocker.as_bytes());
    } else {
        hasher.update(0u64.to_le_bytes());
    }
    let digest = hasher.finalize();
    let mut encoded = String::with_capacity(digest.len() * 2);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in digest {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

fn finish_snapshot(mut snapshot: ProviderClientRuntimeSnapshot) -> ProviderClientRuntimeSnapshot {
    snapshot.snapshot_fingerprint_sha256 = snapshot_fingerprint(&snapshot);
    snapshot
}

fn system_managed_snapshot(
    provider: CloudProvider,
    observed_at_ms: u64,
) -> ProviderClientRuntimeSnapshot {
    finish_snapshot(ProviderClientRuntimeSnapshot {
        version: SNAPSHOT_VERSION,
        schema_kind: SNAPSHOT_SCHEMA_KIND.into(),
        provider,
        observed_at_ms,
        evidence_kind: ProviderClientRuntimeEvidenceKind::SystemFileProvider,
        state: ProviderClientRuntimeState::ManagedBySystem,
        process_observation_complete: true,
        runtime_observed: Some(true),
        copy_prerequisite_met: true,
        blocker: None,
        notices: vec![
            "provider-client-runtime-prerequisite-only".into(),
            "remote-capacity-and-sync-still-required".into(),
        ],
        snapshot_fingerprint_sha256: String::new(),
        raw_process_names_included: false,
        local_paths_included: false,
        remote_capacity_verified: false,
        remote_sync_attested: false,
        cloud_write_executed: false,
    })
}

fn unavailable_snapshot(
    provider: CloudProvider,
    observed_at_ms: u64,
) -> ProviderClientRuntimeSnapshot {
    finish_snapshot(ProviderClientRuntimeSnapshot {
        version: SNAPSHOT_VERSION,
        schema_kind: SNAPSHOT_SCHEMA_KIND.into(),
        provider,
        observed_at_ms,
        evidence_kind: ProviderClientRuntimeEvidenceKind::Unavailable,
        state: ProviderClientRuntimeState::EvidenceUnavailable,
        process_observation_complete: false,
        runtime_observed: None,
        copy_prerequisite_met: false,
        blocker: Some("provider-client-runtime-evidence-unavailable".into()),
        notices: vec![
            "provider-client-runtime-prerequisite-only".into(),
            "absence-of-evidence-is-not-process-absence".into(),
            "remote-capacity-and-sync-still-required".into(),
        ],
        snapshot_fingerprint_sha256: String::new(),
        raw_process_names_included: false,
        local_paths_included: false,
        remote_capacity_verified: false,
        remote_sync_attested: false,
        cloud_write_executed: false,
    })
}

fn process_name_matches(provider: CloudProvider, name: &str) -> bool {
    let name = name.trim();
    match provider {
        CloudProvider::Icloud => false,
        CloudProvider::Onedrive => {
            name.eq_ignore_ascii_case("OneDrive")
                || name.eq_ignore_ascii_case("OneDrive Sync Service")
        }
        CloudProvider::GoogleDrive => {
            name.eq_ignore_ascii_case("Google Drive")
                || name.eq_ignore_ascii_case("Google Drive File Stream")
                || name.eq_ignore_ascii_case("DriveFS")
                || name.eq_ignore_ascii_case("DFSFileProviderExtension")
        }
    }
}

/// Convert a bounded process-name observation into path-free provider-runtime evidence.
///
/// `None` means the process observation itself was unavailable. An empty but complete list means
/// the provider client was not observed and therefore fails the copy prerequisite.
pub fn assess_provider_client_runtime(
    provider: CloudProvider,
    process_names: Option<&[u8]>,
    observed_at_ms: u64,
) -> ProviderClientRuntimeSnapshot {
    if provider == CloudProvider::Icloud {
        return system_managed_snapshot(provider, observed_at_ms);
    }
    let Some(process_names) = process_names else {
        return unavailable_snapshot(provider, observed_at_ms);
    };
    let Ok(process_names) = std::str::from_utf8(process_names) else {
        return unavailable_snapshot(provider, observed_at_ms);
    };
    let running = process_names
        .lines()
        .any(|name| process_name_matches(provider, name));
    finish_snapshot(ProviderClientRuntimeSnapshot {
        version: SNAPSHOT_VERSION,
        schema_kind: SNAPSHOT_SCHEMA_KIND.into(),
        provider,
        observed_at_ms,
        evidence_kind: ProviderClientRuntimeEvidenceKind::BoundedProcessList,
        state: if running {
            ProviderClientRuntimeState::Running
        } else {
            ProviderClientRuntimeState::NotObserved
        },
        process_observation_complete: true,
        runtime_observed: Some(running),
        copy_prerequisite_met: running,
        blocker: (!running).then(|| "provider-client-runtime-not-observed".into()),
        notices: vec![
            "provider-client-runtime-prerequisite-only".into(),
            "process-presence-does-not-prove-account-authentication".into(),
            "remote-capacity-and-sync-still-required".into(),
        ],
        snapshot_fingerprint_sha256: String::new(),
        raw_process_names_included: false,
        local_paths_included: false,
        remote_capacity_verified: false,
        remote_sync_attested: false,
        cloud_write_executed: false,
    })
}

#[cfg(not(coverage))]
fn collect_macos_process_names() -> Result<Vec<u8>, String> {
    #[cfg(not(target_os = "macos"))]
    {
        return Err("provider-client-runtime-platform-unsupported".into());
    }
    #[cfg(target_os = "macos")]
    {
        let mut child = Command::new("/bin/ps")
            .args(["-Ac", "-o", "comm="])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| "provider-client-runtime-process-list-unavailable".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "provider-client-runtime-process-list-unavailable".to_string())?;
        let reader = std::thread::spawn(move || {
            let mut output = Vec::new();
            stdout
                .take(PROCESS_OUTPUT_LIMIT + 1)
                .read_to_end(&mut output)
                .map(|_| output)
        });
        let started = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if started.elapsed() < PROCESS_TIMEOUT => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = reader.join();
                    return Err("provider-client-runtime-process-list-timeout".into());
                }
                Err(_) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = reader.join();
                    return Err("provider-client-runtime-process-list-unavailable".into());
                }
            }
        };
        let output = reader
            .join()
            .map_err(|_| "provider-client-runtime-process-list-unavailable".to_string())?
            .map_err(|_| "provider-client-runtime-process-list-unavailable".to_string())?;
        if !status.success() || output.len() as u64 > PROCESS_OUTPUT_LIMIT {
            return Err("provider-client-runtime-process-list-unavailable".into());
        }
        Ok(output)
    }
}

/// Inspect only local provider-client runtime state. No provider API or cloud root is touched.
#[cfg(not(coverage))]
pub fn collect_provider_client_runtime(
    provider: CloudProvider,
    observed_at_ms: u64,
) -> ProviderClientRuntimeSnapshot {
    if provider == CloudProvider::Icloud {
        return assess_provider_client_runtime(provider, Some(&[]), observed_at_ms);
    }
    let process_names = collect_macos_process_names().ok();
    assess_provider_client_runtime(provider, process_names.as_deref(), observed_at_ms)
}

/// Observe only the provider's primary desktop process.
///
/// Provider extensions may remain alive after the desktop app quits, so recovery operations must
/// not use the broader copy-prerequisite observation when waiting to run a vendor maintenance CLI.
#[cfg(not(coverage))]
pub(crate) fn collect_provider_primary_runtime(provider: CloudProvider) -> Option<bool> {
    if provider == CloudProvider::Icloud {
        return Some(true);
    }
    let expected = match provider {
        CloudProvider::Onedrive => "OneDrive",
        CloudProvider::GoogleDrive => "Google Drive",
        CloudProvider::Icloud => unreachable!(),
    };
    collect_macos_process_names().ok().and_then(|names| {
        std::str::from_utf8(&names).ok().map(|names| {
            names
                .lines()
                .any(|name| name.trim().eq_ignore_ascii_case(expected))
        })
    })
}

#[cfg(not(coverage))]
pub fn require_provider_client_runtime(
    provider: CloudProvider,
    observed_at_ms: u64,
) -> Result<ProviderClientRuntimeSnapshot, String> {
    let snapshot = collect_provider_client_runtime(provider, observed_at_ms);
    require_provider_client_runtime_snapshot(&snapshot)?;
    Ok(snapshot)
}

pub fn require_provider_client_runtime_snapshot(
    snapshot: &ProviderClientRuntimeSnapshot,
) -> Result<(), String> {
    validate_provider_client_runtime_snapshot(snapshot)?;
    if snapshot.copy_prerequisite_met {
        Ok(())
    } else {
        Err(snapshot
            .blocker
            .clone()
            .unwrap_or_else(|| "provider-client-runtime-verification-required".into()))
    }
}

pub fn validate_provider_client_runtime_snapshot(
    snapshot: &ProviderClientRuntimeSnapshot,
) -> Result<(), String> {
    if snapshot.version != SNAPSHOT_VERSION || snapshot.schema_kind != SNAPSHOT_SCHEMA_KIND {
        return Err("provider-client-runtime-schema-invalid".into());
    }
    if snapshot.raw_process_names_included
        || snapshot.local_paths_included
        || snapshot.remote_capacity_verified
        || snapshot.remote_sync_attested
        || snapshot.cloud_write_executed
    {
        return Err("provider-client-runtime-claim-invalid".into());
    }
    let shape_valid = match snapshot.state {
        ProviderClientRuntimeState::ManagedBySystem => {
            snapshot.provider == CloudProvider::Icloud
                && snapshot.evidence_kind == ProviderClientRuntimeEvidenceKind::SystemFileProvider
                && snapshot.process_observation_complete
                && snapshot.runtime_observed == Some(true)
                && snapshot.copy_prerequisite_met
                && snapshot.blocker.is_none()
        }
        ProviderClientRuntimeState::Running => {
            snapshot.provider != CloudProvider::Icloud
                && snapshot.evidence_kind == ProviderClientRuntimeEvidenceKind::BoundedProcessList
                && snapshot.process_observation_complete
                && snapshot.runtime_observed == Some(true)
                && snapshot.copy_prerequisite_met
                && snapshot.blocker.is_none()
        }
        ProviderClientRuntimeState::NotObserved => {
            snapshot.provider != CloudProvider::Icloud
                && snapshot.evidence_kind == ProviderClientRuntimeEvidenceKind::BoundedProcessList
                && snapshot.process_observation_complete
                && snapshot.runtime_observed == Some(false)
                && !snapshot.copy_prerequisite_met
                && snapshot.blocker.as_deref() == Some("provider-client-runtime-not-observed")
        }
        ProviderClientRuntimeState::EvidenceUnavailable => {
            snapshot.provider != CloudProvider::Icloud
                && snapshot.evidence_kind == ProviderClientRuntimeEvidenceKind::Unavailable
                && !snapshot.process_observation_complete
                && snapshot.runtime_observed.is_none()
                && !snapshot.copy_prerequisite_met
                && snapshot.blocker.as_deref()
                    == Some("provider-client-runtime-evidence-unavailable")
        }
    };
    if !shape_valid {
        return Err("provider-client-runtime-shape-invalid".into());
    }
    let expected_notices = match snapshot.state {
        ProviderClientRuntimeState::ManagedBySystem => vec![
            "provider-client-runtime-prerequisite-only".to_string(),
            "remote-capacity-and-sync-still-required".to_string(),
        ],
        ProviderClientRuntimeState::Running | ProviderClientRuntimeState::NotObserved => vec![
            "provider-client-runtime-prerequisite-only".to_string(),
            "process-presence-does-not-prove-account-authentication".to_string(),
            "remote-capacity-and-sync-still-required".to_string(),
        ],
        ProviderClientRuntimeState::EvidenceUnavailable => vec![
            "provider-client-runtime-prerequisite-only".to_string(),
            "absence-of-evidence-is-not-process-absence".to_string(),
            "remote-capacity-and-sync-still-required".to_string(),
        ],
    };
    if snapshot.notices != expected_notices {
        return Err("provider-client-runtime-notices-invalid".into());
    }
    if snapshot.snapshot_fingerprint_sha256.len() != 64
        || !snapshot
            .snapshot_fingerprint_sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || snapshot.snapshot_fingerprint_sha256 != snapshot_fingerprint(snapshot)
    {
        return Err("provider-client-runtime-fingerprint-invalid".into());
    }
    Ok(())
}

/// Persist one path-free provider-client observation for incident comparison.
///
/// The record is create-only, fsynced, and bounded to DiskSage-shaped files. It contains process
/// state only; command lines, local paths, account identifiers, cloud capacity, and sync claims
/// are rejected by the snapshot validator and are never written.
#[cfg(not(coverage))]
pub fn write_runtime_snapshot_evidence(
    app_data_dir: &Path,
    snapshot: &ProviderClientRuntimeSnapshot,
) -> Result<PathBuf, String> {
    validate_provider_client_runtime_snapshot(snapshot)?;
    let directory = runtime_evidence_directory(app_data_dir)?;
    let path = directory.join(format!(
        "{:020}-{}.json",
        snapshot.observed_at_ms, snapshot.snapshot_fingerprint_sha256
    ));
    let encoded = serde_json::to_vec_pretty(snapshot)
        .map_err(|_| "provider-client-runtime-evidence-encode-failed".to_string())?;
    if encoded.len() > MAX_PERSISTED_RUNTIME_SNAPSHOT_BYTES {
        return Err("provider-client-runtime-evidence-too-large".into());
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o400);
    }
    let mut file = options
        .open(&path)
        .map_err(|_| "provider-client-runtime-evidence-create-failed".to_string())?;
    let result = (|| -> Result<(), String> {
        file.write_all(&encoded)
            .and_then(|_| file.sync_all())
            .map_err(|_| "provider-client-runtime-evidence-write-failed".to_string())?;
        #[cfg(not(unix))]
        file.set_len(encoded.len() as u64)
            .map_err(|_| "provider-client-runtime-evidence-write-failed".to_string())?;
        #[cfg(unix)]
        std::fs::File::open(&directory)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| "provider-client-runtime-evidence-directory-sync-failed".to_string())?;
        Ok(())
    })();
    if let Err(error) = result {
        drop(file);
        let _ = std::fs::remove_file(&path);
        return Err(error);
    }
    prune_runtime_snapshot_evidence(&directory)?;
    Ok(path)
}

#[cfg(not(coverage))]
fn runtime_evidence_directory(app_data_dir: &Path) -> Result<PathBuf, String> {
    if !app_data_dir.is_absolute()
        || app_data_dir
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("provider-client-runtime-evidence-parent-invalid".into());
    }
    std::fs::create_dir_all(app_data_dir)
        .map_err(|_| "provider-client-runtime-evidence-parent-create-failed".to_string())?;
    let parent = std::fs::symlink_metadata(app_data_dir)
        .map_err(|_| "provider-client-runtime-evidence-parent-unavailable".to_string())?;
    if parent.file_type().is_symlink() || !parent.is_dir() {
        return Err("provider-client-runtime-evidence-parent-unsafe".into());
    }
    let directory = app_data_dir.join(PROVIDER_CLIENT_RUNTIME_EVIDENCE_DIRECTORY);
    std::fs::create_dir_all(&directory)
        .map_err(|_| "provider-client-runtime-evidence-directory-create-failed".to_string())?;
    let metadata = std::fs::symlink_metadata(&directory)
        .map_err(|_| "provider-client-runtime-evidence-directory-unavailable".to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("provider-client-runtime-evidence-directory-unsafe".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).map_err(
            |_| "provider-client-runtime-evidence-directory-permissions-failed".to_string(),
        )?;
    }
    Ok(directory)
}

#[cfg(not(coverage))]
fn is_runtime_snapshot_record_name(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(".json") else {
        return false;
    };
    let Some((timestamp, fingerprint)) = stem.split_once('-') else {
        return false;
    };
    timestamp.len() == 20
        && timestamp.bytes().all(|byte| byte.is_ascii_digit())
        && fingerprint.len() == 64
        && fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(not(coverage))]
fn prune_runtime_snapshot_evidence(directory: &Path) -> Result<(), String> {
    let mut records = std::fs::read_dir(directory)
        .map_err(|_| "provider-client-runtime-evidence-directory-read-failed".to_string())?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            is_runtime_snapshot_record_name(&name).then_some((name, entry.path()))
        })
        .collect::<Vec<_>>();
    records.sort_by(|left, right| left.0.cmp(&right.0));
    while records.len() > MAX_PERSISTED_RUNTIME_SNAPSHOTS {
        let (_, path) = records.remove(0);
        std::fs::remove_file(path)
            .map_err(|_| "provider-client-runtime-evidence-retention-failed")?;
    }
    Ok(())
}

pub fn attach_runtime_notice(notices: &mut Vec<String>, snapshot: &ProviderClientRuntimeSnapshot) {
    notices.retain(|notice| {
        !matches!(
            notice.as_str(),
            "provider-client-runtime-unverified"
                | "provider-client-runtime-observed"
                | "provider-client-runtime-not-observed"
                | "provider-client-runtime-evidence-unavailable"
        )
    });
    notices.push(
        match snapshot.state {
            ProviderClientRuntimeState::ManagedBySystem | ProviderClientRuntimeState::Running => {
                "provider-client-runtime-observed"
            }
            ProviderClientRuntimeState::NotObserved => "provider-client-runtime-not-observed",
            ProviderClientRuntimeState::EvidenceUnavailable => {
                "provider-client-runtime-evidence-unavailable"
            }
        }
        .into(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_only_exact_vendor_runtime_names() {
        let names = b"Finder\nOneDrive Sync Service\nnot-google-drive-helper\nnot-DFSFileProviderExtension-helper\n";
        let onedrive = assess_provider_client_runtime(CloudProvider::Onedrive, Some(names), 42);
        let google = assess_provider_client_runtime(CloudProvider::GoogleDrive, Some(names), 42);

        assert_eq!(onedrive.state, ProviderClientRuntimeState::Running);
        assert!(onedrive.copy_prerequisite_met);
        assert_eq!(google.state, ProviderClientRuntimeState::NotObserved);
        assert!(!google.copy_prerequisite_met);
        assert_eq!(
            google.blocker.as_deref(),
            Some("provider-client-runtime-not-observed")
        );
    }

    #[test]
    fn detects_google_drive_runtime_without_emitting_process_names() {
        let snapshot = assess_provider_client_runtime(
            CloudProvider::GoogleDrive,
            Some(b"Finder\nGoogle Drive\n"),
            7,
        );
        let encoded = serde_json::to_string(&snapshot).unwrap();

        assert_eq!(snapshot.state, ProviderClientRuntimeState::Running);
        assert!(snapshot.copy_prerequisite_met);
        assert!(!snapshot.raw_process_names_included);
        assert!(!encoded.contains("Finder"));
        assert!(!encoded.contains("Google Drive"));
    }

    #[test]
    fn detects_current_macos_google_file_provider_extension() {
        let snapshot = assess_provider_client_runtime(
            CloudProvider::GoogleDrive,
            Some(b"Finder\nDFSFileProviderExtension\n"),
            8,
        );
        let encoded = serde_json::to_string(&snapshot).unwrap();

        assert_eq!(snapshot.state, ProviderClientRuntimeState::Running);
        assert!(snapshot.copy_prerequisite_met);
        assert!(!snapshot.raw_process_names_included);
        assert!(!encoded.contains("DFSFileProviderExtension"));
    }

    #[test]
    fn unavailable_or_invalid_process_evidence_fails_closed() {
        for names in [None, Some(&[0xff][..])] {
            let snapshot = assess_provider_client_runtime(CloudProvider::Onedrive, names, 9);
            assert_eq!(
                snapshot.state,
                ProviderClientRuntimeState::EvidenceUnavailable
            );
            assert_eq!(snapshot.runtime_observed, None);
            assert!(!snapshot.copy_prerequisite_met);
            assert_eq!(
                snapshot.blocker.as_deref(),
                Some("provider-client-runtime-evidence-unavailable")
            );
        }
    }

    #[test]
    fn icloud_is_explicitly_system_managed_without_claiming_remote_state() {
        let snapshot = assess_provider_client_runtime(CloudProvider::Icloud, None, 11);

        assert_eq!(snapshot.state, ProviderClientRuntimeState::ManagedBySystem);
        assert!(snapshot.copy_prerequisite_met);
        assert!(!snapshot.remote_capacity_verified);
        assert!(!snapshot.remote_sync_attested);
        assert!(!snapshot.cloud_write_executed);
    }

    #[test]
    fn fingerprints_bind_provider_time_and_runtime_state() {
        let running =
            assess_provider_client_runtime(CloudProvider::GoogleDrive, Some(b"Google Drive\n"), 1);
        let stopped =
            assess_provider_client_runtime(CloudProvider::GoogleDrive, Some(b"Finder\n"), 1);
        let later =
            assess_provider_client_runtime(CloudProvider::GoogleDrive, Some(b"Google Drive\n"), 2);

        assert_eq!(running.snapshot_fingerprint_sha256.len(), 64);
        assert_ne!(
            running.snapshot_fingerprint_sha256,
            stopped.snapshot_fingerprint_sha256
        );
        assert_ne!(
            running.snapshot_fingerprint_sha256,
            later.snapshot_fingerprint_sha256
        );
    }

    #[test]
    fn runtime_notice_replaces_only_prior_runtime_status() {
        let mut notices = vec![
            "dry-run-only".into(),
            "provider-client-runtime-unverified".into(),
            "cloud-sync-unverified".into(),
        ];
        let snapshot =
            assess_provider_client_runtime(CloudProvider::GoogleDrive, Some(b"Finder\n"), 1);

        attach_runtime_notice(&mut notices, &snapshot);

        assert_eq!(
            notices,
            vec![
                "dry-run-only",
                "cloud-sync-unverified",
                "provider-client-runtime-not-observed"
            ]
        );
    }

    #[test]
    fn copy_prerequisite_rejects_missing_or_unavailable_runtime() {
        let running =
            assess_provider_client_runtime(CloudProvider::GoogleDrive, Some(b"Google Drive\n"), 1);
        let stopped =
            assess_provider_client_runtime(CloudProvider::GoogleDrive, Some(b"Finder\n"), 1);
        let unavailable = assess_provider_client_runtime(CloudProvider::GoogleDrive, None, 1);

        assert!(require_provider_client_runtime_snapshot(&running).is_ok());
        assert_eq!(
            require_provider_client_runtime_snapshot(&stopped).unwrap_err(),
            "provider-client-runtime-not-observed"
        );
        assert_eq!(
            require_provider_client_runtime_snapshot(&unavailable).unwrap_err(),
            "provider-client-runtime-evidence-unavailable"
        );
    }

    #[test]
    fn validation_rejects_forged_runtime_shape_claims_and_fingerprint() {
        let valid =
            assess_provider_client_runtime(CloudProvider::GoogleDrive, Some(b"Google Drive\n"), 1);
        assert!(validate_provider_client_runtime_snapshot(&valid).is_ok());

        let mut forged_claim = valid.clone();
        forged_claim.remote_sync_attested = true;
        assert_eq!(
            validate_provider_client_runtime_snapshot(&forged_claim).unwrap_err(),
            "provider-client-runtime-claim-invalid"
        );

        let mut forged_shape = valid.clone();
        forged_shape.runtime_observed = Some(false);
        assert_eq!(
            validate_provider_client_runtime_snapshot(&forged_shape).unwrap_err(),
            "provider-client-runtime-shape-invalid"
        );

        let mut forged_fingerprint = valid;
        forged_fingerprint.snapshot_fingerprint_sha256 = "0".repeat(64);
        assert_eq!(
            validate_provider_client_runtime_snapshot(&forged_fingerprint).unwrap_err(),
            "provider-client-runtime-fingerprint-invalid"
        );

        let mut forged_notices = assess_provider_client_runtime(
            CloudProvider::Onedrive,
            Some(b"OneDrive Sync Service\n"),
            42,
        );
        forged_notices.notices.pop();
        assert_eq!(
            validate_provider_client_runtime_snapshot(&forged_notices).unwrap_err(),
            "provider-client-runtime-notices-invalid"
        );
    }

    #[cfg(not(coverage))]
    #[test]
    fn runtime_evidence_is_path_free_create_only_and_bounded() {
        let temp = tempfile::tempdir().unwrap();
        let first =
            assess_provider_client_runtime(CloudProvider::GoogleDrive, Some(b"Google Drive\n"), 1);
        let first_path = write_runtime_snapshot_evidence(temp.path(), &first).unwrap();
        let encoded = std::fs::read(&first_path).unwrap();
        assert!(!String::from_utf8_lossy(&encoded).contains("Google Drive"));
        assert_eq!(
            write_runtime_snapshot_evidence(temp.path(), &first).unwrap_err(),
            "provider-client-runtime-evidence-create-failed"
        );

        for observed_at_ms in 2..=129 {
            let snapshot = assess_provider_client_runtime(
                CloudProvider::GoogleDrive,
                Some(b"Finder\n"),
                observed_at_ms,
            );
            write_runtime_snapshot_evidence(temp.path(), &snapshot).unwrap();
        }
        let records =
            std::fs::read_dir(temp.path().join(PROVIDER_CLIENT_RUNTIME_EVIDENCE_DIRECTORY))
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| {
                    entry
                        .file_name()
                        .to_str()
                        .is_some_and(is_runtime_snapshot_record_name)
                })
                .count();
        assert_eq!(records, MAX_PERSISTED_RUNTIME_SNAPSHOTS);
        assert!(!first_path.exists());
    }
}
