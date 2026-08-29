use serde::Serialize;
use std::path::Path;

// Keep the established Podman evidence/prune implementation private behind this public safety
// boundary. The split lets storage-repair receipts strengthen their contract without exposing the
// implementation's pre-integration numeric assumptions as public API.
#[path = "podman_reclaim_public_impl.rs"]
mod legacy_public;

pub use legacy_public::{
    inspect_raw_image_evidence, plan_podman_storage_repair, probe_podman_reclaim,
    prune_dangling_images, GuestFilesystemEvidence, PodmanDanglingImagePruneExecution,
    PodmanHostCompactionPlan, PodmanMachineEvidence, PodmanReclaimAssessment, PodmanReclaimPlan, PodmanRecommendedAction,
    PodmanRecommendedActionKind, PodmanStorageCheckPlan, PodmanStoreEvidence,
    PodmanSystemDfCategoryEvidence, PodmanSystemDfEvidence, PodmanUnusedImageEvidence,
    RawImageEvidence, DEFAULT_PODMAN_MACHINE, DEFAULT_PROBE_TIMEOUT, PODMAN_RECLAIM_SCHEMA_KIND,
};

/// Public receipt for one fingerprint-approved native Podman storage repair attempt.
///
/// Repair counts are present only when the fresh postcheck is complete. A failed or unavailable
/// postcheck therefore records that the command ran without converting missing evidence into a
/// claim that every previously damaged layer was repaired.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PodmanStorageRepairExecution {
    pub schema_version: u32,
    pub machine: String,
    pub candidate_set_sha256: String,
    pub command: Vec<String>,
    pub status_code: i32,
    pub command_attempted: bool,
    pub execution_issue: Option<String>,
    pub executed: bool,
    pub repaired_layer_records: Option<u64>,
    pub remaining_damaged_layer_records: Option<u64>,
    pub postcheck_complete: bool,
    pub executed_at_ms: u64,
    pub rationale: String,
}

fn public_repair_execution(
    raw: legacy_public::PodmanStorageRepairExecution,
) -> PodmanStorageRepairExecution {
    let counts_verified = raw.postcheck_complete;
    PodmanStorageRepairExecution {
        schema_version: raw.schema_version,
        machine: raw.machine,
        candidate_set_sha256: raw.candidate_set_sha256,
        command: raw.command,
        status_code: raw.status_code,
        command_attempted: raw.command_attempted,
        execution_issue: raw.execution_issue,
        executed: raw.executed,
        repaired_layer_records: counts_verified.then_some(raw.repaired_layer_records),
        remaining_damaged_layer_records: counts_verified
            .then_some(raw.remaining_damaged_layer_records),
        postcheck_complete: raw.postcheck_complete,
        executed_at_ms: raw.executed_at_ms,
        rationale: raw.rationale,
    }
}

/// Execute one native non-forced Podman storage repair behind a bounded public request contract.
///
/// The exact candidate fingerprint and confirmation checks remain owned by the underlying repair
/// implementation. This boundary additionally applies the repository-wide rationale bounds and
/// refuses to publish numeric repair outcomes unless the fresh postcheck is complete.
pub fn execute_podman_storage_repair(
    podman_bin: &Path,
    machine: &str,
    confirmation_phrase: &str,
    rationale: &str,
    executed_at_ms: u64,
) -> Result<PodmanStorageRepairExecution, String> {
    if executed_at_ms == 0
        || rationale.trim().is_empty()
        || rationale != rationale.trim()
        || rationale.chars().count() > 1_000
        || rationale.chars().any(char::is_control)
    {
        return Err("podman-storage-repair-request-invalid".into());
    }

    let raw = legacy_public::execute_podman_storage_repair(
        podman_bin,
        machine,
        confirmation_phrase,
        rationale,
        executed_at_ms,
    )?;
    Ok(public_repair_execution(raw))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw_execution() -> legacy_public::PodmanStorageRepairExecution {
        legacy_public::PodmanStorageRepairExecution {
            schema_version: 1,
            machine: "podman-machine-default".into(),
            candidate_set_sha256: "a".repeat(64),
            command: vec!["podman".into(), "system".into(), "check".into()],
            status_code: 0,
            command_attempted: true,
            execution_issue: None,
            executed: true,
            repaired_layer_records: 2,
            remaining_damaged_layer_records: 0,
            postcheck_complete: true,
            executed_at_ms: 42,
            rationale: "reviewed".into(),
        }
    }

    #[test]
    fn public_receipt_never_upgrades_unverified_or_failed_repairs() {
        let mut incomplete = raw_execution();
        incomplete.postcheck_complete = false;
        incomplete.executed = false;
        incomplete.execution_issue = Some("podman-storage-repair-postcheck-incomplete".into());
        let receipt = public_repair_execution(incomplete);
        assert!(!receipt.executed);
        assert_eq!(receipt.repaired_layer_records, None);

        let mut nonzero = raw_execution();
        nonzero.status_code = 1;
        nonzero.executed = false;
        let receipt = public_repair_execution(nonzero);
        assert!(!receipt.executed);
        assert_eq!(receipt.repaired_layer_records, Some(2));
    }
}
