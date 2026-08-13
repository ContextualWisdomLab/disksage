//! Dynamic, machine-readable ADR state for one cloud offload goal.
//!
//! The Markdown ADR documents the policy. This latest snapshot records the decision made by the
//! running application after each provider attestation, so the goal and its evidence cannot drift
//! silently between an operator view and the persisted receipt.

use crate::cloud_transfer::{CloudOffloadGoalState, ProviderSyncState};
use crate::provider_evidence::ProviderSyncEvidenceRecord;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const CLOUD_ADR_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CloudOffloadAdrSnapshot {
    pub schema_version: u32,
    pub adr_id: String,
    pub receipt_id: String,
    pub goal_state: CloudOffloadGoalState,
    pub provider_sync_state: ProviderSyncState,
    pub sync_complete: bool,
    pub decision: String,
    pub consequences: Vec<String>,
    pub evidence_record_id: String,
    pub updated_at_ms: u64,
}

fn decision_for(goal_state: CloudOffloadGoalState, sync_state: ProviderSyncState) -> String {
    match goal_state {
        CloudOffloadGoalState::CopyVerified => "retain-source-after-copy".into(),
        CloudOffloadGoalState::PendingProviderSync => {
            format!("retain-source-provider-state-{}", sync_state.as_str())
        }
        CloudOffloadGoalState::ProviderSyncConfirmed => {
            "retain-source-eviction-gate-pending".into()
        }
        CloudOffloadGoalState::EvictionReady => "source-eviction-permit-issued".into(),
    }
}

/// Build the current ADR snapshot from the same evidence used by the eviction gate.
pub fn snapshot_from_evidence(
    record: &ProviderSyncEvidenceRecord,
    goal_state: CloudOffloadGoalState,
    updated_at_ms: u64,
) -> CloudOffloadAdrSnapshot {
    let evidence = &record.evidence;
    let decision = decision_for(goal_state, evidence.sync_state);
    let mut consequences = vec!["source-retained".into()];
    if goal_state == CloudOffloadGoalState::EvictionReady {
        consequences.push("explicit-trash-step-may-proceed".into());
    } else {
        consequences.push("eviction-blocked-until-provider-proof".into());
    }
    CloudOffloadAdrSnapshot {
        schema_version: CLOUD_ADR_SCHEMA_VERSION,
        adr_id: format!("cloud-offload:{}", record.record_id),
        receipt_id: evidence.receipt_id.clone(),
        goal_state,
        provider_sync_state: evidence.sync_state,
        sync_complete: evidence.sync_complete,
        decision,
        consequences,
        evidence_record_id: record.record_id.clone(),
        updated_at_ms,
    }
}

fn secure_directory(directory: &Path) -> Result<(), String> {
    std::fs::create_dir_all(directory)
        .map_err(|_| "cloud-adr-directory-create-failed".to_string())?;
    let metadata = std::fs::symlink_metadata(directory)
        .map_err(|_| "cloud-adr-directory-metadata-failed".to_string())?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("cloud-adr-directory-unsafe".into());
    }
    Ok(())
}

/// Atomically replace the latest snapshot for a receipt. The write contains no source paths or
/// credentials; the immutable provider evidence remains the authority for hashes and timestamps.
pub fn write_latest_snapshot(
    directory: &Path,
    snapshot: &CloudOffloadAdrSnapshot,
) -> Result<PathBuf, String> {
    secure_directory(directory)?;
    let path = directory.join(format!("{}-latest.json", snapshot.receipt_id));
    let temporary = directory.join(format!(
        ".{}-{}-{}-latest.json.tmp",
        snapshot.receipt_id,
        snapshot.updated_at_ms,
        std::process::id()
    ));
    let encoded =
        serde_json::to_vec_pretty(snapshot).map_err(|_| "cloud-adr-json-invalid".to_string())?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|_| "cloud-adr-temp-create-failed".to_string())?;
    file.write_all(&encoded)
        .map_err(|_| "cloud-adr-write-failed".to_string())?;
    file.sync_all()
        .map_err(|_| "cloud-adr-sync-failed".to_string())?;
    drop(file);
    if std::fs::rename(&temporary, &path).is_err() {
        let _ = std::fs::remove_file(&temporary);
        return Err("cloud-adr-rename-failed".into());
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_tracks_pending_upload_without_authorizing_eviction() {
        let record = ProviderSyncEvidenceRecord {
            version: 1,
            record_id: "a".repeat(64),
            evidence: crate::cloud_transfer::ProviderSyncEvidence {
                receipt_id: "b".repeat(64),
                provider: crate::cloud::CloudProvider::Icloud,
                destination: "/cloud/file.bin".into(),
                observed_bytes: 1,
                destination_blake3: "c".repeat(64),
                confirmed_at_ms: 2,
                kind: crate::cloud_transfer::SyncEvidenceKind::ProviderNativeStatus,
                evidence_id: "foundation:test".into(),
                sync_complete: false,
                sync_state: ProviderSyncState::PendingUpload,
                remote_content: None,
            },
        };
        let snapshot =
            snapshot_from_evidence(&record, CloudOffloadGoalState::PendingProviderSync, 3);
        assert_eq!(
            snapshot.provider_sync_state,
            ProviderSyncState::PendingUpload
        );
        assert_eq!(
            snapshot.decision,
            "retain-source-provider-state-pending-upload"
        );
        assert!(snapshot.consequences.contains(&"source-retained".into()));
    }

    #[test]
    fn latest_snapshot_is_replaced_atomically_for_the_same_receipt() {
        let directory = tempfile::tempdir().unwrap();
        let mut snapshot = CloudOffloadAdrSnapshot {
            schema_version: CLOUD_ADR_SCHEMA_VERSION,
            adr_id: "cloud-offload:test".into(),
            receipt_id: "a".repeat(64),
            goal_state: CloudOffloadGoalState::PendingProviderSync,
            provider_sync_state: ProviderSyncState::PendingUpload,
            sync_complete: false,
            decision: "retain-source-provider-state-pending-upload".into(),
            consequences: vec!["source-retained".into()],
            evidence_record_id: "b".repeat(64),
            updated_at_ms: 1,
        };
        let path = write_latest_snapshot(directory.path(), &snapshot).unwrap();
        snapshot.goal_state = CloudOffloadGoalState::EvictionReady;
        snapshot.provider_sync_state = ProviderSyncState::Complete;
        snapshot.sync_complete = true;
        snapshot.updated_at_ms = 2;
        write_latest_snapshot(directory.path(), &snapshot).unwrap();
        let encoded = std::fs::read(path).unwrap();
        let current: CloudOffloadAdrSnapshot = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(current.goal_state, CloudOffloadGoalState::EvictionReady);
        assert_eq!(current.provider_sync_state, ProviderSyncState::Complete);
        assert!(!directory
            .path()
            .join(format!(
                ".{}-1-{}-latest.json.tmp",
                "a".repeat(64),
                std::process::id()
            ))
            .exists());
    }
}
