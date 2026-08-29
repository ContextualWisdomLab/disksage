//! Path-free assessment of OneDrive's provider-managed local cache.
//!
//! Collection adapters may inspect provider-owned storage read-only, but this module accepts only
//! aggregate facts. It never exposes item names, maps private blobs to user paths, or authorizes
//! deleting/resetting provider internals.

use crate::provider_global_sync::ProviderGlobalSyncState;
use serde::{Deserialize, Serialize};
#[cfg(all(target_os = "macos", not(coverage)))]
use std::path::{Path, PathBuf};
#[cfg(all(target_os = "macos", not(coverage)))]
use std::time::{Duration, Instant};

pub const ONEDRIVE_INTERNAL_PRESSURE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OneDriveInternalPressureState {
    Clear,
    ProviderBusy,
    ProviderSyncStalled,
    InternalPressure,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OneDriveInternalPressureObservation {
    pub observed_at_ms: u64,
    pub provider_cache_allocated_bytes: u64,
    pub provider_cache_file_count: u64,
    pub provider_cache_fingerprint: String,
    pub cache_scan_complete: bool,
    pub active_reader_writer_count: u64,
    pub active_use_evidence_complete: bool,
    pub global_sync_state: ProviderGlobalSyncState,
    pub provider_reported_local_disk_full: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OneDriveInternalPressureReport {
    pub schema_version: u32,
    pub state: OneDriveInternalPressureState,
    pub observed_at_ms: u64,
    pub provider_cache_allocated_bytes: u64,
    pub provider_cache_file_count: u64,
    pub evidence_complete: bool,
    pub blockers: Vec<String>,
    pub next_action: String,
    pub provider_internal_mutation_authorized: bool,
    pub provider_restart_authorized: bool,
}

/// Assess aggregate provider-cache pressure without inventing an age or size threshold.
///
/// A stall requires two complete observations separated by the caller's explicit service deadline.
/// Equal cache fingerprints alone never imply that the private blobs are orphaned.
pub fn assess(
    current: &OneDriveInternalPressureObservation,
    previous: Option<&OneDriveInternalPressureObservation>,
    stall_after_ms: Option<u64>,
) -> OneDriveInternalPressureReport {
    let evidence_complete = current.cache_scan_complete && current.active_use_evidence_complete;
    let stalled = previous
        .zip(stall_after_ms)
        .is_some_and(|(previous, deadline)| {
            deadline > 0
                && previous.cache_scan_complete
                && previous.active_use_evidence_complete
                && current
                    .observed_at_ms
                    .saturating_sub(previous.observed_at_ms)
                    >= deadline
                && current.provider_cache_fingerprint == previous.provider_cache_fingerprint
                && current.global_sync_state == ProviderGlobalSyncState::Pending
                && previous.global_sync_state == ProviderGlobalSyncState::Pending
        });
    let state = if !evidence_complete
        || current.global_sync_state == ProviderGlobalSyncState::Unavailable
    {
        OneDriveInternalPressureState::Unavailable
    } else if current.provider_reported_local_disk_full
        && current.provider_cache_allocated_bytes > 0
    {
        OneDriveInternalPressureState::InternalPressure
    } else if stalled {
        OneDriveInternalPressureState::ProviderSyncStalled
    } else if current.active_reader_writer_count > 0
        || current.global_sync_state == ProviderGlobalSyncState::Pending
    {
        OneDriveInternalPressureState::ProviderBusy
    } else {
        OneDriveInternalPressureState::Clear
    };
    let (blocker, next_action) = match state {
        OneDriveInternalPressureState::Clear => (
            "provider-native-lifecycle-safety-not-proven",
            "OneDrive 상태를 다시 확인한 뒤 동기화된 파일의 로컬 사본만 비우세요.",
        ),
        OneDriveInternalPressureState::ProviderBusy => (
            "provider-reader-writer-or-sync-active",
            "OneDrive 동기화가 끝날 때까지 앱을 열어 두고 다시 확인하세요.",
        ),
        OneDriveInternalPressureState::ProviderSyncStalled => (
            "provider-sync-stalled",
            "OneDrive 상태 화면에서 오류를 해결한 뒤 다시 확인하세요.",
        ),
        OneDriveInternalPressureState::InternalPressure => (
            "provider-internal-pressure",
            "다른 안전한 항목으로 여유 공간을 확보한 뒤 OneDrive 동기화를 다시 확인하세요.",
        ),
        OneDriveInternalPressureState::Unavailable => (
            "provider-pressure-evidence-incomplete",
            "OneDrive를 실행한 상태에서 진단을 다시 시도하세요.",
        ),
    };
    OneDriveInternalPressureReport {
        schema_version: ONEDRIVE_INTERNAL_PRESSURE_SCHEMA_VERSION,
        state,
        observed_at_ms: current.observed_at_ms,
        provider_cache_allocated_bytes: current.provider_cache_allocated_bytes,
        provider_cache_file_count: current.provider_cache_file_count,
        evidence_complete,
        blockers: vec![blocker.into(), "provider-internal-delete-forbidden".into()],
        next_action: next_action.into(),
        provider_internal_mutation_authorized: false,
        provider_restart_authorized: false,
    }
}

#[cfg(all(target_os = "macos", not(coverage)))]
fn provider_cache_root(home: &Path) -> Result<PathBuf, String> {
    if !home.is_absolute()
        || home
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err("onedrive-pressure-home-invalid".into());
    }
    let root = home.join(
        "Library/Group Containers/UBF8T346G9.OneDriveStandaloneSuite/.Dbfs.Dbfs_Personal.noindex",
    );
    let metadata = std::fs::symlink_metadata(&root)
        .map_err(|_| "onedrive-pressure-cache-unavailable".to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("onedrive-pressure-cache-root-invalid".into());
    }
    Ok(root)
}

#[cfg(all(target_os = "macos", not(coverage)))]
fn scan_cache(root: &Path) -> Result<(u64, u64, String), String> {
    scan_cache_with_limits(root, 100_000, Duration::from_secs(5))
}

#[cfg(all(target_os = "macos", not(coverage)))]
fn scan_cache_with_limits(
    root: &Path,
    max_entries: u64,
    max_duration: Duration,
) -> Result<(u64, u64, String), String> {
    use std::os::unix::fs::MetadataExt;
    let started = Instant::now();
    let mut stack = vec![root.to_path_buf()];
    let mut allocated = 0_u64;
    let mut files = 0_u64;
    let mut visited = 0_u64;
    let mut hasher = blake3::Hasher::new();
    while let Some(path) = stack.pop() {
        if visited >= max_entries || started.elapsed() >= max_duration {
            return Err("onedrive-pressure-cache-scan-bounded".into());
        }
        visited += 1;
        let metadata = std::fs::symlink_metadata(&path)
            .map_err(|_| "onedrive-pressure-cache-metadata-unavailable".to_string())?;
        if metadata.file_type().is_symlink() {
            return Err("onedrive-pressure-cache-symlink-rejected".into());
        }
        allocated = allocated.saturating_add(metadata.blocks().saturating_mul(512));
        if metadata.is_file() {
            files += 1;
            hasher.update(&metadata.len().to_le_bytes());
            hasher.update(&metadata.blocks().to_le_bytes());
            hasher.update(&metadata.mtime().to_le_bytes());
        } else if metadata.is_dir() {
            let mut children = Vec::new();
            let entries = std::fs::read_dir(&path)
                .map_err(|_| "onedrive-pressure-cache-directory-unreadable".to_string())?;
            let remaining = max_entries.saturating_sub(visited);
            for entry in entries {
                if started.elapsed() >= max_duration || children.len() as u64 >= remaining {
                    return Err("onedrive-pressure-cache-scan-bounded".into());
                }
                children.push(
                    entry.map_err(|_| "onedrive-pressure-cache-entry-unavailable".to_string())?,
                );
            }
            children.sort_by_key(std::fs::DirEntry::file_name);
            stack.extend(children.into_iter().rev().map(|entry| entry.path()));
        }
    }
    Ok((allocated, files, hasher.finalize().to_hex().to_string()))
}

/// Collect one bounded, read-only macOS observation. No provider path or item name is returned.
#[cfg(all(target_os = "macos", not(coverage)))]
pub fn collect(
    home: &Path,
    observed_at_ms: u64,
) -> Result<OneDriveInternalPressureObservation, String> {
    let root = provider_cache_root(home)?;
    let (allocated, files, fingerprint) = scan_cache(&root)?;
    let active = crate::git_worktree::active_use_evidence(&root, 5_000, 64, true);
    let global = crate::provider_global_sync::inspect_new_copy_admission(
        crate::cloud::CloudProvider::Onedrive,
    )?;
    Ok(OneDriveInternalPressureObservation {
        observed_at_ms,
        provider_cache_allocated_bytes: allocated,
        provider_cache_file_count: files,
        provider_cache_fingerprint: fingerprint,
        cache_scan_complete: true,
        active_reader_writer_count: active.observed_pids.len() as u64,
        active_use_evidence_complete: active.evidence_complete,
        global_sync_state: global.state,
        provider_reported_local_disk_full: global
            .blockers
            .iter()
            .any(|blocker| blocker == "provider-global-sync-local-disk-full"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation(at: u64) -> OneDriveInternalPressureObservation {
        OneDriveInternalPressureObservation {
            observed_at_ms: at,
            provider_cache_allocated_bytes: 13 * 1024 * 1024 * 1024,
            provider_cache_file_count: 10,
            provider_cache_fingerprint: "a".repeat(64),
            cache_scan_complete: true,
            active_reader_writer_count: 0,
            active_use_evidence_complete: true,
            global_sync_state: ProviderGlobalSyncState::Pending,
            provider_reported_local_disk_full: false,
        }
    }

    #[test]
    fn stall_requires_two_complete_observations_and_explicit_deadline() {
        let previous = observation(1_000);
        let current = observation(2_000);
        assert_eq!(
            assess(&current, Some(&previous), None).state,
            OneDriveInternalPressureState::ProviderBusy
        );
        assert_eq!(
            assess(&current, Some(&previous), Some(1_000)).state,
            OneDriveInternalPressureState::ProviderSyncStalled
        );
    }

    #[test]
    fn provider_disk_full_is_pressure_but_never_delete_or_restart_authority() {
        let mut current = observation(2_000);
        current.provider_reported_local_disk_full = true;
        let report = assess(&current, None, None);
        assert_eq!(
            report.state,
            OneDriveInternalPressureState::InternalPressure
        );
        assert!(!report.provider_internal_mutation_authorized);
        assert!(!report.provider_restart_authorized);
    }

    #[test]
    fn incomplete_active_use_evidence_fails_closed() {
        let mut current = observation(2_000);
        current.active_use_evidence_complete = false;
        assert_eq!(
            assess(&current, None, None).state,
            OneDriveInternalPressureState::Unavailable
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn one_directory_cannot_bypass_the_entry_cap() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("a"), b"a").unwrap();
        std::fs::write(temp.path().join("b"), b"b").unwrap();
        assert_eq!(
            scan_cache_with_limits(temp.path(), 2, Duration::from_secs(5)).unwrap_err(),
            "onedrive-pressure-cache-scan-bounded"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn queued_paths_consume_the_remaining_entry_budget() {
        assert_eq!(remaining_entry_budget(5, 2, 3), 0);
        assert_eq!(remaining_entry_budget(10, 2, 3), 5);
    }
}
