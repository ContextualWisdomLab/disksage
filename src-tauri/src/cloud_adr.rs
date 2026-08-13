//! Runtime cloud-offload ADR and Goal projections.
//!
//! Receipts and provider-evidence records remain the immutable authorities. These files are
//! replaceable, atomically written projections for the UI, agents, and reconciliation jobs.

use crate::cloud_transfer::{CloudCopyReceipt, CloudOffloadGoalState, ProviderSyncState};
use crate::provider_evidence::ProviderSyncEvidenceRecord;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

pub const CLOUD_ADR_SCHEMA_VERSION: u32 = 2;
pub const CLOUD_GOAL_SCHEMA_VERSION: u32 = 1;
const MAX_PROJECTION_BYTES: u64 = 256 * 1024;

// ponytail: one process-wide lock keeps low-volume projections ordered; use per-receipt locks if
// concurrent multi-account projection throughput ever becomes measurable.
static PROJECTION_WRITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

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
    pub evidence_record_id: Option<String>,
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CloudOffloadGoalSnapshot {
    pub schema_version: u32,
    pub goal_id: String,
    pub status: String,
    pub receipt_id: String,
    pub goal_state: CloudOffloadGoalState,
    pub provider_sync_state: ProviderSyncState,
    pub completion_gates: BTreeMap<String, bool>,
    pub safety_invariant: String,
    pub evidence_record_id: Option<String>,
    pub updated_at_ms: u64,
}

/// The latest replaceable projection state used when a fresh provider attestation is unavailable.
/// This is never an eviction permit; it only keeps reconciliation/UI state truthful.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CloudProjectionState {
    pub goal_state: CloudOffloadGoalState,
    pub provider_sync_state: ProviderSyncState,
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
        CloudOffloadGoalState::SourceEvicted => "source-moved-to-os-trash".into(),
    }
}

pub fn snapshot_from_evidence(
    record: &ProviderSyncEvidenceRecord,
    goal_state: CloudOffloadGoalState,
    updated_at_ms: u64,
) -> CloudOffloadAdrSnapshot {
    let evidence = &record.evidence;
    let mut consequences = if goal_state == CloudOffloadGoalState::SourceEvicted {
        vec!["source-in-os-trash-reversible".into()]
    } else {
        vec!["source-retained".into()]
    };
    if goal_state == CloudOffloadGoalState::SourceEvicted {
        consequences.push("explicit-trash-step-completed".into());
    } else if goal_state == CloudOffloadGoalState::EvictionReady {
        consequences.push("explicit-trash-step-may-proceed".into());
    } else {
        consequences.push("eviction-blocked-until-provider-proof".into());
    }
    CloudOffloadAdrSnapshot {
        schema_version: CLOUD_ADR_SCHEMA_VERSION,
        adr_id: format!("cloud-offload:{}", evidence.receipt_id),
        receipt_id: evidence.receipt_id.clone(),
        goal_state,
        provider_sync_state: evidence.sync_state,
        sync_complete: evidence.sync_complete,
        decision: decision_for(goal_state, evidence.sync_state),
        consequences,
        evidence_record_id: Some(record.record_id.clone()),
        updated_at_ms,
    }
}

/// Build the initial ADR projection immediately after a verified copy, before provider evidence
/// exists. Unknown provider state is explicit; no synthetic evidence record is created.
pub fn initial_adr_snapshot(
    receipt: &CloudCopyReceipt,
    updated_at_ms: u64,
) -> CloudOffloadAdrSnapshot {
    CloudOffloadAdrSnapshot {
        schema_version: CLOUD_ADR_SCHEMA_VERSION,
        adr_id: format!("cloud-offload:{}", receipt.receipt_id),
        receipt_id: receipt.receipt_id.clone(),
        goal_state: CloudOffloadGoalState::CopyVerified,
        provider_sync_state: ProviderSyncState::Unknown,
        sync_complete: false,
        decision: decision_for(
            CloudOffloadGoalState::CopyVerified,
            ProviderSyncState::Unknown,
        ),
        consequences: vec![
            "source-retained".into(),
            "eviction-blocked-until-provider-proof".into(),
        ],
        evidence_record_id: None,
        updated_at_ms,
    }
}

fn completion_gates(
    receipt: &CloudCopyReceipt,
    record: Option<&ProviderSyncEvidenceRecord>,
    goal_state: CloudOffloadGoalState,
) -> (BTreeMap<String, bool>, ProviderSyncState, Option<String>) {
    let lineage_bound = receipt.lineage.is_some() && receipt.lineage_fingerprint.is_some();
    let mut gates = BTreeMap::new();
    gates.insert("metadata-and-lineage-bound".into(), lineage_bound);
    gates.insert("copy-content-verified".into(), receipt.copy_verified);
    let Some(record) = record else {
        gates.insert("provider-sync-state-complete".into(), false);
        gates.insert("immutable-evidence-record-valid".into(), false);
        gates.insert("explicit-eviction-permit".into(), false);
        return (gates, ProviderSyncState::Unknown, None);
    };
    let evidence = &record.evidence;
    let evidence_valid = crate::provider_evidence::validate_sync_evidence_record(record).is_ok();
    let content_verified = receipt.copy_verified
        && receipt.bytes == evidence.observed_bytes
        && receipt.blake3 == evidence.destination_blake3;
    let provider_complete = evidence_valid
        && content_verified
        && evidence.sync_complete
        && evidence.sync_state.is_complete();
    let permit_issued = matches!(
        goal_state,
        CloudOffloadGoalState::EvictionReady | CloudOffloadGoalState::SourceEvicted
    );
    gates.insert("copy-content-verified".into(), content_verified);
    gates.insert("provider-sync-state-complete".into(), provider_complete);
    gates.insert("immutable-evidence-record-valid".into(), evidence_valid);
    gates.insert("explicit-eviction-permit".into(), permit_issued);
    (gates, evidence.sync_state, Some(record.record_id.clone()))
}

pub fn goal_snapshot_from_evidence(
    receipt: &CloudCopyReceipt,
    record: &ProviderSyncEvidenceRecord,
    goal_state: CloudOffloadGoalState,
    updated_at_ms: u64,
) -> CloudOffloadGoalSnapshot {
    let (completion_gates, provider_sync_state, evidence_record_id) =
        completion_gates(receipt, Some(record), goal_state);
    CloudOffloadGoalSnapshot {
        schema_version: CLOUD_GOAL_SCHEMA_VERSION,
        goal_id: "disksage-cloud-offload".into(),
        status: if goal_state == CloudOffloadGoalState::SourceEvicted {
            "completed".into()
        } else {
            "active".into()
        },
        receipt_id: receipt.receipt_id.clone(),
        goal_state,
        provider_sync_state,
        completion_gates,
        safety_invariant: "source-retained-until-an-explicit-trash-step".into(),
        evidence_record_id,
        updated_at_ms,
    }
}

pub fn initial_goal_snapshot(
    receipt: &CloudCopyReceipt,
    updated_at_ms: u64,
) -> CloudOffloadGoalSnapshot {
    let (completion_gates, provider_sync_state, evidence_record_id) =
        completion_gates(receipt, None, CloudOffloadGoalState::CopyVerified);
    CloudOffloadGoalSnapshot {
        schema_version: CLOUD_GOAL_SCHEMA_VERSION,
        goal_id: "disksage-cloud-offload".into(),
        status: "active".into(),
        receipt_id: receipt.receipt_id.clone(),
        goal_state: CloudOffloadGoalState::CopyVerified,
        provider_sync_state,
        completion_gates,
        safety_invariant: "source-retained-until-an-explicit-trash-step".into(),
        evidence_record_id,
        updated_at_ms,
    }
}

fn secure_directory(directory: &Path) -> Result<(), String> {
    std::fs::create_dir_all(directory)
        .map_err(|_| "cloud-snapshot-directory-create-failed".to_string())?;
    let metadata = std::fs::symlink_metadata(directory)
        .map_err(|_| "cloud-snapshot-directory-metadata-failed".to_string())?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("cloud-snapshot-directory-unsafe".into());
    }
    Ok(())
}

fn goal_state_rank(state: CloudOffloadGoalState) -> u8 {
    match state {
        CloudOffloadGoalState::CopyVerified => 0,
        CloudOffloadGoalState::PendingProviderSync => 1,
        CloudOffloadGoalState::ProviderSyncConfirmed => 2,
        CloudOffloadGoalState::EvictionReady => 3,
        CloudOffloadGoalState::SourceEvicted => 4,
    }
}

fn projection_state(encoded: &[u8], kind: &str) -> Result<(CloudOffloadGoalState, u64), String> {
    match kind {
        "adr" => serde_json::from_slice::<CloudOffloadAdrSnapshot>(encoded)
            .map(|snapshot| (snapshot.goal_state, snapshot.updated_at_ms))
            .map_err(|_| "cloud-adr-existing-invalid".into()),
        "goal" => serde_json::from_slice::<CloudOffloadGoalSnapshot>(encoded)
            .map(|snapshot| (snapshot.goal_state, snapshot.updated_at_ms))
            .map_err(|_| "cloud-goal-existing-invalid".into()),
        _ => Err("cloud-projection-kind-invalid".into()),
    }
}

fn write_latest_json(
    directory: &Path,
    receipt_id: &str,
    updated_at_ms: u64,
    encoded: &[u8],
    kind: &str,
) -> Result<PathBuf, String> {
    if receipt_id.len() != 64 || !receipt_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("cloud-snapshot-receipt-id-invalid".into());
    }
    secure_directory(directory)?;
    let _guard = PROJECTION_WRITE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "cloud-projection-write-lock-poisoned".to_string())?;
    let path = directory.join(format!("{receipt_id}-latest.json"));
    let incoming = projection_state(encoded, kind)?;
    if let Ok(metadata) = std::fs::symlink_metadata(&path) {
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(format!("cloud-{kind}-existing-unsafe"));
        }
        let existing =
            std::fs::read(&path).map_err(|_| format!("cloud-{kind}-existing-read-failed"))?;
        let previous = projection_state(&existing, kind)?;
        if goal_state_rank(incoming.0) < goal_state_rank(previous.0)
            || (incoming.0 == previous.0 && incoming.1 < previous.1)
        {
            return Err(format!("cloud-{kind}-state-regression"));
        }
    }
    let temporary = directory.join(format!(
        ".{receipt_id}-{updated_at_ms}-{}-{kind}.tmp",
        std::process::id()
    ));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|_| format!("cloud-{kind}-temp-create-failed"))?;
    file.write_all(encoded)
        .map_err(|_| format!("cloud-{kind}-write-failed"))?;
    file.sync_all()
        .map_err(|_| format!("cloud-{kind}-sync-failed"))?;
    drop(file);
    if std::fs::rename(&temporary, &path).is_err() {
        let _ = std::fs::remove_file(&temporary);
        return Err(format!("cloud-{kind}-rename-failed"));
    }
    Ok(path)
}

pub fn write_latest_snapshot(
    directory: &Path,
    snapshot: &CloudOffloadAdrSnapshot,
) -> Result<PathBuf, String> {
    let encoded =
        serde_json::to_vec_pretty(snapshot).map_err(|_| "cloud-adr-json-invalid".to_string())?;
    write_latest_json(
        directory,
        &snapshot.receipt_id,
        snapshot.updated_at_ms,
        &encoded,
        "adr",
    )
}

pub fn write_latest_goal_snapshot(
    directory: &Path,
    snapshot: &CloudOffloadGoalSnapshot,
) -> Result<PathBuf, String> {
    let encoded =
        serde_json::to_vec_pretty(snapshot).map_err(|_| "cloud-goal-json-invalid".to_string())?;
    write_latest_json(
        directory,
        &snapshot.receipt_id,
        snapshot.updated_at_ms,
        &encoded,
        "goal",
    )
}

/// Persist replaceable ADR/Goal projections without turning an authoritative receipt/evidence
/// result into a failed operation. Immutable records remain the source of truth.
pub fn write_projection_pair(
    adr_dir: &Path,
    adr: &CloudOffloadAdrSnapshot,
    goal_dir: &Path,
    goal: &CloudOffloadGoalSnapshot,
) -> (Option<PathBuf>, Option<PathBuf>, Vec<String>) {
    let mut warnings = Vec::new();
    let adr_path = match write_latest_snapshot(adr_dir, adr) {
        Ok(path) => Some(path),
        Err(error) => {
            // The projection writer only returns bounded, path-free error codes. Preserve the
            // code so reconciliation can distinguish a rejected state regression from I/O.
            warnings.push(format!("adr-projection-write-failed:{error}"));
            None
        }
    };
    let goal_path = match write_latest_goal_snapshot(goal_dir, goal) {
        Ok(path) => Some(path),
        Err(error) => {
            warnings.push(format!("goal-projection-write-failed:{error}"));
            None
        }
    };
    (adr_path, goal_path, warnings)
}

/// Seed projections for a receipt whose provider evidence is not available yet.
///
/// This never creates an evidence record or advances a goal. A previously observed advanced
/// projection is authoritative for the current state, so its expected state-regression warning is
/// ignored while missing projections are created with an explicit `unknown` provider state.
pub fn ensure_initial_projection_pair(
    receipt: &CloudCopyReceipt,
    adr_dir: &Path,
    goal_dir: &Path,
    updated_at_ms: u64,
) -> Vec<String> {
    let adr = initial_adr_snapshot(receipt, updated_at_ms);
    let goal = initial_goal_snapshot(receipt, updated_at_ms);
    let (_, _, mut warnings) = write_projection_pair(adr_dir, &adr, goal_dir, &goal);
    warnings.retain(|warning| {
        !warning.ends_with("cloud-adr-state-regression")
            && !warning.ends_with("cloud-goal-state-regression")
    });
    warnings
}

fn read_latest_projection<T: serde::de::DeserializeOwned>(
    directory: &Path,
    receipt_id: &str,
    kind: &str,
) -> Result<Option<T>, String> {
    let metadata = match std::fs::symlink_metadata(directory) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(format!("cloud-{kind}-directory-metadata-failed")),
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!("cloud-{kind}-directory-unsafe"));
    }
    let path = directory.join(format!("{receipt_id}-latest.json"));
    let metadata = match std::fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(format!("cloud-{kind}-existing-metadata-failed")),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!("cloud-{kind}-existing-unsafe"));
    }
    if metadata.len() == 0 || metadata.len() > MAX_PROJECTION_BYTES {
        return Err(format!("cloud-{kind}-existing-size-invalid"));
    }
    let capacity = usize::try_from(metadata.len())
        .map_err(|_| format!("cloud-{kind}-existing-size-invalid"))?;
    let mut encoded = Vec::with_capacity(capacity);
    std::fs::File::open(&path)
        .map_err(|_| format!("cloud-{kind}-existing-read-failed"))?
        .take(MAX_PROJECTION_BYTES + 1)
        .read_to_end(&mut encoded)
        .map_err(|_| format!("cloud-{kind}-existing-read-failed"))?;
    if encoded.len() as u64 != metadata.len() {
        return Err(format!("cloud-{kind}-existing-changed"));
    }
    serde_json::from_slice(&encoded)
        .map(Some)
        .map_err(|_| format!("cloud-{kind}-existing-invalid"))
}

/// Read the last paired ADR/Goal state without creating or mutating anything.
/// A partial or divergent pair is treated as unavailable so callers cannot mistake stale state for
/// a fresh provider attestation.
pub fn read_projection_state(
    receipt_id: &str,
    adr_dir: &Path,
    goal_dir: &Path,
) -> Result<Option<CloudProjectionState>, String> {
    if receipt_id.len() != 64 || !receipt_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("cloud-snapshot-receipt-id-invalid".into());
    }
    let adr = read_latest_projection::<CloudOffloadAdrSnapshot>(adr_dir, receipt_id, "adr")?;
    let goal = read_latest_projection::<CloudOffloadGoalSnapshot>(goal_dir, receipt_id, "goal")?;
    match (adr, goal) {
        (None, None) => Ok(None),
        (Some(_), None) | (None, Some(_)) => Err("cloud-projection-pair-incomplete".into()),
        (Some(adr), Some(goal)) => {
            if adr.receipt_id != receipt_id
                || goal.receipt_id != receipt_id
                || adr.goal_state != goal.goal_state
                || adr.provider_sync_state != goal.provider_sync_state
                || adr.updated_at_ms != goal.updated_at_ms
            {
                return Err("cloud-projection-state-mismatch".into());
            }
            Ok(Some(CloudProjectionState {
                goal_state: goal.goal_state,
                provider_sync_state: goal.provider_sync_state,
                updated_at_ms: goal.updated_at_ms,
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::CloudProvider;

    fn receipt() -> CloudCopyReceipt {
        CloudCopyReceipt {
            version: crate::cloud_transfer::RECEIPT_VERSION,
            receipt_id: "a".repeat(64),
            candidate_fingerprint: "b".repeat(64),
            provider: CloudProvider::Icloud,
            source: "/source/file.bin".into(),
            destination: "/cloud/file.bin".into(),
            bytes: 1,
            blake3: "c".repeat(64),
            sha256: "d".repeat(64),
            quick_xor_base64: String::new(),
            source_modified_ms: 1,
            copied_at_ms: 2,
            copy_verified: true,
            provider_sync_confirmed: false,
            lineage_fingerprint: None,
            lineage: None,
        }
    }

    fn pending_record() -> ProviderSyncEvidenceRecord {
        crate::provider_evidence::create_sync_evidence_record(
            &crate::cloud_transfer::ProviderSyncEvidence {
                receipt_id: "a".repeat(64),
                provider: CloudProvider::Icloud,
                destination: "/cloud/file.bin".into(),
                observed_bytes: 1,
                destination_blake3: "c".repeat(64),
                confirmed_at_ms: 3,
                kind: crate::cloud_transfer::SyncEvidenceKind::ProviderNativeStatus,
                evidence_id: "foundation:test".into(),
                sync_complete: false,
                sync_state: ProviderSyncState::PendingUpload,
                remote_content: None,
            },
        )
        .unwrap()
    }

    #[test]
    fn pending_upload_goal_never_satisfies_provider_gate() {
        let record = pending_record();
        let snapshot = goal_snapshot_from_evidence(
            &receipt(),
            &record,
            CloudOffloadGoalState::PendingProviderSync,
            4,
        );
        assert_eq!(
            snapshot.provider_sync_state,
            ProviderSyncState::PendingUpload
        );
        assert!(!snapshot.completion_gates["provider-sync-state-complete"]);
        assert!(!snapshot.completion_gates["explicit-eviction-permit"]);
    }

    #[test]
    fn initial_adr_is_written_without_fabricating_provider_evidence() {
        let snapshot = initial_adr_snapshot(&receipt(), 5);
        assert_eq!(snapshot.goal_state, CloudOffloadGoalState::CopyVerified);
        assert_eq!(snapshot.provider_sync_state, ProviderSyncState::Unknown);
        assert_eq!(snapshot.evidence_record_id, None);
        assert_eq!(snapshot.adr_id, format!("cloud-offload:{}", "a".repeat(64)));
    }

    #[test]
    fn goal_projection_replaces_latest_atomically() {
        let directory = tempfile::tempdir().unwrap();
        let snapshot = initial_goal_snapshot(&receipt(), 5);
        let path = write_latest_goal_snapshot(directory.path(), &snapshot).unwrap();
        let persisted: CloudOffloadGoalSnapshot =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(persisted.goal_state, CloudOffloadGoalState::CopyVerified);
        assert!(persisted.evidence_record_id.is_none());
    }

    #[test]
    fn snapshot_writer_rejects_path_like_receipt_ids() {
        let directory = tempfile::tempdir().unwrap();
        let mut snapshot = initial_goal_snapshot(&receipt(), 5);
        snapshot.receipt_id = "../outside".into();
        assert_eq!(
            write_latest_goal_snapshot(directory.path(), &snapshot).unwrap_err(),
            "cloud-snapshot-receipt-id-invalid"
        );
    }

    #[test]
    fn projection_writer_rejects_late_state_regression() {
        let directory = tempfile::tempdir().unwrap();
        let receipt = receipt();
        let mut source_evicted = goal_snapshot_from_evidence(
            &receipt,
            &pending_record(),
            CloudOffloadGoalState::SourceEvicted,
            10,
        );
        write_latest_goal_snapshot(directory.path(), &source_evicted).unwrap();
        source_evicted.goal_state = CloudOffloadGoalState::EvictionReady;
        source_evicted.updated_at_ms = 11;
        assert_eq!(
            write_latest_goal_snapshot(directory.path(), &source_evicted).unwrap_err(),
            "cloud-goal-state-regression"
        );
        let persisted: CloudOffloadGoalSnapshot = serde_json::from_slice(
            &std::fs::read(directory.path().join(format!("{}-latest.json", receipt.receipt_id)))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(persisted.goal_state, CloudOffloadGoalState::SourceEvicted);
    }

    #[test]
    fn projection_pair_preserves_sanitized_write_error_codes() {
        let adr_directory = tempfile::tempdir().unwrap();
        let goal_directory = tempfile::tempdir().unwrap();
        let receipt = receipt();
        let record = pending_record();
        let adr = snapshot_from_evidence(
            &record,
            CloudOffloadGoalState::SourceEvicted,
            10,
        );
        let goal = goal_snapshot_from_evidence(
            &receipt,
            &record,
            CloudOffloadGoalState::SourceEvicted,
            10,
        );
        write_latest_snapshot(adr_directory.path(), &adr).unwrap();
        write_latest_goal_snapshot(goal_directory.path(), &goal).unwrap();

        let (_, _, warnings) = write_projection_pair(
            adr_directory.path(),
            &initial_adr_snapshot(&receipt, 11),
            goal_directory.path(),
            &initial_goal_snapshot(&receipt, 11),
        );
        assert!(warnings
            .iter()
            .any(|warning| warning == "adr-projection-write-failed:cloud-adr-state-regression"));
        assert!(warnings
            .iter()
            .any(|warning| warning == "goal-projection-write-failed:cloud-goal-state-regression"));
    }

    #[test]
    fn initial_projection_pair_seeds_missing_state_without_evidence() {
        let temporary = tempfile::tempdir().unwrap();
        let adr_dir = temporary.path().join("adr");
        let goal_dir = temporary.path().join("goals");
        let receipt = receipt();
        assert!(ensure_initial_projection_pair(&receipt, &adr_dir, &goal_dir, 4).is_empty());

        let adr: CloudOffloadAdrSnapshot = serde_json::from_slice(
            &std::fs::read(adr_dir.join(format!("{}-latest.json", receipt.receipt_id))).unwrap(),
        )
        .unwrap();
        let goal: CloudOffloadGoalSnapshot = serde_json::from_slice(
            &std::fs::read(goal_dir.join(format!("{}-latest.json", receipt.receipt_id))).unwrap(),
        )
        .unwrap();
        assert_eq!(adr.goal_state, CloudOffloadGoalState::CopyVerified);
        assert_eq!(adr.provider_sync_state, ProviderSyncState::Unknown);
        assert_eq!(goal.goal_state, CloudOffloadGoalState::CopyVerified);
        assert_eq!(goal.provider_sync_state, ProviderSyncState::Unknown);
        assert!(goal.evidence_record_id.is_none());
    }

    #[test]
    fn projection_state_can_be_read_without_granting_eviction_authority() {
        let temporary = tempfile::tempdir().unwrap();
        let adr_dir = temporary.path().join("adr");
        let goal_dir = temporary.path().join("goals");
        let receipt = receipt();
        ensure_initial_projection_pair(&receipt, &adr_dir, &goal_dir, 4);

        assert_eq!(
            read_projection_state(&receipt.receipt_id, &adr_dir, &goal_dir).unwrap(),
            Some(CloudProjectionState {
                goal_state: CloudOffloadGoalState::CopyVerified,
                provider_sync_state: ProviderSyncState::Unknown,
                updated_at_ms: 4,
            })
        );
    }

    #[test]
    fn divergent_projection_pair_is_not_reused_as_current_state() {
        let temporary = tempfile::tempdir().unwrap();
        let adr_dir = temporary.path().join("adr");
        let goal_dir = temporary.path().join("goals");
        let receipt = receipt();
        ensure_initial_projection_pair(&receipt, &adr_dir, &goal_dir, 4);
        let mut goal = initial_goal_snapshot(&receipt, 5);
        goal.goal_state = CloudOffloadGoalState::PendingProviderSync;
        goal.provider_sync_state = ProviderSyncState::PendingUpload;
        write_latest_goal_snapshot(&goal_dir, &goal).unwrap();

        assert_eq!(
            read_projection_state(&receipt.receipt_id, &adr_dir, &goal_dir).unwrap_err(),
            "cloud-projection-state-mismatch"
        );
    }
}
