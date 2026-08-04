//! Local provider-client runtime evidence for fail-closed cloud copies.
//!
//! A readable File Provider root is not proof that its vendor client is currently running.
//! This module observes only bounded process names and emits no command line, local path, account
//! identifier, token, remote-capacity claim, or synchronization claim.

use crate::cloud::CloudProvider;
use sha2::{Digest, Sha256};

#[cfg(not(coverage))]
use std::io::Read;
#[cfg(not(coverage))]
use std::process::{Command, Stdio};
#[cfg(not(coverage))]
use std::time::{Duration, Instant};

const SNAPSHOT_VERSION: u32 = 1;
const SNAPSHOT_SCHEMA_KIND: &str = "disksage.provider-client-runtime";
#[cfg(not(coverage))]
const PROCESS_OUTPUT_LIMIT: u64 = 64 * 1024;
#[cfg(not(coverage))]
const PROCESS_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ProviderClientRuntimeSnapshot {
    pub version: u32,
    pub schema_kind: &'static str,
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
        schema_kind: SNAPSHOT_SCHEMA_KIND,
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
        schema_kind: SNAPSHOT_SCHEMA_KIND,
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
        schema_kind: SNAPSHOT_SCHEMA_KIND,
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
    if snapshot.copy_prerequisite_met {
        Ok(())
    } else {
        Err(snapshot
            .blocker
            .clone()
            .unwrap_or_else(|| "provider-client-runtime-verification-required".into()))
    }
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
        let names = b"Finder\nOneDrive Sync Service\nnot-google-drive-helper\n";
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
}
