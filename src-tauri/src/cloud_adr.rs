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
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
#[cfg(unix)]
use std::os::unix::io::AsRawFd;
#[cfg(windows)]
use std::os::windows::fs::OpenOptionsExt;

pub const CLOUD_ADR_SCHEMA_VERSION: u32 = 3;
pub const CLOUD_GOAL_SCHEMA_VERSION: u32 = 1;
const MAX_PROJECTION_BYTES: u64 = 256 * 1024;

// ponytail: one process-wide lock keeps low-volume projections ordered; the receipt lock below
// closes the cross-process race without adding a lock manager or database.
static PROJECTION_WRITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
const INTERPROCESS_LOCK_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CloudOffloadAdrSnapshot {
    pub schema_version: u32,
    pub adr_id: String,
    pub receipt_id: String,
    pub goal_state: CloudOffloadGoalState,
    pub provider_sync_state: ProviderSyncState,
    pub sync_complete: bool,
    /// Dynamic ADR context; old v2 projections deserialize with an empty context.
    #[serde(default)]
    pub context: Vec<String>,
    pub decision: String,
    pub consequences: Vec<String>,
    pub evidence_record_id: Option<String>,
    pub updated_at_ms: u64,
}

fn context_for(
    goal_state: CloudOffloadGoalState,
    sync_state: ProviderSyncState,
    sync_complete: bool,
) -> Vec<String> {
    vec![
        "metadata-first-lineage".into(),
        format!("goal-state:{}", goal_state.as_str()),
        format!("provider-sync-state:{}", sync_state.as_str()),
        format!("provider-sync-complete:{sync_complete}"),
        "filename-dates-auxiliary".into(),
        "production-time-precedence:embedded-metadata>explicit-filename-date>filesystem-created>filesystem-modified".into(),
        "provider-evidence-authoritative".into(),
        "source-retained-until-explicit-trash-step".into(),
    ]
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
        context: context_for(goal_state, evidence.sync_state, evidence.sync_complete),
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
        context: context_for(
            CloudOffloadGoalState::CopyVerified,
            ProviderSyncState::Unknown,
            false,
        ),
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

fn valid_receipt_id(receipt_id: &str) -> bool {
    receipt_id.len() == 64 && receipt_id.bytes().all(|byte| byte.is_ascii_hexdigit())
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

struct InterprocessProjectionLock {
    file: std::fs::File,
}

impl Drop for InterprocessProjectionLock {
    fn drop(&mut self) {
        #[cfg(unix)]
        // SAFETY: the descriptor belongs to this guard and remains open until this method
        // returns. Unlocking is best-effort because the file descriptor is closing anyway.
        unsafe {
            libc::flock(self.file.as_raw_fd(), libc::LOCK_UN)
        };
    }
}

fn acquire_interprocess_projection_lock(
    directory: &Path,
    lock_stem: &str,
) -> Result<InterprocessProjectionLock, String> {
    let lock_path = directory.join(format!(".{lock_stem}.lock"));
    if let Ok(metadata) = std::fs::symlink_metadata(&lock_path) {
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err("cloud-projection-lock-unsafe".into());
        }
    }
    let deadline = Instant::now() + INTERPROCESS_LOCK_TIMEOUT;
    loop {
        let mut options = std::fs::OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(unix)]
        options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
        #[cfg(windows)]
        options.share_mode(0);
        let file = match options.open(&lock_path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                if Instant::now() >= deadline {
                    return Err("cloud-projection-lock-timeout".into());
                }
                std::thread::sleep(Duration::from_millis(10));
                continue;
            }
            Err(_) => return Err("cloud-projection-lock-open-failed".into()),
        };

        #[cfg(unix)]
        {
            // SAFETY: flock only uses the live descriptor owned by `file`.
            let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
            if result == 0 {
                return Ok(InterprocessProjectionLock { file });
            }
            let would_block =
                std::io::Error::last_os_error().kind() == std::io::ErrorKind::WouldBlock;
            drop(file);
            if !would_block {
                return Err("cloud-projection-lock-acquire-failed".into());
            }
        }

        #[cfg(windows)]
        return Ok(InterprocessProjectionLock { file });

        #[cfg(unix)]
        if Instant::now() >= deadline {
            return Err("cloud-projection-lock-timeout".into());
        }
        #[cfg(unix)]
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn acquire_projection_pair_lock(
    adr_dir: &Path,
    receipt_id: &str,
) -> Result<InterprocessProjectionLock, String> {
    secure_directory(adr_dir)?;
    acquire_interprocess_projection_lock(adr_dir, &format!("{receipt_id}.pair"))
}

fn write_latest_json(
    directory: &Path,
    receipt_id: &str,
    updated_at_ms: u64,
    encoded: &[u8],
    kind: &str,
) -> Result<PathBuf, String> {
    if !valid_receipt_id(receipt_id) {
        return Err("cloud-snapshot-receipt-id-invalid".into());
    }
    secure_directory(directory)?;
    let _guard = PROJECTION_WRITE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "cloud-projection-write-lock-poisoned".to_string())?;
    let _interprocess_guard = acquire_interprocess_projection_lock(directory, receipt_id)?;
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
#[derive(Debug)]
pub struct ProjectionWriteOutcome {
    pub adr_path: Option<PathBuf>,
    pub goal_path: Option<PathBuf>,
    pub warnings: Vec<String>,
    pub wrote: bool,
}

pub fn write_projection_pair(
    adr_dir: &Path,
    adr: &CloudOffloadAdrSnapshot,
    goal_dir: &Path,
    goal: &CloudOffloadGoalSnapshot,
) -> (Option<PathBuf>, Option<PathBuf>, Vec<String>) {
    if adr.receipt_id == goal.receipt_id && valid_receipt_id(&adr.receipt_id) {
        let pair_lock = match acquire_projection_pair_lock(adr_dir, &adr.receipt_id) {
            Ok(lock) => lock,
            Err(error) => {
                return (
                    None,
                    None,
                    vec![format!("projection-pair-lock-failed:{error}")],
                )
            }
        };
        let result = write_projection_pair_unlocked(adr_dir, adr, goal_dir, goal);
        drop(pair_lock);
        return result;
    }
    write_projection_pair_unlocked(adr_dir, adr, goal_dir, goal)
}

fn write_projection_pair_unlocked(
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

/// Persist a source-state blocker without allowing a previously observed goal state to regress.
///
/// A missing or non-local source is a current safety fact, not proof that a completed goal should
/// be rewound. Keep the monotonic state for audit history, but make the replaceable Goal blocked
/// and revoke its explicit eviction gate. A `source-evicted` projection is already the terminal
/// state and is left untouched when its original source is no longer present.
pub fn write_projection_pair_with_source_blocker(
    adr_dir: &Path,
    adr: &CloudOffloadAdrSnapshot,
    goal_dir: &Path,
    goal: &CloudOffloadGoalSnapshot,
    source_blocker: Option<&str>,
) -> (Option<PathBuf>, Option<PathBuf>, Vec<String>) {
    let outcome = write_projection_pair_with_source_blocker_outcome(
        adr_dir,
        adr,
        goal_dir,
        goal,
        source_blocker,
    );
    (outcome.adr_path, outcome.goal_path, outcome.warnings)
}

/// Write a projection pair and report whether either projection was actually changed.
///
/// The source-evicted terminal state deliberately returns existing paths with `wrote = false`;
/// callers must not treat those paths as a mutation.
pub fn write_projection_pair_with_source_blocker_outcome(
    adr_dir: &Path,
    adr: &CloudOffloadAdrSnapshot,
    goal_dir: &Path,
    goal: &CloudOffloadGoalSnapshot,
    source_blocker: Option<&str>,
) -> ProjectionWriteOutcome {
    write_projection_pair_with_state_blockers_outcome(
        adr_dir,
        adr,
        goal_dir,
        goal,
        source_blocker,
        None,
    )
}

/// Persist a provider-attestation blocker without rewinding a previously observed goal state.
pub fn write_projection_pair_with_provider_blocker_outcome(
    adr_dir: &Path,
    adr: &CloudOffloadAdrSnapshot,
    goal_dir: &Path,
    goal: &CloudOffloadGoalSnapshot,
    provider_blocker: &str,
) -> ProjectionWriteOutcome {
    write_projection_pair_with_state_blockers_outcome(
        adr_dir,
        adr,
        goal_dir,
        goal,
        None,
        Some(provider_blocker),
    )
}

/// Persist both source and provider blockers without rewinding a prior goal state.
pub fn write_projection_pair_with_state_blockers_outcome(
    adr_dir: &Path,
    adr: &CloudOffloadAdrSnapshot,
    goal_dir: &Path,
    goal: &CloudOffloadGoalSnapshot,
    source_blocker: Option<&str>,
    provider_blocker: Option<&str>,
) -> ProjectionWriteOutcome {
    if source_blocker.is_none() && provider_blocker.is_none() {
        let (adr_path, goal_path, warnings) = write_projection_pair(adr_dir, adr, goal_dir, goal);
        return ProjectionWriteOutcome {
            wrote: adr_path.is_some() || goal_path.is_some(),
            adr_path,
            goal_path,
            warnings,
        };
    };

    if adr.receipt_id != goal.receipt_id || !valid_receipt_id(&goal.receipt_id) {
        let (adr_path, goal_path, warnings) = write_projection_pair(adr_dir, adr, goal_dir, goal);
        return ProjectionWriteOutcome {
            wrote: adr_path.is_some() || goal_path.is_some(),
            adr_path,
            goal_path,
            warnings,
        };
    }
    let pair_lock = match acquire_projection_pair_lock(adr_dir, &goal.receipt_id) {
        Ok(lock) => lock,
        Err(error) => {
            return ProjectionWriteOutcome {
                adr_path: None,
                goal_path: None,
                warnings: vec![format!("projection-pair-lock-failed:{error}")],
                wrote: false,
            }
        }
    };

    let mut adr = adr.clone();
    let mut goal = goal.clone();
    if let (Ok(Some(_previous_adr)), Ok(Some(previous_goal))) = (
        read_latest_projection::<CloudOffloadAdrSnapshot>(
            adr_dir,
            &goal.receipt_id,
            "adr",
        ),
        read_latest_projection::<CloudOffloadGoalSnapshot>(
            goal_dir,
            &goal.receipt_id,
            "goal",
        ),
    ) {
        if previous_goal.goal_state == CloudOffloadGoalState::SourceEvicted {
            return ProjectionWriteOutcome {
                adr_path: Some(adr_dir.join(format!("{}-latest.json", goal.receipt_id))),
                goal_path: Some(goal_dir.join(format!("{}-latest.json", goal.receipt_id))),
                warnings: Vec::new(),
                wrote: false,
            };
        }
        if goal_state_rank(previous_goal.goal_state) > goal_state_rank(goal.goal_state) {
            adr.goal_state = previous_goal.goal_state;
            goal.goal_state = previous_goal.goal_state;
        }
    }
    goal.status = "blocked".into();
    if source_blocker.is_some() {
        goal.completion_gates.insert("source-present".into(), false);
    }
    if provider_blocker.is_some() {
        goal.completion_gates
            .insert("provider-sync-state-complete".into(), false);
    }
    goal.completion_gates
        .insert("explicit-eviction-permit".into(), false);
    let decision_state = if source_blocker.is_some()
        && goal_state_rank(goal.goal_state)
            >= goal_state_rank(CloudOffloadGoalState::ProviderSyncConfirmed)
    {
        CloudOffloadGoalState::ProviderSyncConfirmed
    } else {
        goal.goal_state
    };
    let mut decision = decision_for(decision_state, adr.provider_sync_state);
    if source_blocker.is_some() {
        decision.push_str("-source-state-unverified");
    }
    if provider_blocker.is_some() {
        decision.push_str("-provider-state-unverified");
    }
    adr.decision = decision;
    adr.consequences
        .retain(|value| value != "explicit-trash-step-may-proceed");
    if let Some(source_blocker) = source_blocker {
        let blocker = format!("source-state-blocked:{source_blocker}");
        if !adr.consequences.iter().any(|value| value == &blocker) {
            adr.consequences.push(blocker);
        }
        if !adr
            .consequences
            .iter()
            .any(|value| value == "eviction-blocked-until-source-state")
        {
            adr.consequences
                .push("eviction-blocked-until-source-state".into());
        }
    }
    if let Some(provider_blocker) = provider_blocker {
        let blocker = format!("provider-state-blocked:{provider_blocker}");
        if !adr.consequences.iter().any(|value| value == &blocker) {
            adr.consequences.push(blocker);
        }
        if !adr
            .consequences
            .iter()
            .any(|value| value == "eviction-blocked-until-provider-proof")
        {
            adr.consequences
                .push("eviction-blocked-until-provider-proof".into());
        }
    }
    let (adr_path, goal_path, warnings) =
        write_projection_pair_unlocked(adr_dir, &adr, goal_dir, &goal);
    drop(pair_lock);
    ProjectionWriteOutcome {
        wrote: adr_path.is_some() || goal_path.is_some(),
        adr_path,
        goal_path,
        warnings,
    }
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

/// Seed the same no-evidence projection while binding the current source state.
///
/// A provider probe can fail before it produces evidence. In that case a missing or unsafe source
/// must still make the projection explicitly blocked; otherwise a truthful provider-unknown
/// state could be mistaken for a live source eligible for later eviction.
#[cfg(not(coverage))]
pub fn ensure_initial_projection_pair_with_source_state(
    receipt: &CloudCopyReceipt,
    adr_dir: &Path,
    goal_dir: &Path,
    updated_at_ms: u64,
) -> Vec<String> {
    ensure_initial_projection_pair_with_source_state_outcome(receipt, adr_dir, goal_dir, updated_at_ms)
        .warnings
}

#[cfg(not(coverage))]
pub fn ensure_initial_projection_pair_with_source_state_outcome(
    receipt: &CloudCopyReceipt,
    adr_dir: &Path,
    goal_dir: &Path,
    updated_at_ms: u64,
) -> ProjectionWriteOutcome {
    let mut adr = initial_adr_snapshot(receipt, updated_at_ms);
    let mut goal = initial_goal_snapshot(receipt, updated_at_ms);
    let source_blocker =
        crate::cloud_transfer::source_eviction_blocker(Path::new(&receipt.source));
    if let Some(blocker) = source_blocker {
        goal.status = "blocked".into();
        goal.completion_gates.insert("source-present".into(), false);
        adr.decision = format!("{}-source-state-unverified", adr.decision);
        adr.consequences
            .push(format!("source-state-blocked:{blocker}"));
    }
    let mut outcome = write_projection_pair_with_source_blocker_outcome(
        adr_dir,
        &adr,
        goal_dir,
        &goal,
        source_blocker,
    );
    outcome.warnings.retain(|warning| {
        !warning.ends_with("cloud-adr-state-regression")
            && !warning.ends_with("cloud-goal-state-regression")
    });
    outcome
}

/// Seed or update projections when provider attestation fails, preserving the current monotonic
/// state while making the missing proof explicit in the replaceable Goal and ADR.
#[cfg(not(coverage))]
pub fn ensure_initial_projection_pair_with_provider_state_outcome(
    receipt: &CloudCopyReceipt,
    adr_dir: &Path,
    goal_dir: &Path,
    updated_at_ms: u64,
    provider_blocker: &str,
) -> ProjectionWriteOutcome {
    let mut adr = initial_adr_snapshot(receipt, updated_at_ms);
    let mut goal = initial_goal_snapshot(receipt, updated_at_ms);
    let source_blocker =
        crate::cloud_transfer::source_eviction_blocker(Path::new(&receipt.source));
    if let Some(blocker) = source_blocker {
        goal.status = "blocked".into();
        goal.completion_gates.insert("source-present".into(), false);
        adr.decision = format!("{}-source-state-unverified", adr.decision);
        adr.consequences
            .push(format!("source-state-blocked:{blocker}"));
    }
    let mut outcome = write_projection_pair_with_state_blockers_outcome(
        adr_dir,
        &adr,
        goal_dir,
        &goal,
        source_blocker,
        Some(provider_blocker),
    );
    outcome.warnings.retain(|warning| {
        !warning.ends_with("cloud-adr-state-regression")
            && !warning.ends_with("cloud-goal-state-regression")
    });
    outcome
}

fn read_latest_projection<T: serde::de::DeserializeOwned>(
    directory: &Path,
    receipt_id: &str,
    kind: &str,
) -> Result<Option<T>, String> {
    if !valid_receipt_id(receipt_id) {
        return Err("cloud-snapshot-receipt-id-invalid".into());
    }
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

/// Read the last paired ADR/Goal state under the same receipt lock used by writers.
/// The lock file is bounded internal coordination metadata; projection files remain untouched.
/// A partial or divergent pair is treated as unavailable so callers cannot mistake stale state for
/// a fresh provider attestation.
pub fn read_projection_state(
    receipt_id: &str,
    adr_dir: &Path,
    goal_dir: &Path,
) -> Result<Option<CloudProjectionState>, String> {
    if !valid_receipt_id(receipt_id) {
        return Err("cloud-snapshot-receipt-id-invalid".into());
    }
    let _pair_lock = acquire_projection_pair_lock(adr_dir, receipt_id)?;
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

/// Read only the replaceable Goal status for UI/reconciliation reporting.
/// The status is informational; eviction authority still comes from immutable evidence and gates.
pub fn read_goal_status(goal_dir: &Path, receipt_id: &str) -> Result<Option<String>, String> {
    Ok(read_latest_projection::<CloudOffloadGoalSnapshot>(
        goal_dir,
        receipt_id,
        "goal",
    )?
    .map(|snapshot| snapshot.status))
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

    fn complete_record() -> ProviderSyncEvidenceRecord {
        crate::provider_evidence::create_sync_evidence_record(
            &crate::cloud_transfer::ProviderSyncEvidence {
                receipt_id: "a".repeat(64),
                provider: CloudProvider::Icloud,
                destination: "/cloud/file.bin".into(),
                observed_bytes: 1,
                destination_blake3: "c".repeat(64),
                confirmed_at_ms: 3,
                kind: crate::cloud_transfer::SyncEvidenceKind::ProviderNativeStatus,
                evidence_id: "foundation:complete".into(),
                sync_complete: true,
                sync_state: ProviderSyncState::Complete,
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
        assert!(snapshot
            .context
            .contains(&"goal-state:copy-verified".to_string()));
        assert!(snapshot
            .context
            .contains(&"metadata-first-lineage".to_string()));
    }

    #[test]
    fn adr_v2_projection_deserializes_without_context() {
        let mut value = serde_json::to_value(initial_adr_snapshot(&receipt(), 5)).unwrap();
        let object = value.as_object_mut().unwrap();
        object.remove("context");
        object.insert("schema_version".into(), serde_json::json!(2));
        let parsed: CloudOffloadAdrSnapshot = serde_json::from_value(value).unwrap();
        assert!(parsed.context.is_empty());
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
    fn projection_writer_creates_receipt_scoped_lock() {
        let directory = tempfile::tempdir().unwrap();
        let snapshot = initial_goal_snapshot(&receipt(), 5);
        write_latest_goal_snapshot(directory.path(), &snapshot).unwrap();
        let lock_path = directory
            .path()
            .join(format!(".{}.lock", snapshot.receipt_id));
        let metadata = std::fs::symlink_metadata(lock_path).unwrap();
        assert!(metadata.is_file());
    }

    #[test]
    fn projection_pair_writer_creates_receipt_scoped_pair_lock() {
        let temporary = tempfile::tempdir().unwrap();
        let adr_dir = temporary.path().join("adr");
        let goal_dir = temporary.path().join("goals");
        let receipt = receipt();
        write_projection_pair(
            &adr_dir,
            &initial_adr_snapshot(&receipt, 5),
            &goal_dir,
            &initial_goal_snapshot(&receipt, 5),
        );
        let metadata = std::fs::symlink_metadata(
            adr_dir.join(format!(".{}.pair.lock", receipt.receipt_id)),
        )
        .unwrap();
        assert!(metadata.is_file());
    }

    #[test]
    fn concurrent_same_timestamp_pair_writers_preserve_pair_identity() {
        let temporary = tempfile::tempdir().unwrap();
        let adr_dir = temporary.path().join("adr");
        let goal_dir = temporary.path().join("goals");
        let receipt = receipt();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
        let mut writers = Vec::new();
        for index in 0..8 {
            let barrier = std::sync::Arc::clone(&barrier);
            let adr_dir = adr_dir.clone();
            let goal_dir = goal_dir.clone();
            let receipt = receipt.clone();
            writers.push(std::thread::spawn(move || {
                barrier.wait();
                let token = format!("writer-{index}");
                let mut adr = initial_adr_snapshot(&receipt, 42);
                adr.evidence_record_id = Some(token.clone());
                let mut goal = initial_goal_snapshot(&receipt, 42);
                goal.evidence_record_id = Some(token);
                let (_, _, warnings) = write_projection_pair(&adr_dir, &adr, &goal_dir, &goal);
                assert!(warnings.is_empty());
            }));
        }
        for writer in writers {
            writer.join().unwrap();
        }
        let persisted_adr: CloudOffloadAdrSnapshot = serde_json::from_slice(
            &std::fs::read(adr_dir.join(format!("{}-latest.json", receipt.receipt_id))).unwrap(),
        )
        .unwrap();
        let persisted_goal: CloudOffloadGoalSnapshot = serde_json::from_slice(
            &std::fs::read(goal_dir.join(format!("{}-latest.json", receipt.receipt_id))).unwrap(),
        )
        .unwrap();
        assert_eq!(persisted_adr.updated_at_ms, persisted_goal.updated_at_ms);
        assert_eq!(
            persisted_adr.evidence_record_id,
            persisted_goal.evidence_record_id
        );
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

        let outcome = write_projection_pair_with_source_blocker_outcome(
            adr_directory.path(),
            &initial_adr_snapshot(&receipt, 12),
            goal_directory.path(),
            &initial_goal_snapshot(&receipt, 12),
            Some("source-not-present"),
        );
        assert!(!outcome.wrote);
        assert!(outcome.warnings.is_empty());
    }

    #[test]
    fn late_pair_writer_cannot_rewind_source_evicted_state() {
        let temporary = tempfile::tempdir().unwrap();
        let adr_dir = temporary.path().join("adr");
        let goal_dir = temporary.path().join("goals");
        let receipt = receipt();
        let pending = pending_record();
        let source_evicted_adr =
            snapshot_from_evidence(&pending, CloudOffloadGoalState::SourceEvicted, 10);
        let source_evicted_goal = goal_snapshot_from_evidence(
            &receipt,
            &pending,
            CloudOffloadGoalState::SourceEvicted,
            10,
        );
        let (_, _, warnings) = write_projection_pair(
            &adr_dir,
            &source_evicted_adr,
            &goal_dir,
            &source_evicted_goal,
        );
        assert!(warnings.is_empty());

        let complete = complete_record();
        let late_adr =
            snapshot_from_evidence(&complete, CloudOffloadGoalState::ProviderSyncConfirmed, 11);
        let late_goal = goal_snapshot_from_evidence(
            &receipt,
            &complete,
            CloudOffloadGoalState::ProviderSyncConfirmed,
            11,
        );
        let outcome = write_projection_pair(&adr_dir, &late_adr, &goal_dir, &late_goal);
        assert!(outcome.0.is_none());
        assert!(outcome.1.is_none());
        assert!(outcome.2.iter().any(|warning| {
            warning == "adr-projection-write-failed:cloud-adr-state-regression"
        }));
        assert!(outcome.2.iter().any(|warning| {
            warning == "goal-projection-write-failed:cloud-goal-state-regression"
        }));
        assert_eq!(
            read_projection_state(&receipt.receipt_id, &adr_dir, &goal_dir)
                .unwrap()
                .unwrap()
                .goal_state,
            CloudOffloadGoalState::SourceEvicted
        );
    }

    #[test]
    fn source_blocker_updates_goal_without_rewinding_advanced_state() {
        let temporary = tempfile::tempdir().unwrap();
        let adr_dir = temporary.path().join("adr");
        let goal_dir = temporary.path().join("goals");
        let receipt = receipt();
        let record = complete_record();
        let advanced_adr = snapshot_from_evidence(
            &record,
            CloudOffloadGoalState::EvictionReady,
            10,
        );
        let advanced_goal = goal_snapshot_from_evidence(
            &receipt,
            &record,
            CloudOffloadGoalState::EvictionReady,
            10,
        );
        write_projection_pair(&adr_dir, &advanced_adr, &goal_dir, &advanced_goal);

        let mut blocked_adr = snapshot_from_evidence(
            &record,
            CloudOffloadGoalState::ProviderSyncConfirmed,
            11,
        );
        let mut blocked_goal = goal_snapshot_from_evidence(
            &receipt,
            &record,
            CloudOffloadGoalState::ProviderSyncConfirmed,
            11,
        );
        blocked_goal.status = "blocked".into();
        blocked_goal.completion_gates.insert("source-present".into(), false);
        blocked_adr.decision.push_str("-source-state-unverified");
        blocked_adr
            .consequences
            .push("source-state-blocked:source-not-present".into());

        let warnings = write_projection_pair_with_source_blocker(
            &adr_dir,
            &blocked_adr,
            &goal_dir,
            &blocked_goal,
            Some("source-not-present"),
        )
        .2;
        assert!(warnings.is_empty());
        let persisted: CloudOffloadGoalSnapshot = serde_json::from_slice(
            &std::fs::read(goal_dir.join(format!("{}-latest.json", receipt.receipt_id))).unwrap(),
        )
        .unwrap();
        assert_eq!(persisted.goal_state, CloudOffloadGoalState::EvictionReady);
        assert_eq!(persisted.status, "blocked");
        assert!(!persisted.completion_gates["source-present"]);
        assert!(!persisted.completion_gates["explicit-eviction-permit"]);
        let persisted_adr: CloudOffloadAdrSnapshot = serde_json::from_slice(
            &std::fs::read(adr_dir.join(format!("{}-latest.json", receipt.receipt_id))).unwrap(),
        )
        .unwrap();
        assert_eq!(
            persisted_adr.decision,
            "retain-source-eviction-gate-pending-source-state-unverified"
        );
        assert!(persisted_adr
            .consequences
            .contains(&"eviction-blocked-until-source-state".into()));
        assert!(!persisted_adr
            .consequences
            .contains(&"explicit-trash-step-may-proceed".into()));
    }

    #[cfg(not(coverage))]
    #[test]
    fn provider_blocker_updates_goal_without_rewinding_advanced_state() {
        let temporary = tempfile::tempdir().unwrap();
        let source = temporary.path().join("source.bin");
        std::fs::write(&source, b"source").unwrap();
        let adr_dir = temporary.path().join("adr");
        let goal_dir = temporary.path().join("goals");
        let mut receipt = receipt();
        receipt.source = source.to_string_lossy().into_owned();
        let record = pending_record();
        let advanced_adr = snapshot_from_evidence(
            &record,
            CloudOffloadGoalState::PendingProviderSync,
            10,
        );
        let advanced_goal = goal_snapshot_from_evidence(
            &receipt,
            &record,
            CloudOffloadGoalState::PendingProviderSync,
            10,
        );
        write_projection_pair(&adr_dir, &advanced_adr, &goal_dir, &advanced_goal);

        let outcome = write_projection_pair_with_provider_blocker_outcome(
            &adr_dir,
            &advanced_adr,
            &goal_dir,
            &advanced_goal,
            "provider-oauth-connection-missing",
        );
        assert!(outcome.warnings.is_empty());
        let persisted: CloudOffloadGoalSnapshot = serde_json::from_slice(
            &std::fs::read(goal_dir.join(format!("{}-latest.json", receipt.receipt_id))).unwrap(),
        )
        .unwrap();
        assert_eq!(persisted.status, "blocked");
        assert_eq!(persisted.goal_state, CloudOffloadGoalState::PendingProviderSync);
        assert!(!persisted.completion_gates["provider-sync-state-complete"]);
        assert!(!persisted.completion_gates["explicit-eviction-permit"]);
        let persisted_adr: CloudOffloadAdrSnapshot = serde_json::from_slice(
            &std::fs::read(adr_dir.join(format!("{}-latest.json", receipt.receipt_id))).unwrap(),
        )
        .unwrap();
        assert!(persisted_adr.decision.ends_with("-provider-state-unverified"));
        assert!(persisted_adr
            .consequences
            .contains(&"provider-state-blocked:provider-oauth-connection-missing".into()));
        assert!(persisted_adr
            .consequences
            .contains(&"eviction-blocked-until-provider-proof".into()));
    }

    #[cfg(not(coverage))]
    #[test]
    fn source_and_provider_blockers_are_both_projected() {
        let temporary = tempfile::tempdir().unwrap();
        let adr_dir = temporary.path().join("adr");
        let goal_dir = temporary.path().join("goals");
        let receipt = receipt();
        let record = pending_record();
        let adr = snapshot_from_evidence(&record, CloudOffloadGoalState::PendingProviderSync, 10);
        let goal = goal_snapshot_from_evidence(
            &receipt,
            &record,
            CloudOffloadGoalState::PendingProviderSync,
            10,
        );

        let outcome = write_projection_pair_with_state_blockers_outcome(
            &adr_dir,
            &adr,
            &goal_dir,
            &goal,
            Some("source-not-present"),
            Some("provider-sync-incomplete"),
        );
        assert!(outcome.warnings.is_empty());

        let persisted_goal: CloudOffloadGoalSnapshot = serde_json::from_slice(
            &std::fs::read(goal_dir.join(format!("{}-latest.json", receipt.receipt_id))).unwrap(),
        )
        .unwrap();
        assert_eq!(persisted_goal.status, "blocked");
        assert!(!persisted_goal.completion_gates["source-present"]);
        assert!(!persisted_goal.completion_gates["provider-sync-state-complete"]);
        assert!(!persisted_goal.completion_gates["explicit-eviction-permit"]);

        let persisted_adr: CloudOffloadAdrSnapshot = serde_json::from_slice(
            &std::fs::read(adr_dir.join(format!("{}-latest.json", receipt.receipt_id))).unwrap(),
        )
        .unwrap();
        assert!(persisted_adr
            .consequences
            .contains(&"eviction-blocked-until-source-state".into()));
        assert!(persisted_adr
            .consequences
            .contains(&"eviction-blocked-until-provider-proof".into()));
    }

    #[cfg(not(coverage))]
    #[test]
    fn provider_attestation_error_updates_seeded_projection_without_rewinding_goal() {
        let temporary = tempfile::tempdir().unwrap();
        let source = temporary.path().join("source.bin");
        std::fs::write(&source, b"source").unwrap();
        let adr_dir = temporary.path().join("adr");
        let goal_dir = temporary.path().join("goals");
        let mut receipt = receipt();
        receipt.source = source.to_string_lossy().into_owned();
        let record = pending_record();
        let advanced_adr = snapshot_from_evidence(
            &record,
            CloudOffloadGoalState::PendingProviderSync,
            10,
        );
        let advanced_goal = goal_snapshot_from_evidence(
            &receipt,
            &record,
            CloudOffloadGoalState::PendingProviderSync,
            10,
        );
        write_projection_pair(&adr_dir, &advanced_adr, &goal_dir, &advanced_goal);

        let outcome = ensure_initial_projection_pair_with_provider_state_outcome(
            &receipt,
            &adr_dir,
            &goal_dir,
            11,
            "provider-oauth-connection-missing",
        );
        assert!(outcome.wrote);
        assert!(outcome.warnings.is_empty());
        let persisted: CloudOffloadGoalSnapshot = serde_json::from_slice(
            &std::fs::read(goal_dir.join(format!("{}-latest.json", receipt.receipt_id))).unwrap(),
        )
        .unwrap();
        assert_eq!(persisted.goal_state, CloudOffloadGoalState::PendingProviderSync);
        assert_eq!(persisted.status, "blocked");
        assert!(!persisted.completion_gates["provider-sync-state-complete"]);
        assert!(!persisted.completion_gates["explicit-eviction-permit"]);
        let persisted_adr: CloudOffloadAdrSnapshot = serde_json::from_slice(
            &std::fs::read(adr_dir.join(format!("{}-latest.json", receipt.receipt_id))).unwrap(),
        )
        .unwrap();
        assert!(persisted_adr.decision.ends_with("-provider-state-unverified"));
        assert!(persisted_adr
            .consequences
            .contains(&"provider-state-blocked:provider-oauth-connection-missing".into()));
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

    #[cfg(not(coverage))]
    #[test]
    fn source_state_projection_blocks_missing_source_without_fabricating_evidence() {
        let temporary = tempfile::tempdir().unwrap();
        let adr_dir = temporary.path().join("adr");
        let goal_dir = temporary.path().join("goals");
        let receipt = receipt();
        assert!(ensure_initial_projection_pair_with_source_state(
            &receipt, &adr_dir, &goal_dir, 4
        )
        .is_empty());

        let adr: CloudOffloadAdrSnapshot = serde_json::from_slice(
            &std::fs::read(adr_dir.join(format!("{}-latest.json", receipt.receipt_id))).unwrap(),
        )
        .unwrap();
        let goal: CloudOffloadGoalSnapshot = serde_json::from_slice(
            &std::fs::read(goal_dir.join(format!("{}-latest.json", receipt.receipt_id))).unwrap(),
        )
        .unwrap();
        assert_eq!(goal.status, "blocked");
        assert_eq!(goal.completion_gates["source-present"], false);
        assert!(adr.decision.ends_with("-source-state-unverified"));
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
    fn goal_status_can_be_read_without_granting_eviction_authority() {
        let temporary = tempfile::tempdir().unwrap();
        let adr_dir = temporary.path().join("adr");
        let goal_dir = temporary.path().join("goals");
        let receipt = receipt();
        let mut goal = initial_goal_snapshot(&receipt, 4);
        goal.status = "blocked".into();
        write_latest_goal_snapshot(&goal_dir, &goal).unwrap();

        assert_eq!(
            read_goal_status(&goal_dir, &receipt.receipt_id).unwrap(),
            Some("blocked".into())
        );
        assert_eq!(
            read_goal_status(&adr_dir, &receipt.receipt_id).unwrap(),
            None
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
