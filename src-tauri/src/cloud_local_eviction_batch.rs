//! Evidence-bound batch coordination for iCloud and OneDrive local-copy eviction.
//!
//! The coordinator deliberately separates three phases:
//! 1. plan every input path without opening file content,
//! 2. re-plan every selected item before the first mutation,
//! 3. execute sequentially and write immutable per-item checkpoints.
//!
//! An unavailable input is reported and excluded from the executable item set. It is never silently
//! treated as eligible. A batch fingerprint binds the exact successful per-item plan fingerprints,
//! their source-manifest positions, aggregate byte counts, and every unavailable position.

use crate::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use crate::cloud_local_eviction::{
    approve_icloud_local_eviction, execute_icloud_local_eviction, plan_icloud_local_eviction,
    write_immutable_record, IcloudLocalEvictionApproval, IcloudLocalEvictionPlan,
    IcloudLocalEvictionResult,
};
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Version number serialized into every iCloud local-eviction batch record.
///
/// Readers reject unsupported versions instead of guessing how to interpret a record.
pub const ICLOUD_LOCAL_EVICTION_BATCH_VERSION: u32 = 1;
/// Maximum number of manifest entries accepted in one iCloud local-eviction batch.
///
/// The bound limits memory use, review scope, and work authorized by one approval.
pub const MAX_BATCH_ITEMS: usize = 128;
const MAX_RATIONALE_BYTES: usize = 1_024;

const BATCH_NOTICES: [&str; 4] = [
    "all-selected-items-are-replanned-before-first-mutation",
    "unavailable-inputs-are-excluded-and-fingerprint-bound",
    "execution-stops-after-first-unverified-or-failed-item",
    "allocated-byte-reduction-is-not-volume-free-space-proof",
];

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
/// One manifest entry converted into a safe, read-only local-eviction plan.
pub struct IcloudLocalEvictionBatchItem {
    /// Zero-based position of this entry in the caller's original manifest.
    pub input_index: u32,
    /// Evidence-bound single-item plan that is revalidated before execution.
    pub plan: IcloudLocalEvictionPlan,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
/// One manifest entry excluded because complete planning evidence was unavailable.
pub struct IcloudLocalEvictionBatchUnavailable {
    /// Zero-based position of this entry in the caller's original manifest.
    pub input_index: u32,
    /// Bounded, path-free diagnostic code suitable for logs and external records.
    pub error_code: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
/// Complete read-only evidence describing exactly what a human may approve.
pub struct IcloudLocalEvictionBatchPlan {
    /// Serialized schema version used for fail-closed compatibility checks.
    pub version: u32,
    /// Cloud provider that owns every planned item; iCloud and OneDrive are supported.
    pub provider: CloudProvider,
    /// Account boundary within which every planned item was discovered.
    pub account_scope: CloudAccountScope,
    /// Canonical cloud-root path against which every item path was validated.
    pub cloud_root: String,
    /// Unix timestamp in milliseconds when the read-only evidence was collected.
    pub observed_at_ms: u64,
    /// Number of caller-supplied entries, including unavailable entries.
    pub input_count: u32,
    /// Number of entries that produced safe single-item plans.
    pub planned_count: u32,
    /// Number of entries excluded because evidence was incomplete.
    pub unavailable_count: u32,
    /// Sum of logical byte sizes reported by all planned items.
    pub total_logical_bytes: u64,
    /// Sum of locally allocated bytes reported by all planned items.
    pub total_allocated_bytes: u64,
    /// Ordered executable plans bound to their original manifest positions.
    pub items: Vec<IcloudLocalEvictionBatchItem>,
    /// Ordered excluded entries represented without disclosing their paths.
    pub unavailable: Vec<IcloudLocalEvictionBatchUnavailable>,
    /// BLAKE3 digest binding the exact plan, identities, totals, and exclusions.
    pub batch_fingerprint: String,
    /// Whether execution may proceed after exact attributed human approval.
    pub eligible_after_human_approval: bool,
    /// Fail-closed reasons that currently prevent execution.
    pub blockers: Vec<String>,
    /// Operator-facing limitations that remain true for this record.
    pub notices: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
/// Attributed human approval cryptographically bound to one exact batch plan.
pub struct IcloudLocalEvictionBatchApproval {
    /// Serialized schema version used for fail-closed compatibility checks.
    pub version: u32,
    /// BLAKE3 identifier derived from the approved plan, reviewer, rationale, and time.
    pub approval_id: String,
    /// BLAKE3 digest binding the exact plan, identities, totals, and exclusions.
    pub batch_fingerprint: String,
    /// Unix timestamp in milliseconds when the approval was recorded.
    pub approved_at_ms: u64,
    /// Human identity in the required `human:<identifier>` form.
    pub approved_by: String,
    /// Non-empty explanation of why this exact batch was approved.
    pub rationale: String,
}

/// Immutable evidence that Finder selected the exact approved OneDrive items.
///
/// This record does not claim that local bytes were released. The customer must choose
/// OneDrive's **Free Up Space** action in Finder and then run verification.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OnedriveFinderAssistanceReceipt {
    pub version: u32,
    pub receipt_id: String,
    pub batch_fingerprint: String,
    pub approval_id: String,
    pub approval_evidence_sha256: String,
    pub requested_at_ms: u64,
    pub selected_count: u32,
    pub total_allocated_bytes_before: u64,
    pub items: Vec<OnedriveFinderAssistanceItem>,
    pub finder_selection_requested: bool,
    pub customer_next_action: String,
}

/// Private, identity-bound evidence for one Finder-selected item.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OnedriveFinderAssistanceItem {
    pub path: String,
    pub plan_fingerprint: String,
    pub item_identifier_fingerprint: String,
    pub logical_bytes: u64,
    pub allocated_bytes_before: u64,
    pub filesystem_modified_ms: u64,
}

/// Postcheck of local allocation after the customer used OneDrive's Finder action.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OnedriveFinderAssistanceVerification {
    pub version: u32,
    pub verification_id: String,
    pub receipt_id: String,
    pub verified_at_ms: u64,
    pub retained_count: u32,
    pub verified_count: u32,
    pub total_allocated_bytes_before: u64,
    pub total_allocated_bytes_after: u64,
    pub observed_allocation_reduction_bytes: u64,
    pub verification_complete: bool,
    pub customer_next_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
/// Outcome recorded for one attempted item in an executed batch.
pub struct IcloudLocalEvictionBatchItemOutcome {
    /// Zero-based position of this entry in the caller's original manifest.
    pub input_index: u32,
    /// Fingerprint of the single-item plan used for this attempt.
    pub plan_fingerprint: String,
    /// BLAKE3 identifier derived from the approved plan, reviewer, rationale, and time.
    pub approval_id: String,
    /// Immutable execution-result identifier, or `None` when no result was produced.
    pub result_id: Option<String>,
    /// Whether the operating-system eviction request reported success.
    pub eviction_request_succeeded: bool,
    /// Whether post-request verification and immutable recording completed.
    pub verification_complete: bool,
    /// Observed local allocation reduction without claiming volume-wide free space.
    pub observed_allocation_reduction_bytes: u64,
    /// Bounded, path-free diagnostic code suitable for logs and external records.
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
/// Immutable batch-level execution summary and checkpoint state.
pub struct IcloudLocalEvictionBatchResult {
    /// Serialized schema version used for fail-closed compatibility checks.
    pub version: u32,
    /// BLAKE3 identifier derived from the complete current batch-result state.
    pub result_id: String,
    /// BLAKE3 digest binding the exact plan, identities, totals, and exclusions.
    pub batch_fingerprint: String,
    /// BLAKE3 identifier derived from the approved plan, reviewer, rationale, and time.
    pub approval_id: String,
    /// Unix timestamp in milliseconds when batch execution began.
    pub started_at_ms: u64,
    /// Unix timestamp in milliseconds represented by the latest checkpoint.
    pub completed_at_ms: u64,
    /// Number of caller-supplied entries, including unavailable entries.
    pub input_count: u32,
    /// Number of entries that produced safe single-item plans.
    pub planned_count: u32,
    /// Number of entries excluded because evidence was incomplete.
    pub unavailable_count: u32,
    /// Number of items for which an execution attempt was recorded.
    pub attempted_count: u32,
    /// Number of attempts whose operating-system request reported success.
    pub succeeded_count: u32,
    /// Number of attempts with complete verification and evidence recording.
    pub verified_count: u32,
    /// Total locally allocated bytes reported by the approved pre-execution plan.
    pub total_allocated_bytes_before: u64,
    /// Observed local allocation reduction without claiming volume-wide free space.
    pub observed_allocation_reduction_bytes: u64,
    /// Whether every planned item was attempted and every request succeeded.
    pub execution_complete: bool,
    /// Whether every planned item completed verification and immutable recording.
    pub verification_complete: bool,
    /// Whether fail-closed processing stopped before the batch completed.
    pub halted: bool,
    /// Stable reason code explaining why processing stopped, when applicable.
    pub halt_reason: Option<String>,
    /// Ordered outcome records for items attempted before completion or halt.
    pub item_outcomes: Vec<IcloudLocalEvictionBatchItemOutcome>,
    /// Operator-facing limitations that remain true for this record.
    pub notices: Vec<String>,
}

fn valid_hex64(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn hash_field(hasher: &mut blake3::Hasher, value: &[u8]) {
    hasher.update(&u64::try_from(value.len()).unwrap_or(u64::MAX).to_le_bytes());
    hasher.update(value);
}

fn batch_blockers(items: &[IcloudLocalEvictionBatchItem]) -> Vec<String> {
    if items.is_empty() {
        return vec!["icloud-local-eviction-batch-has-no-planned-items".into()];
    }
    if items.iter().any(|item| !item_plan_is_safe(&item.plan)) {
        return vec!["icloud-local-eviction-batch-item-not-eligible".into()];
    }
    vec!["human-local-eviction-batch-approval-required".into()]
}

fn batch_fingerprint_for(plan: &IcloudLocalEvictionBatchPlan) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-icloud-local-eviction-batch-plan-v1\0");
    hash_field(&mut hasher, plan.provider.as_str().as_bytes());
    hash_field(&mut hasher, plan.account_scope.as_str().as_bytes());
    hash_field(&mut hasher, plan.cloud_root.as_bytes());
    hasher.update(&plan.input_count.to_le_bytes());
    hasher.update(&plan.planned_count.to_le_bytes());
    hasher.update(&plan.unavailable_count.to_le_bytes());
    hasher.update(&plan.total_logical_bytes.to_le_bytes());
    hasher.update(&plan.total_allocated_bytes.to_le_bytes());
    for item in &plan.items {
        hasher.update(&item.input_index.to_le_bytes());
        hash_field(&mut hasher, item.plan.plan_fingerprint.as_bytes());
        hasher.update(&item.plan.logical_bytes.to_le_bytes());
        hasher.update(&item.plan.allocated_bytes.to_le_bytes());
    }
    for unavailable in &plan.unavailable {
        hasher.update(&unavailable.input_index.to_le_bytes());
        hash_field(&mut hasher, unavailable.error_code.as_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

fn approval_id_for(
    batch_fingerprint: &str,
    approved_at_ms: u64,
    approved_by: &str,
    rationale: &str,
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-icloud-local-eviction-batch-approval-v1\0");
    hash_field(&mut hasher, batch_fingerprint.as_bytes());
    hash_field(&mut hasher, approved_by.as_bytes());
    hash_field(&mut hasher, rationale.as_bytes());
    hasher.update(&approved_at_ms.to_le_bytes());
    hasher.finalize().to_hex().to_string()
}

fn result_id_for(result: &IcloudLocalEvictionBatchResult) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-icloud-local-eviction-batch-result-v1\0");
    hash_field(&mut hasher, result.batch_fingerprint.as_bytes());
    hash_field(&mut hasher, result.approval_id.as_bytes());
    hasher.update(&result.started_at_ms.to_le_bytes());
    hasher.update(&result.completed_at_ms.to_le_bytes());
    hasher.update(&result.input_count.to_le_bytes());
    hasher.update(&result.planned_count.to_le_bytes());
    hasher.update(&result.unavailable_count.to_le_bytes());
    hasher.update(&result.attempted_count.to_le_bytes());
    hasher.update(&result.succeeded_count.to_le_bytes());
    hasher.update(&result.verified_count.to_le_bytes());
    hasher.update(&result.total_allocated_bytes_before.to_le_bytes());
    hasher.update(&result.observed_allocation_reduction_bytes.to_le_bytes());
    hasher.update(&[
        result.execution_complete as u8,
        result.verification_complete as u8,
        result.halted as u8,
    ]);
    hash_field(
        &mut hasher,
        result.halt_reason.as_deref().unwrap_or_default().as_bytes(),
    );
    for outcome in &result.item_outcomes {
        hasher.update(&outcome.input_index.to_le_bytes());
        hash_field(&mut hasher, outcome.plan_fingerprint.as_bytes());
        hash_field(&mut hasher, outcome.approval_id.as_bytes());
        hash_field(
            &mut hasher,
            outcome.result_id.as_deref().unwrap_or_default().as_bytes(),
        );
        hasher.update(&[
            outcome.eviction_request_succeeded as u8,
            outcome.verification_complete as u8,
        ]);
        hasher.update(&outcome.observed_allocation_reduction_bytes.to_le_bytes());
        hash_field(
            &mut hasher,
            outcome.error_code.as_deref().unwrap_or_default().as_bytes(),
        );
    }
    hasher.finalize().to_hex().to_string()
}

fn bounded_error_code(error: &str) -> String {
    let trimmed = error.trim();
    if !trimmed.is_empty()
        && trimmed.len() <= 128
        && trimmed.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'-' | b'_' | b':' | b'.')
        })
    {
        trimmed.into()
    } else {
        "icloud-local-eviction-batch-item-unavailable".into()
    }
}

fn item_plan_is_safe(plan: &IcloudLocalEvictionPlan) -> bool {
    let expected_blockers = ["human-local-eviction-approval-required"];
    plan.version == crate::cloud_local_eviction::ICLOUD_LOCAL_EVICTION_VERSION
        && matches!(plan.provider, CloudProvider::Icloud | CloudProvider::Onedrive)
        && valid_hex64(&plan.plan_fingerprint)
        && plan.logical_bytes > 0
        && plan.allocated_bytes > 0
        && plan.eligible_after_human_approval
        && plan
            .blockers
            .iter()
            .map(String::as_str)
            .eq(expected_blockers)
        && plan.active_use.evidence_complete
        && !plan.active_use.active
        && !plan.active_use.results_truncated
        && plan.icloud_state.is_ubiquitous
        && plan.icloud_state.is_uploaded
        && !plan.icloud_state.is_uploading
        && !plan.icloud_state.upload_error_present
        && !plan.icloud_state.is_downloading
        && !plan.icloud_state.download_error_present
        && plan.icloud_state.downloading_status_current
        && !plan.icloud_state.downloading_status_not_downloaded
        && !plan.icloud_state.has_unresolved_conflicts
        && !plan.icloud_state.is_excluded_from_sync
        && match plan.icloud_state.observation_method {
            crate::cloud_local_eviction::IcloudStateObservationMethod::FileProviderCtlEvaluate => {
                plan.icloud_state.is_sync_paused == Some(false)
                    && plan.icloud_state.is_trashed == Some(false)
                    && plan.icloud_state.allows_eviction == Some(true)
                    && plan.icloud_state.provider_reported_bytes == Some(plan.logical_bytes)
                    && plan
                        .icloud_state
                        .item_identifier_fingerprint
                        .as_deref()
                        .is_some_and(valid_hex64)
            }
            crate::cloud_local_eviction::IcloudStateObservationMethod::FoundationUbiquitousResourceValues => {
                plan.provider == CloudProvider::Icloud
                    && plan.icloud_state.is_sync_paused.is_none()
                    && plan.icloud_state.is_trashed.is_none()
                    && plan.icloud_state.allows_eviction.is_none()
                    && plan.icloud_state.provider_reported_bytes.is_none()
                    && plan.icloud_state.item_identifier_fingerprint.is_none()
            }
        }
}

fn expected_notices() -> Vec<String> {
    BATCH_NOTICES
        .iter()
        .map(|notice| (*notice).into())
        .collect()
}

fn validate_batch_plan(
    root: &CloudRoot,
    plan: &IcloudLocalEvictionBatchPlan,
) -> Result<(), String> {
    if plan.version != ICLOUD_LOCAL_EVICTION_BATCH_VERSION
        || !matches!(
            plan.provider,
            CloudProvider::Icloud | CloudProvider::Onedrive
        )
        || plan.provider != root.provider
        || plan.account_scope != root.account_scope
        || plan.cloud_root != root.path
        || plan.input_count == 0
        || usize::try_from(plan.input_count).unwrap_or(usize::MAX) > MAX_BATCH_ITEMS
        || plan.planned_count != u32::try_from(plan.items.len()).unwrap_or(u32::MAX)
        || plan.unavailable_count != u32::try_from(plan.unavailable.len()).unwrap_or(u32::MAX)
        || plan.input_count != plan.planned_count.saturating_add(plan.unavailable_count)
    {
        return Err("icloud-local-eviction-batch-plan-shape-invalid".into());
    }

    let mut indices = BTreeSet::new();
    let mut paths = BTreeSet::new();
    let mut fingerprints = BTreeSet::new();
    for item in &plan.items {
        if item.input_index >= plan.input_count
            || !indices.insert(item.input_index)
            || !paths.insert(item.plan.path.as_str())
            || !fingerprints.insert(item.plan.plan_fingerprint.as_str())
            || item.plan.provider != root.provider
            || item.plan.account_scope != root.account_scope
            || item.plan.cloud_root != root.path
        {
            return Err("icloud-local-eviction-batch-item-identity-invalid".into());
        }
    }
    for unavailable in &plan.unavailable {
        if unavailable.input_index >= plan.input_count
            || !indices.insert(unavailable.input_index)
            || unavailable.error_code != bounded_error_code(&unavailable.error_code)
        {
            return Err("icloud-local-eviction-batch-unavailable-identity-invalid".into());
        }
    }
    if indices.len() != usize::try_from(plan.input_count).unwrap_or(usize::MAX) {
        return Err("icloud-local-eviction-batch-input-index-gap".into());
    }

    let total_logical_bytes = plan
        .items
        .iter()
        .try_fold(0u64, |total, item| {
            total.checked_add(item.plan.logical_bytes)
        })
        .ok_or_else(|| "icloud-local-eviction-batch-logical-total-overflow".to_string())?;
    let total_allocated_bytes = plan
        .items
        .iter()
        .try_fold(0u64, |total, item| {
            total.checked_add(item.plan.allocated_bytes)
        })
        .ok_or_else(|| "icloud-local-eviction-batch-allocated-total-overflow".to_string())?;
    let expected_blockers = batch_blockers(&plan.items);
    let expected_eligible = expected_blockers == ["human-local-eviction-batch-approval-required"];
    if plan.total_logical_bytes != total_logical_bytes
        || plan.total_allocated_bytes != total_allocated_bytes
        || plan.blockers != expected_blockers
        || plan.eligible_after_human_approval != expected_eligible
        || plan.notices != expected_notices()
        || !valid_hex64(&plan.batch_fingerprint)
        || plan.batch_fingerprint != batch_fingerprint_for(plan)
    {
        return Err("icloud-local-eviction-batch-plan-integrity-mismatch".into());
    }
    Ok(())
}

fn build_batch_plan(
    root: &CloudRoot,
    input_count: usize,
    items: Vec<IcloudLocalEvictionBatchItem>,
    unavailable: Vec<IcloudLocalEvictionBatchUnavailable>,
    observed_at_ms: u64,
) -> Result<IcloudLocalEvictionBatchPlan, String> {
    let total_logical_bytes = items
        .iter()
        .try_fold(0u64, |total, item| {
            total.checked_add(item.plan.logical_bytes)
        })
        .ok_or_else(|| "icloud-local-eviction-batch-logical-total-overflow".to_string())?;
    let total_allocated_bytes = items
        .iter()
        .try_fold(0u64, |total, item| {
            total.checked_add(item.plan.allocated_bytes)
        })
        .ok_or_else(|| "icloud-local-eviction-batch-allocated-total-overflow".to_string())?;
    let blockers = batch_blockers(&items);
    let eligible_after_human_approval =
        blockers == ["human-local-eviction-batch-approval-required"];
    let mut plan = IcloudLocalEvictionBatchPlan {
        version: ICLOUD_LOCAL_EVICTION_BATCH_VERSION,
        provider: root.provider,
        account_scope: root.account_scope,
        cloud_root: root.path.clone(),
        observed_at_ms,
        input_count: u32::try_from(input_count)
            .map_err(|_| "icloud-local-eviction-batch-input-count-overflow".to_string())?,
        planned_count: u32::try_from(items.len())
            .map_err(|_| "icloud-local-eviction-batch-planned-count-overflow".to_string())?,
        unavailable_count: u32::try_from(unavailable.len())
            .map_err(|_| "icloud-local-eviction-batch-unavailable-count-overflow".to_string())?,
        total_logical_bytes,
        total_allocated_bytes,
        items,
        unavailable,
        batch_fingerprint: String::new(),
        eligible_after_human_approval,
        blockers,
        notices: expected_notices(),
    };
    plan.batch_fingerprint = batch_fingerprint_for(&plan);
    validate_batch_plan(root, &plan)?;
    Ok(plan)
}

fn plan_batch_with<F>(
    root: &CloudRoot,
    paths: &[PathBuf],
    observed_at_ms: u64,
    mut planner: F,
) -> Result<IcloudLocalEvictionBatchPlan, String>
where
    F: FnMut(&CloudRoot, &Path, u64) -> Result<IcloudLocalEvictionPlan, String>,
{
    if !matches!(
        root.provider,
        CloudProvider::Icloud | CloudProvider::Onedrive
    ) {
        return Err("cloud-local-eviction-batch-provider-unsupported".into());
    }
    if paths.is_empty() || paths.len() > MAX_BATCH_ITEMS {
        return Err("icloud-local-eviction-batch-input-count-invalid".into());
    }
    let mut unique_paths = BTreeSet::new();
    if paths.iter().any(|path| !unique_paths.insert(path.clone())) {
        return Err("icloud-local-eviction-batch-duplicate-input-path".into());
    }

    let mut items = Vec::new();
    let mut unavailable = Vec::new();
    for (index, path) in paths.iter().enumerate() {
        let input_index = u32::try_from(index)
            .map_err(|_| "icloud-local-eviction-batch-input-index-overflow".to_string())?;
        match planner(root, path, observed_at_ms) {
            Ok(plan) if item_plan_is_safe(&plan) => {
                items.push(IcloudLocalEvictionBatchItem { input_index, plan });
            }
            Ok(_) => unavailable.push(IcloudLocalEvictionBatchUnavailable {
                input_index,
                error_code: "icloud-local-eviction-batch-item-not-eligible".into(),
            }),
            Err(error) => unavailable.push(IcloudLocalEvictionBatchUnavailable {
                input_index,
                error_code: bounded_error_code(&error),
            }),
        }
    }
    build_batch_plan(root, paths.len(), items, unavailable, observed_at_ms)
}

/// Build a bounded read-only batch plan. Unsafe or unavailable paths are excluded by index with a
/// bounded, path-free error code. No file content is opened and no local allocation is changed.
#[cfg(not(coverage))]
pub fn plan_icloud_local_eviction_batch(
    root: &CloudRoot,
    paths: &[PathBuf],
    observed_at_ms: u64,
) -> Result<IcloudLocalEvictionBatchPlan, String> {
    plan_batch_with(root, paths, observed_at_ms, plan_icloud_local_eviction)
}

/// Create an attributed human approval for one exact eligible batch plan.
///
/// This pure function validates the plan, reviewer identity, rationale, and timestamps. It
/// never reads file content and never changes local or cloud data.
pub fn approve_icloud_local_eviction_batch(
    plan: &IcloudLocalEvictionBatchPlan,
    root: &CloudRoot,
    approved_batch_fingerprint: &str,
    approved_at_ms: u64,
    approved_by: &str,
    rationale: &str,
) -> Result<IcloudLocalEvictionBatchApproval, String> {
    validate_batch_plan(root, plan)?;
    if plan.batch_fingerprint != approved_batch_fingerprint || !plan.eligible_after_human_approval {
        return Err("icloud-local-eviction-batch-fingerprint-mismatch".into());
    }
    let reviewer = approved_by.trim();
    if !reviewer.starts_with("human:") || reviewer.len() <= "human:".len() {
        return Err("icloud-local-eviction-batch-human-attribution-required".into());
    }
    let rationale = rationale.trim();
    if rationale.is_empty() || rationale.len() > MAX_RATIONALE_BYTES {
        return Err("icloud-local-eviction-batch-rationale-invalid".into());
    }
    if approved_at_ms < plan.observed_at_ms {
        return Err("icloud-local-eviction-batch-approval-predates-plan".into());
    }
    Ok(IcloudLocalEvictionBatchApproval {
        version: ICLOUD_LOCAL_EVICTION_BATCH_VERSION,
        approval_id: approval_id_for(&plan.batch_fingerprint, approved_at_ms, reviewer, rationale),
        batch_fingerprint: plan.batch_fingerprint.clone(),
        approved_at_ms,
        approved_by: reviewer.into(),
        rationale: rationale.into(),
    })
}

fn validate_batch_approval(
    root: &CloudRoot,
    plan: &IcloudLocalEvictionBatchPlan,
    approval: &IcloudLocalEvictionBatchApproval,
    confirmation_batch_fingerprint: &str,
) -> Result<(), String> {
    validate_batch_plan(root, plan)?;
    let reviewer = approval.approved_by.trim();
    let rationale = approval.rationale.trim();
    if approval.version != ICLOUD_LOCAL_EVICTION_BATCH_VERSION
        || approval.batch_fingerprint != plan.batch_fingerprint
        || approval.batch_fingerprint != confirmation_batch_fingerprint
        || approval.approval_id
            != approval_id_for(
                &approval.batch_fingerprint,
                approval.approved_at_ms,
                &approval.approved_by,
                &approval.rationale,
            )
        || approval.approved_at_ms < plan.observed_at_ms
        || reviewer != approval.approved_by
        || !reviewer.starts_with("human:")
        || reviewer.len() <= "human:".len()
        || rationale != approval.rationale
        || rationale.is_empty()
        || rationale.len() > MAX_RATIONALE_BYTES
    {
        return Err("icloud-local-eviction-batch-approval-integrity-mismatch".into());
    }
    Ok(())
}

fn finder_receipt_id(receipt: &OnedriveFinderAssistanceReceipt) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-onedrive-finder-assistance-v1\0");
    hasher.update(&receipt.version.to_le_bytes());
    hash_field(&mut hasher, receipt.batch_fingerprint.as_bytes());
    hash_field(&mut hasher, receipt.approval_id.as_bytes());
    hash_field(&mut hasher, receipt.approval_evidence_sha256.as_bytes());
    hasher.update(&receipt.requested_at_ms.to_le_bytes());
    hasher.update(&receipt.selected_count.to_le_bytes());
    hasher.update(&receipt.total_allocated_bytes_before.to_le_bytes());
    for item in &receipt.items {
        hash_field(&mut hasher, item.path.as_bytes());
        hash_field(&mut hasher, item.plan_fingerprint.as_bytes());
        hash_field(&mut hasher, item.item_identifier_fingerprint.as_bytes());
        hasher.update(&item.logical_bytes.to_le_bytes());
        hasher.update(&item.allocated_bytes_before.to_le_bytes());
        hasher.update(&item.filesystem_modified_ms.to_le_bytes());
    }
    hasher.update(&[receipt.finder_selection_requested as u8]);
    hash_field(&mut hasher, receipt.customer_next_action.as_bytes());
    hasher.finalize().to_hex().to_string()
}

fn approval_evidence_sha256(approval: &IcloudLocalEvictionBatchApproval) -> Result<String, String> {
    let bytes = serde_json::to_vec(approval)
        .map_err(|_| "onedrive-finder-assistance-approval-invalid".to_string())?;
    Ok(crate::content_digest::digest_bytes(&bytes).sha256)
}

fn authenticate_finder_approval(
    receipt: &OnedriveFinderAssistanceReceipt,
    record_dir: &Path,
) -> Result<(), String> {
    let path = record_dir.join(format!("{}.batch-approval.json", receipt.approval_id));
    let metadata = std::fs::symlink_metadata(&path)
        .map_err(|_| "onedrive-finder-assistance-approval-evidence-missing".to_string())?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || !metadata.permissions().readonly()
    {
        return Err("onedrive-finder-assistance-approval-evidence-invalid".into());
    }
    let approval: IcloudLocalEvictionBatchApproval = serde_json::from_slice(
        &std::fs::read(&path)
            .map_err(|_| "onedrive-finder-assistance-approval-evidence-invalid".to_string())?,
    )
    .map_err(|_| "onedrive-finder-assistance-approval-evidence-invalid".to_string())?;
    if approval.approval_id != receipt.approval_id
        || approval.batch_fingerprint != receipt.batch_fingerprint
        || approval.approval_id
            != approval_id_for(
                &approval.batch_fingerprint,
                approval.approved_at_ms,
                &approval.approved_by,
                &approval.rationale,
            )
        || approval_evidence_sha256(&approval)? != receipt.approval_evidence_sha256
    {
        return Err("onedrive-finder-assistance-approval-evidence-invalid".into());
    }
    Ok(())
}

fn finder_verification_id(value: &OnedriveFinderAssistanceVerification) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-onedrive-finder-assistance-verification-v1\0");
    hasher.update(&value.version.to_le_bytes());
    hash_field(&mut hasher, value.receipt_id.as_bytes());
    hasher.update(&value.verified_at_ms.to_le_bytes());
    hasher.update(&value.retained_count.to_le_bytes());
    hasher.update(&value.verified_count.to_le_bytes());
    hasher.update(&value.total_allocated_bytes_before.to_le_bytes());
    hasher.update(&value.total_allocated_bytes_after.to_le_bytes());
    hasher.update(&value.observed_allocation_reduction_bytes.to_le_bytes());
    hasher.update(&[value.verification_complete as u8]);
    hash_field(&mut hasher, value.customer_next_action.as_bytes());
    hasher.finalize().to_hex().to_string()
}

fn write_or_verify_immutable_record<T: Serialize>(
    record_dir: &Path,
    filename: &str,
    value: &T,
) -> Result<PathBuf, String> {
    match write_immutable_record(record_dir, filename, value) {
        Ok(path) => Ok(path),
        Err(write_error) => {
            let path = record_dir.join(filename);
            let metadata = std::fs::symlink_metadata(&path).map_err(|_| write_error.clone())?;
            let mut expected = serde_json::to_vec_pretty(value).map_err(|_| write_error.clone())?;
            expected.push(b'\n');
            if metadata.file_type().is_symlink()
                || !metadata.is_file()
                || !metadata.permissions().readonly()
                || metadata.len() != expected.len() as u64
                || std::fs::read(&path).map_err(|_| write_error.clone())? != expected
            {
                return Err(write_error);
            }
            Ok(path)
        }
    }
}

fn prepare_onedrive_finder_assistance_with<P, F>(
    root: &CloudRoot,
    plan: &IcloudLocalEvictionBatchPlan,
    approval: &IcloudLocalEvictionBatchApproval,
    confirmation_batch_fingerprint: &str,
    record_dir: &Path,
    requested_at_ms: u64,
    mut planner: P,
    reveal: F,
) -> Result<OnedriveFinderAssistanceReceipt, String>
where
    P: FnMut(&CloudRoot, &Path, u64) -> Result<IcloudLocalEvictionPlan, String>,
    F: FnOnce(&[PathBuf]) -> Result<(), String>,
{
    if root.provider != CloudProvider::Onedrive {
        return Err("onedrive-finder-assistance-provider-required".into());
    }
    validate_batch_approval(root, plan, approval, confirmation_batch_fingerprint)?;
    let live = preflight_with(root, plan, requested_at_ms, &mut planner)?;
    write_or_verify_immutable_record(
        record_dir,
        &format!("{}.batch-approval.json", approval.approval_id),
        approval,
    )?;
    let paths = live
        .iter()
        .map(|item| PathBuf::from(&item.path))
        .collect::<Vec<_>>();
    let items = live
        .iter()
        .map(|item| {
            Ok(OnedriveFinderAssistanceItem {
                path: item.path.clone(),
                plan_fingerprint: item.plan_fingerprint.clone(),
                item_identifier_fingerprint: item
                    .icloud_state
                    .item_identifier_fingerprint
                    .clone()
                    .filter(|value| valid_hex64(value))
                    .ok_or_else(|| {
                        "onedrive-finder-assistance-item-identity-unconfirmed".to_string()
                    })?,
                logical_bytes: item.logical_bytes,
                allocated_bytes_before: item.allocated_bytes,
                filesystem_modified_ms: item.filesystem_modified_ms,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut receipt = OnedriveFinderAssistanceReceipt {
        version: ICLOUD_LOCAL_EVICTION_BATCH_VERSION,
        receipt_id: String::new(),
        batch_fingerprint: plan.batch_fingerprint.clone(),
        approval_id: approval.approval_id.clone(),
        approval_evidence_sha256: approval_evidence_sha256(approval)?,
        requested_at_ms,
        selected_count: plan.planned_count,
        total_allocated_bytes_before: plan.total_allocated_bytes,
        items,
        finder_selection_requested: false,
        customer_next_action: "DiskSage is preparing the approved Finder selection.".into(),
    };
    receipt.receipt_id = finder_receipt_id(&receipt);
    write_or_verify_immutable_record(
        record_dir,
        &format!("{}.finder-assistance-pending.json", receipt.receipt_id),
        &receipt,
    )?;
    reveal(&paths)?;
    receipt.finder_selection_requested = true;
    receipt.customer_next_action =
        "In Finder, choose OneDrive Free Up Space for the selected items, then verify in DiskSage."
            .into();
    receipt.receipt_id = finder_receipt_id(&receipt);
    write_or_verify_immutable_record(
        record_dir,
        &format!("{}.finder-assistance.json", receipt.receipt_id),
        &receipt,
    )?;
    Ok(receipt)
}

/// Select the exact approved OneDrive items in Finder without invoking private APIs or changing
/// cloud/local data. OneDrive's context-menu action remains an explicit customer action.
#[cfg(all(target_os = "macos", not(coverage)))]
pub fn prepare_onedrive_finder_assistance(
    root: &CloudRoot,
    plan: &IcloudLocalEvictionBatchPlan,
    approval: &IcloudLocalEvictionBatchApproval,
    confirmation_batch_fingerprint: &str,
    record_dir: &Path,
    requested_at_ms: u64,
) -> Result<OnedriveFinderAssistanceReceipt, String> {
    prepare_onedrive_finder_assistance_with(
        root,
        plan,
        approval,
        confirmation_batch_fingerprint,
        record_dir,
        requested_at_ms,
        plan_icloud_local_eviction,
        |paths| {
            tauri_plugin_opener::reveal_items_in_dir(paths)
                .map_err(|_| "onedrive-finder-selection-failed".to_string())
        },
    )
}

#[cfg(any(not(target_os = "macos"), coverage))]
pub fn prepare_onedrive_finder_assistance(
    _root: &CloudRoot,
    _plan: &IcloudLocalEvictionBatchPlan,
    _approval: &IcloudLocalEvictionBatchApproval,
    _confirmation_batch_fingerprint: &str,
    _record_dir: &Path,
    _requested_at_ms: u64,
) -> Result<OnedriveFinderAssistanceReceipt, String> {
    Err("onedrive-finder-assistance-requires-macos".into())
}

fn verify_onedrive_finder_assistance_with<P>(
    root: &CloudRoot,
    receipt: &OnedriveFinderAssistanceReceipt,
    record_dir: &Path,
    verified_at_ms: u64,
    mut planner: P,
) -> Result<OnedriveFinderAssistanceVerification, String>
where
    P: FnMut(&CloudRoot, &Path, u64) -> Result<IcloudLocalEvictionPlan, String>,
{
    authenticate_finder_approval(receipt, record_dir)?;
    if root.provider != CloudProvider::Onedrive
        || receipt.version != ICLOUD_LOCAL_EVICTION_BATCH_VERSION
        || receipt.receipt_id != finder_receipt_id(receipt)
        || receipt.items.len() != usize::try_from(receipt.selected_count).unwrap_or(usize::MAX)
        || !receipt.finder_selection_requested
        || receipt.customer_next_action
            != "In Finder, choose OneDrive Free Up Space for the selected items, then verify in DiskSage."
    {
        return Err("onedrive-finder-assistance-receipt-invalid".into());
    }
    let mut unique_paths = BTreeSet::new();
    let total_before = receipt.items.iter().try_fold(0u64, |total, item| {
        let path = Path::new(&item.path);
        if !path.is_absolute()
            || path
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
            || !unique_paths.insert(item.path.as_str())
            || !valid_hex64(&item.plan_fingerprint)
            || !valid_hex64(&item.item_identifier_fingerprint)
        {
            return Err("onedrive-finder-assistance-receipt-invalid".to_string());
        }
        total
            .checked_add(item.allocated_bytes_before)
            .ok_or_else(|| "onedrive-finder-assistance-receipt-invalid".to_string())
    })?;
    if total_before != receipt.total_allocated_bytes_before {
        return Err("onedrive-finder-assistance-receipt-invalid".into());
    }
    let mut retained_count = 0u32;
    let mut verified_count = 0u32;
    let mut total_after = 0u64;
    for item in &receipt.items {
        let current = planner(root, Path::new(&item.path), verified_at_ms)
            .map_err(|_| "onedrive-finder-assistance-postcheck-unavailable".to_string())?;
        retained_count = retained_count.saturating_add(1);
        let identity_matches = current.icloud_state.item_identifier_fingerprint.as_deref()
            == Some(item.item_identifier_fingerprint.as_str());
        if !identity_matches
            || current.logical_bytes != item.logical_bytes
            || current.filesystem_modified_ms != item.filesystem_modified_ms
        {
            return Err("onedrive-finder-assistance-item-identity-changed".into());
        }
        total_after = total_after.saturating_add(current.allocated_bytes);
        if item_plan_is_verified_online_only(&current)
            && current.allocated_bytes < item.allocated_bytes_before
        {
            verified_count = verified_count.saturating_add(1);
        }
    }
    let reduction = receipt
        .total_allocated_bytes_before
        .saturating_sub(total_after);
    let complete = verified_count == receipt.selected_count;
    let mut result = OnedriveFinderAssistanceVerification {
        version: ICLOUD_LOCAL_EVICTION_BATCH_VERSION,
        verification_id: String::new(),
        receipt_id: receipt.receipt_id.clone(),
        verified_at_ms,
        retained_count,
        verified_count,
        total_allocated_bytes_before: receipt.total_allocated_bytes_before,
        total_allocated_bytes_after: total_after,
        observed_allocation_reduction_bytes: reduction,
        verification_complete: complete,
        customer_next_action: if complete {
            "Local space release verified; keep the cloud copy available in OneDrive.".into()
        } else {
            "Wait for OneDrive to finish, then verify again in DiskSage.".into()
        },
    };
    result.verification_id = finder_verification_id(&result);
    Ok(result)
}

fn item_plan_is_verified_online_only(plan: &IcloudLocalEvictionPlan) -> bool {
    plan.icloud_state.is_ubiquitous
        && plan.icloud_state.is_uploaded
        && !plan.icloud_state.is_uploading
        && !plan.icloud_state.upload_error_present
        && !plan.icloud_state.is_downloading
        && !plan.icloud_state.download_error_present
        && !plan.icloud_state.downloading_status_current
        && plan.icloud_state.downloading_status_not_downloaded
        && !plan.icloud_state.has_unresolved_conflicts
        && !plan.icloud_state.is_excluded_from_sync
        && plan.icloud_state.is_sync_paused == Some(false)
        && plan.icloud_state.is_trashed == Some(false)
}

/// Verify path retention, provider item identity, and allocated-byte reduction after the customer
/// used OneDrive's Finder action. This function never opens file content.
#[cfg(not(coverage))]
pub fn verify_onedrive_finder_assistance(
    root: &CloudRoot,
    receipt: &OnedriveFinderAssistanceReceipt,
    record_dir: &Path,
    verified_at_ms: u64,
) -> Result<OnedriveFinderAssistanceVerification, String> {
    let result = verify_onedrive_finder_assistance_with(
        root,
        receipt,
        record_dir,
        verified_at_ms,
        plan_icloud_local_eviction,
    )?;
    write_or_verify_immutable_record(
        record_dir,
        &format!("{}.finder-verification.json", result.verification_id),
        &result,
    )?;
    Ok(result)
}

fn preflight_with<F>(
    root: &CloudRoot,
    plan: &IcloudLocalEvictionBatchPlan,
    observed_at_ms: u64,
    mut planner: F,
) -> Result<Vec<IcloudLocalEvictionPlan>, String>
where
    F: FnMut(&CloudRoot, &Path, u64) -> Result<IcloudLocalEvictionPlan, String>,
{
    validate_batch_plan(root, plan)?;
    let mut live = Vec::with_capacity(plan.items.len());
    for item in &plan.items {
        let current = planner(root, Path::new(&item.plan.path), observed_at_ms)
            .map_err(|_| "icloud-local-eviction-batch-preflight-item-unavailable".to_string())?;
        if current.plan_fingerprint != item.plan.plan_fingerprint || !item_plan_is_safe(&current) {
            return Err("icloud-local-eviction-batch-preflight-item-changed".into());
        }
        live.push(current);
    }
    Ok(live)
}

fn refresh_result_summary(result: &mut IcloudLocalEvictionBatchResult, completed_at_ms: u64) {
    result.completed_at_ms = completed_at_ms;
    result.attempted_count = u32::try_from(result.item_outcomes.len()).unwrap_or(u32::MAX);
    result.succeeded_count = u32::try_from(
        result
            .item_outcomes
            .iter()
            .filter(|outcome| outcome.eviction_request_succeeded)
            .count(),
    )
    .unwrap_or(u32::MAX);
    result.verified_count = u32::try_from(
        result
            .item_outcomes
            .iter()
            .filter(|outcome| outcome.verification_complete)
            .count(),
    )
    .unwrap_or(u32::MAX);
    result.observed_allocation_reduction_bytes =
        result.item_outcomes.iter().fold(0u64, |total, outcome| {
            total.saturating_add(outcome.observed_allocation_reduction_bytes)
        });
    result.execution_complete = result.attempted_count == result.planned_count
        && result.succeeded_count == result.planned_count;
    result.verification_complete =
        result.execution_complete && result.verified_count == result.planned_count;
    result.result_id = result_id_for(result);
}

fn checkpoint_name(approval_id: &str, attempted_count: u32) -> String {
    format!(
        "{approval_id}.{:03}.batch-result.json",
        attempted_count.max(1)
    )
}

/// Execute one fully preflighted batch.
///
/// All current plans and all immutable individual approval records are prepared before the first
/// eviction request. Execution stops after the first error or incomplete verification. Each
/// attempted item is followed by a create-new batch checkpoint; a rerun therefore fails before a
/// mutation instead of silently reusing an earlier approval record.
#[cfg(not(coverage))]
trait BatchRecordWriter {
    fn write<T: serde::Serialize>(
        &mut self,
        record_dir: &Path,
        name: &str,
        value: &T,
    ) -> Result<(), String>;
}

#[cfg(not(coverage))]
struct ImmutableBatchRecordWriter;

#[cfg(not(coverage))]
impl BatchRecordWriter for ImmutableBatchRecordWriter {
    fn write<T: serde::Serialize>(
        &mut self,
        record_dir: &Path,
        name: &str,
        value: &T,
    ) -> Result<(), String> {
        write_immutable_record(record_dir, name, value).map(|_| ())
    }
}

#[cfg(not(coverage))]
/// Execute one approved batch after revalidating every planned item.
///
/// The coordinator prepares all current plans and immutable individual approvals before the
/// first eviction request. It processes items sequentially, writes a create-new checkpoint
/// after every attempt, and stops after the first error or incomplete verification.
///
/// # Errors
///
/// Returns a stable error code when plan or approval integrity fails, preflight evidence
/// changes, or an immutable approval, result, or checkpoint record cannot be written.
pub fn execute_icloud_local_eviction_batch(
    root: &CloudRoot,
    plan: &IcloudLocalEvictionBatchPlan,
    approval: &IcloudLocalEvictionBatchApproval,
    confirmation_batch_fingerprint: &str,
    record_dir: &Path,
    requested_at_ms: u64,
) -> Result<IcloudLocalEvictionBatchResult, String> {
    let mut recorder = ImmutableBatchRecordWriter;
    execute_icloud_local_eviction_batch_with(
        root,
        plan,
        approval,
        confirmation_batch_fingerprint,
        record_dir,
        requested_at_ms,
        plan_icloud_local_eviction,
        execute_icloud_local_eviction,
        &mut recorder,
        crate::cloud::system_now_ms,
    )
}

#[cfg(not(coverage))]
fn fresh_item_requested_at_ms(now_ms: &mut impl FnMut() -> u64) -> u64 {
    now_ms()
}

#[cfg(not(coverage))]
fn execute_icloud_local_eviction_batch_with<P, E, R, N>(
    root: &CloudRoot,
    plan: &IcloudLocalEvictionBatchPlan,
    approval: &IcloudLocalEvictionBatchApproval,
    confirmation_batch_fingerprint: &str,
    record_dir: &Path,
    requested_at_ms: u64,
    mut planner: P,
    mut executor: E,
    recorder: &mut R,
    mut now_ms: N,
) -> Result<IcloudLocalEvictionBatchResult, String>
where
    P: FnMut(&CloudRoot, &Path, u64) -> Result<IcloudLocalEvictionPlan, String>,
    E: FnMut(
        &CloudRoot,
        &IcloudLocalEvictionPlan,
        &IcloudLocalEvictionApproval,
        &str,
        u64,
    ) -> Result<IcloudLocalEvictionResult, String>,
    R: BatchRecordWriter,
    N: FnMut() -> u64,
{
    validate_batch_approval(root, plan, approval, confirmation_batch_fingerprint)?;
    let _live = preflight_with(root, plan, requested_at_ms, &mut planner)?;

    recorder
        .write(
            record_dir,
            &format!("{}.batch-approval.json", approval.approval_id),
            approval,
        )
        .map_err(|_| "icloud-local-eviction-batch-approval-record-failed".to_string())?;

    let mut individual_approvals = Vec::with_capacity(plan.items.len());
    for item in &plan.items {
        let individual = approve_icloud_local_eviction(
            &item.plan,
            &item.plan.plan_fingerprint,
            approval.approved_at_ms,
            &approval.approved_by,
            &approval.rationale,
        )?;
        recorder
            .write(
                record_dir,
                &format!("{}.approval.json", individual.approval_id),
                &individual,
            )
            .map_err(|_| "icloud-local-eviction-batch-item-approval-record-failed".to_string())?;
        individual_approvals.push(individual);
    }

    let mut batch_result = IcloudLocalEvictionBatchResult {
        version: ICLOUD_LOCAL_EVICTION_BATCH_VERSION,
        result_id: String::new(),
        batch_fingerprint: plan.batch_fingerprint.clone(),
        approval_id: approval.approval_id.clone(),
        started_at_ms: requested_at_ms,
        completed_at_ms: requested_at_ms,
        input_count: plan.input_count,
        planned_count: plan.planned_count,
        unavailable_count: plan.unavailable_count,
        attempted_count: 0,
        succeeded_count: 0,
        verified_count: 0,
        total_allocated_bytes_before: plan.total_allocated_bytes,
        observed_allocation_reduction_bytes: 0,
        execution_complete: false,
        verification_complete: false,
        halted: false,
        halt_reason: None,
        item_outcomes: Vec::with_capacity(plan.items.len()),
        notices: vec![
            "batch-result-does-not-delete-cloud-object".into(),
            "physical-volume-reclaim-remains-unattributed".into(),
        ],
    };
    refresh_result_summary(&mut batch_result, requested_at_ms);

    for (item, individual) in plan.items.iter().zip(individual_approvals.iter()) {
        let item_requested_at_ms = fresh_item_requested_at_ms(&mut now_ms);
        let execution = executor(
            root,
            &item.plan,
            individual,
            &item.plan.plan_fingerprint,
            item_requested_at_ms,
        );
        match execution {
            Ok(result) => {
                let result_record = recorder.write(
                    record_dir,
                    &format!("{}.result.json", result.result_id),
                    &result,
                );
                let record_failed = result_record.is_err();
                batch_result
                    .item_outcomes
                    .push(IcloudLocalEvictionBatchItemOutcome {
                        input_index: item.input_index,
                        plan_fingerprint: item.plan.plan_fingerprint.clone(),
                        approval_id: individual.approval_id.clone(),
                        result_id: Some(result.result_id),
                        eviction_request_succeeded: result.eviction_request_succeeded,
                        verification_complete: result.verification_complete && !record_failed,
                        observed_allocation_reduction_bytes: result
                            .observed_allocation_reduction_bytes,
                        error_code: record_failed.then(|| {
                            "icloud-local-eviction-batch-item-result-record-failed".into()
                        }),
                    });
                if record_failed
                    || !result.eviction_request_succeeded
                    || !result.verification_complete
                {
                    batch_result.halted = true;
                    batch_result.halt_reason = Some(if record_failed {
                        "icloud-local-eviction-batch-item-result-record-failed".into()
                    } else if !result.eviction_request_succeeded {
                        "icloud-local-eviction-batch-item-request-failed".into()
                    } else {
                        "icloud-local-eviction-batch-item-verification-incomplete".into()
                    });
                }
            }
            Err(error) => {
                batch_result
                    .item_outcomes
                    .push(IcloudLocalEvictionBatchItemOutcome {
                        input_index: item.input_index,
                        plan_fingerprint: item.plan.plan_fingerprint.clone(),
                        approval_id: individual.approval_id.clone(),
                        result_id: None,
                        eviction_request_succeeded: false,
                        verification_complete: false,
                        observed_allocation_reduction_bytes: 0,
                        error_code: Some(bounded_error_code(&error)),
                    });
                batch_result.halted = true;
                batch_result.halt_reason =
                    Some("icloud-local-eviction-batch-item-execution-failed".into());
            }
        }
        refresh_result_summary(&mut batch_result, item_requested_at_ms);
        recorder
            .write(
                record_dir,
                &checkpoint_name(&approval.approval_id, batch_result.attempted_count),
                &batch_result,
            )
            .map_err(|_| "icloud-local-eviction-batch-checkpoint-record-failed".to_string())?;
        if batch_result.halted {
            break;
        }
    }
    Ok(batch_result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud_local_eviction::{
        ActiveUseEvidence, IcloudLocalState, IcloudStateObservationMethod,
    };
    use std::cell::Cell;

    #[cfg(windows)]
    const ROOT: &str = r"C:\cloud";
    #[cfg(not(windows))]
    const ROOT: &str = "/cloud";

    fn root() -> CloudRoot {
        CloudRoot {
            id: "icloud:test".into(),
            provider: CloudProvider::Icloud,
            account_scope: CloudAccountScope::Personal,
            label: "iCloud test".into(),
            path: ROOT.into(),
            readable: true,
            access_issue: None,
        }
    }

    fn path(index: usize) -> PathBuf {
        Path::new(ROOT).join(format!("file-{index}.bin"))
    }

    fn safe_plan(index: usize) -> IcloudLocalEvictionPlan {
        IcloudLocalEvictionPlan {
            version: crate::cloud_local_eviction::ICLOUD_LOCAL_EVICTION_VERSION,
            provider: CloudProvider::Icloud,
            account_scope: CloudAccountScope::Personal,
            cloud_root: ROOT.into(),
            path: path(index).to_string_lossy().into_owned(),
            logical_bytes: 1_000 + u64::try_from(index).unwrap(),
            allocated_bytes: 2_000 + u64::try_from(index).unwrap(),
            filesystem_modified_ms: 10,
            filesystem_device_id: 1,
            filesystem_inode: u64::try_from(index).unwrap() + 1,
            observed_at_ms: 20,
            icloud_state: IcloudLocalState {
                observation_method: IcloudStateObservationMethod::FileProviderCtlEvaluate,
                is_ubiquitous: true,
                is_uploaded: true,
                is_uploading: false,
                upload_error_present: false,
                is_downloading: false,
                download_error_present: false,
                downloading_status_current: true,
                downloading_status_not_downloaded: false,
                has_unresolved_conflicts: false,
                is_excluded_from_sync: false,
                is_sync_paused: Some(false),
                is_trashed: Some(false),
                allows_eviction: Some(true),
                provider_reported_bytes: Some(1_000 + u64::try_from(index).unwrap()),
                item_identifier_fingerprint: Some(format!("{:064x}", index + 1)),
            },
            active_use: ActiveUseEvidence {
                method: "test".into(),
                evidence_complete: true,
                active: false,
                observed_pids: Vec::new(),
                results_truncated: false,
                error: None,
            },
            plan_fingerprint: format!("{:064x}", index + 1),
            eligible_after_human_approval: true,
            blockers: vec!["human-local-eviction-approval-required".into()],
            notices: Vec::new(),
        }
    }

    #[test]
    fn batch_plan_binds_exact_items_and_reports_unavailable_without_paths() {
        let paths = vec![path(0), path(1), path(2)];
        let plan = plan_batch_with(&root(), &paths, 20, |_, path, _| {
            if path.ends_with("file-1.bin") {
                Err(format!("sensitive path: {}", path.display()))
            } else if path.ends_with("file-0.bin") {
                Ok(safe_plan(0))
            } else {
                Ok(safe_plan(2))
            }
        })
        .unwrap();
        assert_eq!(plan.input_count, 3);
        assert_eq!(plan.planned_count, 2);
        assert_eq!(plan.unavailable_count, 1);
        assert_eq!(
            plan.unavailable[0].error_code,
            "icloud-local-eviction-batch-item-unavailable"
        );
        assert!(!plan.unavailable[0].error_code.contains('/'));
        assert!(plan.eligible_after_human_approval);
        assert_eq!(
            plan.blockers,
            vec!["human-local-eviction-batch-approval-required"]
        );
        assert!(valid_hex64(&plan.batch_fingerprint));
        validate_batch_plan(&root(), &plan).unwrap();
    }

    #[test]
    fn batch_plan_excludes_sync_incomplete_items_without_blocking_safe_items() {
        let paths = vec![path(0), path(1)];
        let plan = plan_batch_with(&root(), &paths, 20, |_, path, _| {
            if path.ends_with("file-0.bin") {
                Ok(safe_plan(0))
            } else {
                let mut incomplete = safe_plan(1);
                incomplete.icloud_state.is_uploaded = false;
                incomplete.eligible_after_human_approval = false;
                incomplete.blockers = vec!["provider-sync-incomplete".into()];
                Ok(incomplete)
            }
        })
        .unwrap();

        assert_eq!(plan.planned_count, 1);
        assert_eq!(plan.unavailable_count, 1);
        assert_eq!(
            plan.unavailable[0].error_code,
            "icloud-local-eviction-batch-item-not-eligible"
        );
        assert!(plan.eligible_after_human_approval);
        validate_batch_plan(&root(), &plan).unwrap();
    }

    #[test]
    fn onedrive_batch_is_approved_for_native_foundation_execution() {
        let mut onedrive_root = root();
        onedrive_root.id = "onedrive:test".into();
        onedrive_root.provider = CloudProvider::Onedrive;
        onedrive_root.label = "OneDrive test".into();
        let plan = plan_batch_with(&onedrive_root, &[path(0)], 20, |_, _, _| {
            let mut plan = safe_plan(0);
            plan.provider = CloudProvider::Onedrive;
            Ok(plan)
        })
        .unwrap();

        assert_eq!(plan.provider, CloudProvider::Onedrive);
        assert_eq!(plan.planned_count, 1);
        assert!(plan.eligible_after_human_approval);
        validate_batch_plan(&onedrive_root, &plan).unwrap();
    }

    #[test]
    fn finder_approval_evidence_uses_sha256_contract() {
        let approval = IcloudLocalEvictionBatchApproval {
            version: 1,
            approval_id: "id".into(),
            batch_fingerprint: "batch".into(),
            approved_at_ms: 42,
            approved_by: "human:test".into(),
            rationale: "reviewed".into(),
        };
        assert_eq!(
            approval_evidence_sha256(&approval).unwrap(),
            "e6630a97a648e9dacd6c77f7f810e41317d7d759b9fc93461277822c4851cd4b"
        );
    }

    #[test]
    fn finder_assistance_selects_only_the_exact_live_approved_items() {
        let mut onedrive_root = root();
        onedrive_root.id = "onedrive:test".into();
        onedrive_root.provider = CloudProvider::Onedrive;
        onedrive_root.label = "OneDrive test".into();
        let mut item = safe_plan(0);
        item.provider = CloudProvider::Onedrive;
        let plan =
            plan_batch_with(&onedrive_root, &[path(0)], 20, |_, _, _| Ok(item.clone())).unwrap();
        let approval = approve_icloud_local_eviction_batch(
            &plan,
            &onedrive_root,
            &plan.batch_fingerprint,
            21,
            "human:operator",
            "Exact Finder selection reviewed",
        )
        .unwrap();
        let records = tempfile::tempdir().unwrap();
        assert_eq!(
            prepare_onedrive_finder_assistance_with(
                &onedrive_root,
                &plan,
                &approval,
                &plan.batch_fingerprint,
                records.path(),
                22,
                |_, _, _| Ok(item.clone()),
                |_| Err("onedrive-finder-selection-failed".into()),
            )
            .unwrap_err(),
            "onedrive-finder-selection-failed"
        );
        let pending_path = std::fs::read_dir(records.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| {
                path.to_string_lossy()
                    .ends_with(".finder-assistance-pending.json")
            })
            .unwrap();
        let pending: OnedriveFinderAssistanceReceipt =
            serde_json::from_slice(&std::fs::read(pending_path).unwrap()).unwrap();
        assert!(!pending.finder_selection_requested);
        assert_eq!(
            verify_onedrive_finder_assistance_with(
                &onedrive_root,
                &pending,
                records.path(),
                23,
                |_, _, _| { Ok(item.clone()) }
            )
            .unwrap_err(),
            "onedrive-finder-assistance-receipt-invalid"
        );
        let selected = std::cell::RefCell::new(Vec::new());
        let receipt = prepare_onedrive_finder_assistance_with(
            &onedrive_root,
            &plan,
            &approval,
            &plan.batch_fingerprint,
            records.path(),
            22,
            |_, _, _| Ok(item.clone()),
            |paths| {
                selected.borrow_mut().extend_from_slice(paths);
                Ok(())
            },
        )
        .unwrap();

        assert_eq!(*selected.borrow(), vec![path(0)]);
        assert!(receipt.finder_selection_requested);
        assert!(records
            .path()
            .join(format!("{}.finder-assistance.json", receipt.receipt_id))
            .is_file());

        let forged_records = tempfile::tempdir().unwrap();
        assert_eq!(
            verify_onedrive_finder_assistance_with(
                &onedrive_root,
                &receipt,
                forged_records.path(),
                23,
                |_, _, _| Ok(item.clone()),
            )
            .unwrap_err(),
            "onedrive-finder-assistance-approval-evidence-missing"
        );

        let mut online_only = item;
        online_only.allocated_bytes = 0;
        online_only.icloud_state.downloading_status_current = false;
        online_only.icloud_state.downloading_status_not_downloaded = true;
        let verification = verify_onedrive_finder_assistance_with(
            &onedrive_root,
            &receipt,
            records.path(),
            23,
            |_, _, _| Ok(online_only.clone()),
        )
        .unwrap();
        assert!(verification.verification_complete);
        assert_eq!(verification.observed_allocation_reduction_bytes, 2_000);

        let mut unsynced = online_only.clone();
        unsynced.icloud_state.is_uploaded = false;
        let incomplete = verify_onedrive_finder_assistance_with(
            &onedrive_root,
            &receipt,
            records.path(),
            24,
            |_, _, _| Ok(unsynced.clone()),
        )
        .unwrap();
        assert!(!incomplete.verification_complete);
        assert_eq!(incomplete.verified_count, 0);

        let mut tampered = receipt;
        tampered.total_allocated_bytes_before += 1;
        tampered.receipt_id = finder_receipt_id(&tampered);
        assert_eq!(
            verify_onedrive_finder_assistance_with(
                &onedrive_root,
                &tampered,
                records.path(),
                24,
                |_, _, _| Ok(online_only.clone()),
            )
            .unwrap_err(),
            "onedrive-finder-assistance-receipt-invalid"
        );
    }

    #[test]
    fn batch_plan_rejects_duplicate_input_paths_and_tampering() {
        let duplicate = vec![path(0), path(0)];
        assert_eq!(
            plan_batch_with(&root(), &duplicate, 20, |_, _, _| Ok(safe_plan(0))).unwrap_err(),
            "icloud-local-eviction-batch-duplicate-input-path"
        );

        let mut plan =
            plan_batch_with(&root(), &[path(0)], 20, |_, _, _| Ok(safe_plan(0))).unwrap();
        plan.total_allocated_bytes += 1;
        assert_eq!(
            validate_batch_plan(&root(), &plan).unwrap_err(),
            "icloud-local-eviction-batch-plan-integrity-mismatch"
        );
    }

    #[test]
    fn batch_item_safety_enforces_file_provider_evidence() {
        let safe = safe_plan(0);
        assert!(item_plan_is_safe(&safe));

        let mut unsafe_plan = safe.clone();
        unsafe_plan.icloud_state.is_sync_paused = None;
        assert!(!item_plan_is_safe(&unsafe_plan));

        let mut unsafe_plan = safe.clone();
        unsafe_plan.icloud_state.is_trashed = Some(true);
        assert!(!item_plan_is_safe(&unsafe_plan));

        let mut unsafe_plan = safe.clone();
        unsafe_plan.icloud_state.allows_eviction = Some(false);
        assert!(!item_plan_is_safe(&unsafe_plan));

        let mut unsafe_plan = safe.clone();
        unsafe_plan.icloud_state.provider_reported_bytes = Some(safe.logical_bytes + 1);
        assert!(!item_plan_is_safe(&unsafe_plan));

        let mut unsafe_plan = safe.clone();
        unsafe_plan.icloud_state.upload_error_present = true;
        assert!(!item_plan_is_safe(&unsafe_plan));

        let mut unsafe_plan = safe.clone();
        unsafe_plan.icloud_state.downloading_status_not_downloaded = true;
        assert!(!item_plan_is_safe(&unsafe_plan));

        let mut unsafe_plan = safe;
        unsafe_plan.icloud_state.item_identifier_fingerprint = None;
        assert!(!item_plan_is_safe(&unsafe_plan));

        let mut unsafe_plan = safe_plan(0);
        unsafe_plan.provider = CloudProvider::Onedrive;
        unsafe_plan.icloud_state.observation_method =
            crate::cloud_local_eviction::IcloudStateObservationMethod::FoundationUbiquitousResourceValues;
        unsafe_plan.icloud_state.is_sync_paused = None;
        unsafe_plan.icloud_state.is_trashed = None;
        unsafe_plan.icloud_state.allows_eviction = None;
        unsafe_plan.icloud_state.provider_reported_bytes = None;
        unsafe_plan.icloud_state.item_identifier_fingerprint = None;
        assert!(!item_plan_is_safe(&unsafe_plan));
    }

    #[test]
    fn approval_requires_exact_fingerprint_human_attribution_and_rationale() {
        let plan = plan_batch_with(&root(), &[path(0)], 20, |_, _, _| Ok(safe_plan(0))).unwrap();
        let approval = approve_icloud_local_eviction_batch(
            &plan,
            &root(),
            &plan.batch_fingerprint,
            21,
            "human:operator",
            "Exact batch reviewed",
        )
        .unwrap();
        validate_batch_approval(&root(), &plan, &approval, &plan.batch_fingerprint).unwrap();

        let mut noncanonical = approval;
        noncanonical.approved_by.push(' ');
        noncanonical.approval_id = approval_id_for(
            &noncanonical.batch_fingerprint,
            noncanonical.approved_at_ms,
            &noncanonical.approved_by,
            &noncanonical.rationale,
        );
        assert!(
            validate_batch_approval(&root(), &plan, &noncanonical, &plan.batch_fingerprint)
                .is_err()
        );
        assert!(approve_icloud_local_eviction_batch(
            &plan,
            &root(),
            &"0".repeat(64),
            21,
            "human:operator",
            "Exact batch reviewed"
        )
        .is_err());
        assert!(approve_icloud_local_eviction_batch(
            &plan,
            &root(),
            &plan.batch_fingerprint,
            21,
            "automation",
            "Exact batch reviewed"
        )
        .is_err());
        assert!(approve_icloud_local_eviction_batch(
            &plan,
            &root(),
            &plan.batch_fingerprint,
            21,
            "human:operator",
            ""
        )
        .is_err());
    }

    #[test]
    fn full_preflight_stops_at_first_changed_item() {
        let plan = plan_batch_with(&root(), &[path(0), path(1), path(2)], 20, |_, path, _| {
            let name = path.file_stem().unwrap().to_string_lossy();
            let index = name.trim_start_matches("file-").parse::<usize>().unwrap();
            Ok(safe_plan(index))
        })
        .unwrap();
        let calls = Cell::new(0usize);
        let error = preflight_with(&root(), &plan, 30, |_, path, _| {
            let call = calls.get();
            calls.set(call + 1);
            let name = path.file_stem().unwrap().to_string_lossy();
            let index = name.trim_start_matches("file-").parse::<usize>().unwrap();
            let mut live = safe_plan(index);
            if index == 1 {
                live.plan_fingerprint = "f".repeat(64);
            }
            Ok(live)
        })
        .unwrap_err();
        assert_eq!(error, "icloud-local-eviction-batch-preflight-item-changed");
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn result_fingerprint_changes_with_any_outcome() {
        let mut result = IcloudLocalEvictionBatchResult {
            version: ICLOUD_LOCAL_EVICTION_BATCH_VERSION,
            result_id: String::new(),
            batch_fingerprint: "a".repeat(64),
            approval_id: "b".repeat(64),
            started_at_ms: 1,
            completed_at_ms: 1,
            input_count: 1,
            planned_count: 1,
            unavailable_count: 0,
            attempted_count: 0,
            succeeded_count: 0,
            verified_count: 0,
            total_allocated_bytes_before: 2_000,
            observed_allocation_reduction_bytes: 0,
            execution_complete: false,
            verification_complete: false,
            halted: false,
            halt_reason: None,
            item_outcomes: Vec::new(),
            notices: Vec::new(),
        };
        refresh_result_summary(&mut result, 2);
        let before = result.result_id.clone();
        result
            .item_outcomes
            .push(IcloudLocalEvictionBatchItemOutcome {
                input_index: 0,
                plan_fingerprint: "c".repeat(64),
                approval_id: "d".repeat(64),
                result_id: Some("e".repeat(64)),
                eviction_request_succeeded: true,
                verification_complete: true,
                observed_allocation_reduction_bytes: 2_000,
                error_code: None,
            });
        refresh_result_summary(&mut result, 3);
        assert_ne!(result.result_id, before);
        assert!(result.execution_complete);
        assert!(result.verification_complete);
    }

    #[derive(Default)]
    struct TestBatchRecorder {
        record_names: Vec<String>,
        fail_result_record: bool,
    }

    impl BatchRecordWriter for TestBatchRecorder {
        fn write<T: serde::Serialize>(
            &mut self,
            _record_dir: &Path,
            name: &str,
            _value: &T,
        ) -> Result<(), String> {
            self.record_names.push(name.to_string());
            if self.fail_result_record && name.ends_with(".result.json") {
                Err("test-result-record-failure".into())
            } else {
                Ok(())
            }
        }
    }

    fn plan_index(path: &Path) -> usize {
        path.file_stem()
            .unwrap()
            .to_string_lossy()
            .trim_start_matches("file-")
            .parse()
            .unwrap()
    }

    fn approved_batch(
        item_count: usize,
    ) -> (
        IcloudLocalEvictionBatchPlan,
        IcloudLocalEvictionBatchApproval,
    ) {
        let paths: Vec<_> = (0..item_count).map(path).collect();
        let plan = plan_batch_with(&root(), &paths, 20, |_, path, _| {
            Ok(safe_plan(plan_index(path)))
        })
        .unwrap();
        let approval = approve_icloud_local_eviction_batch(
            &plan,
            &root(),
            &plan.batch_fingerprint,
            21,
            "human:operator",
            "Exact batch reviewed",
        )
        .unwrap();
        (plan, approval)
    }

    fn successful_result(
        plan: &IcloudLocalEvictionPlan,
        approval: &IcloudLocalEvictionApproval,
        requested_at_ms: u64,
    ) -> IcloudLocalEvictionResult {
        IcloudLocalEvictionResult {
            version: crate::cloud_local_eviction::ICLOUD_LOCAL_EVICTION_VERSION,
            result_id: format!("{requested_at_ms:064x}"),
            plan_fingerprint: plan.plan_fingerprint.clone(),
            approval_id: approval.approval_id.clone(),
            path: plan.path.clone(),
            requested_at_ms,
            allocated_bytes_before: plan.allocated_bytes,
            allocated_bytes_after: 0,
            observed_allocation_reduction_bytes: plan.allocated_bytes,
            eviction_request_succeeded: true,
            cloud_item_path_retained: true,
            is_ubiquitous_after: true,
            is_uploaded_after: true,
            local_copy_status_not_downloaded: true,
            local_allocation_reduction_verified: true,
            verification_complete: true,
            verification_blockers: Vec::new(),
            notices: Vec::new(),
        }
    }

    #[test]
    fn onedrive_batch_uses_the_native_per_item_executor() {
        let mut onedrive = root();
        onedrive.id = "onedrive:test".into();
        onedrive.provider = CloudProvider::Onedrive;
        onedrive.label = "OneDrive test".into();
        let plan = plan_batch_with(&onedrive, &[path(0)], 20, |_, _, _| {
            let mut item = safe_plan(0);
            item.provider = CloudProvider::Onedrive;
            Ok(item)
        })
        .unwrap();
        let approval = approve_icloud_local_eviction_batch(
            &plan,
            &onedrive,
            &plan.batch_fingerprint,
            21,
            "human:operator",
            "Exact OneDrive batch reviewed",
        )
        .unwrap();
        let calls = Cell::new(0usize);
        let mut recorder = TestBatchRecorder::default();
        let result = execute_icloud_local_eviction_batch_with(
            &onedrive,
            &plan,
            &approval,
            &plan.batch_fingerprint,
            Path::new("/records"),
            30,
            |_, _, _| {
                let mut item = safe_plan(0);
                item.provider = CloudProvider::Onedrive;
                Ok(item)
            },
            |root, live_plan, individual, _, requested_at_ms| {
                assert_eq!(root.provider, CloudProvider::Onedrive);
                calls.set(calls.get() + 1);
                Ok(successful_result(live_plan, individual, requested_at_ms))
            },
            &mut recorder,
            || 100,
        )
        .unwrap();

        assert_eq!(calls.get(), 1);
        assert!(result.execution_complete);
        assert!(result.verification_complete);
    }

    #[test]
    fn execution_stops_after_first_failed_item_and_records_each_checkpoint() {
        let root = root();
        let (plan, approval) = approved_batch(3);
        let calls = Cell::new(0usize);
        let clock = Cell::new(100_u64);
        let mut requested_times = Vec::new();
        let mut recorder = TestBatchRecorder::default();

        let result = execute_icloud_local_eviction_batch_with(
            &root,
            &plan,
            &approval,
            &plan.batch_fingerprint,
            Path::new("/records"),
            30,
            |_, path, _| Ok(safe_plan(plan_index(path))),
            |_, live_plan, individual, _, requested_at_ms| {
                let call = calls.get();
                calls.set(call + 1);
                requested_times.push(requested_at_ms);
                if call == 1 {
                    Err("test-item-execution-failed".into())
                } else {
                    Ok(successful_result(live_plan, individual, requested_at_ms))
                }
            },
            &mut recorder,
            || {
                let current = clock.get();
                clock.set(current + 100);
                current
            },
        )
        .unwrap();

        assert_eq!(calls.get(), 2);
        assert_eq!(requested_times, vec![100, 200]);
        assert_eq!(result.attempted_count, 2);
        assert!(result.halted);
        assert_eq!(
            result.halt_reason.as_deref(),
            Some("icloud-local-eviction-batch-item-execution-failed")
        );
        let checkpoints: Vec<_> = recorder
            .record_names
            .iter()
            .filter(|name| name.ends_with(".batch-result.json"))
            .collect();
        assert_eq!(checkpoints.len(), 2);
        assert!(recorder.record_names.windows(2).any(
            |pair| pair[0].ends_with(".result.json") && pair[1].ends_with(".batch-result.json")
        ));
    }

    #[test]
    fn result_record_failure_halts_with_incomplete_verification_and_checkpoint() {
        let root = root();
        let (plan, approval) = approved_batch(1);
        let mut recorder = TestBatchRecorder {
            fail_result_record: true,
            ..TestBatchRecorder::default()
        };

        let result = execute_icloud_local_eviction_batch_with(
            &root,
            &plan,
            &approval,
            &plan.batch_fingerprint,
            Path::new("/records"),
            30,
            |_, path, _| Ok(safe_plan(plan_index(path))),
            |_, live_plan, individual, _, requested_at_ms| {
                Ok(successful_result(live_plan, individual, requested_at_ms))
            },
            &mut recorder,
            || 100,
        )
        .unwrap();

        assert!(result.halted);
        assert!(!result.verification_complete);
        assert_eq!(
            result.halt_reason.as_deref(),
            Some("icloud-local-eviction-batch-item-result-record-failed")
        );
        assert_eq!(
            result.item_outcomes[0].error_code.as_deref(),
            Some("icloud-local-eviction-batch-item-result-record-failed")
        );
        assert_eq!(
            recorder
                .record_names
                .iter()
                .filter(|name| name.ends_with(".batch-result.json"))
                .count(),
            1
        );
    }

    #[test]
    fn item_execution_timestamps_read_the_clock_for_each_item() {
        let next = std::cell::Cell::new(40_u64);
        let mut now_ms = || {
            let current = next.get();
            next.set(current + 7);
            current
        };
        assert_eq!(fresh_item_requested_at_ms(&mut now_ms), 40);
        assert_eq!(fresh_item_requested_at_ms(&mut now_ms), 47);
    }
}
