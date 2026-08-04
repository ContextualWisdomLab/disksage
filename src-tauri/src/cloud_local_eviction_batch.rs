//! Evidence-bound batch coordination for iCloud local-copy eviction.
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
    write_immutable_record, IcloudLocalEvictionPlan,
};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub const ICLOUD_LOCAL_EVICTION_BATCH_VERSION: u32 = 1;
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
pub struct IcloudLocalEvictionBatchItem {
    pub input_index: u32,
    pub plan: IcloudLocalEvictionPlan,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IcloudLocalEvictionBatchUnavailable {
    pub input_index: u32,
    pub error_code: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IcloudLocalEvictionBatchPlan {
    pub version: u32,
    pub provider: CloudProvider,
    pub account_scope: CloudAccountScope,
    pub cloud_root: String,
    pub observed_at_ms: u64,
    pub input_count: u32,
    pub planned_count: u32,
    pub unavailable_count: u32,
    pub total_logical_bytes: u64,
    pub total_allocated_bytes: u64,
    pub items: Vec<IcloudLocalEvictionBatchItem>,
    pub unavailable: Vec<IcloudLocalEvictionBatchUnavailable>,
    pub batch_fingerprint: String,
    pub eligible_after_human_approval: bool,
    pub blockers: Vec<String>,
    pub notices: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IcloudLocalEvictionBatchApproval {
    pub version: u32,
    pub approval_id: String,
    pub batch_fingerprint: String,
    pub approved_at_ms: u64,
    pub approved_by: String,
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IcloudLocalEvictionBatchItemOutcome {
    pub input_index: u32,
    pub plan_fingerprint: String,
    pub approval_id: String,
    pub result_id: Option<String>,
    pub eviction_request_succeeded: bool,
    pub verification_complete: bool,
    pub observed_allocation_reduction_bytes: u64,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IcloudLocalEvictionBatchResult {
    pub version: u32,
    pub result_id: String,
    pub batch_fingerprint: String,
    pub approval_id: String,
    pub started_at_ms: u64,
    pub completed_at_ms: u64,
    pub input_count: u32,
    pub planned_count: u32,
    pub unavailable_count: u32,
    pub attempted_count: u32,
    pub succeeded_count: u32,
    pub verified_count: u32,
    pub total_allocated_bytes_before: u64,
    pub observed_allocation_reduction_bytes: u64,
    pub execution_complete: bool,
    pub verification_complete: bool,
    pub halted: bool,
    pub halt_reason: Option<String>,
    pub item_outcomes: Vec<IcloudLocalEvictionBatchItemOutcome>,
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
    plan.version == crate::cloud_local_eviction::ICLOUD_LOCAL_EVICTION_VERSION
        && plan.provider == CloudProvider::Icloud
        && valid_hex64(&plan.plan_fingerprint)
        && plan.logical_bytes > 0
        && plan.allocated_bytes > 0
        && plan.eligible_after_human_approval
        && plan
            .blockers
            .iter()
            .all(|blocker| blocker == "human-local-eviction-approval-required")
        && plan.active_use.evidence_complete
        && !plan.active_use.active
        && !plan.active_use.results_truncated
        && plan.icloud_state.is_ubiquitous
        && plan.icloud_state.is_uploaded
        && !plan.icloud_state.is_uploading
        && !plan.icloud_state.is_downloading
        && plan.icloud_state.downloading_status_current
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
                plan.icloud_state.is_sync_paused.is_none()
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
        || plan.provider != CloudProvider::Icloud
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
        provider: CloudProvider::Icloud,
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
    if root.provider != CloudProvider::Icloud {
        return Err("icloud-local-eviction-batch-requires-icloud-root".into());
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
            Ok(plan) => items.push(IcloudLocalEvictionBatchItem { input_index, plan }),
            Err(error) => unavailable.push(IcloudLocalEvictionBatchUnavailable {
                input_index,
                error_code: bounded_error_code(&error),
            }),
        }
    }
    build_batch_plan(root, paths.len(), items, unavailable, observed_at_ms)
}

/// Build a bounded read-only batch plan. Unavailable paths are represented by index and a bounded,
/// path-free error code. No file content is opened and no local allocation is changed.
#[cfg(not(coverage))]
pub fn plan_icloud_local_eviction_batch(
    root: &CloudRoot,
    paths: &[PathBuf],
    observed_at_ms: u64,
) -> Result<IcloudLocalEvictionBatchPlan, String> {
    plan_batch_with(root, paths, observed_at_ms, plan_icloud_local_eviction)
}

/// Bind one attributed human decision to an exact eligible batch. This function is pure.
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
pub fn execute_icloud_local_eviction_batch(
    root: &CloudRoot,
    plan: &IcloudLocalEvictionBatchPlan,
    approval: &IcloudLocalEvictionBatchApproval,
    confirmation_batch_fingerprint: &str,
    record_dir: &Path,
    requested_at_ms: u64,
) -> Result<IcloudLocalEvictionBatchResult, String> {
    execute_icloud_local_eviction_batch_with_now(
        root,
        plan,
        approval,
        confirmation_batch_fingerprint,
        record_dir,
        requested_at_ms,
        crate::cloud::system_now_ms,
    )
}

#[cfg(not(coverage))]
fn fresh_item_requested_at_ms(now_ms: &mut impl FnMut() -> u64) -> u64 {
    now_ms()
}

#[cfg(not(coverage))]
fn execute_icloud_local_eviction_batch_with_now(
    root: &CloudRoot,
    plan: &IcloudLocalEvictionBatchPlan,
    approval: &IcloudLocalEvictionBatchApproval,
    confirmation_batch_fingerprint: &str,
    record_dir: &Path,
    requested_at_ms: u64,
    mut now_ms: impl FnMut() -> u64,
) -> Result<IcloudLocalEvictionBatchResult, String> {
    validate_batch_approval(root, plan, approval, confirmation_batch_fingerprint)?;
    let _live = preflight_with(root, plan, requested_at_ms, plan_icloud_local_eviction)?;

    write_immutable_record(
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
        write_immutable_record(
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
        let execution = execute_icloud_local_eviction(
            root,
            &item.plan,
            individual,
            &item.plan.plan_fingerprint,
            item_requested_at_ms,
        );
        match execution {
            Ok(result) => {
                let result_record = write_immutable_record(
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
        write_immutable_record(
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
            observed_at_ms: 20,
            icloud_state: IcloudLocalState {
                observation_method: IcloudStateObservationMethod::FileProviderCtlEvaluate,
                is_ubiquitous: true,
                is_uploaded: true,
                is_uploading: false,
                is_downloading: false,
                downloading_status_current: true,
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

        let mut unsafe_plan = safe;
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
