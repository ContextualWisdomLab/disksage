//! Destination-bound, read-only approval planning for validated incomplete-download output.
//!
//! This module binds one integrity-checked materialization plan to one cloud root, relative
//! destination directory, provider-authoritative capacity snapshot, and exact output names. It
//! never creates a directory or file and never grants approval by itself.

use crate::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use crate::incomplete_download_materialization::{
    incomplete_download_materialization_integrity_valid, IncompleteDownloadMaterializationReport,
    MaterializationUnitKind,
};
use crate::provider_capacity::{
    assess_capacity, root_with_verified_capacity_scope, CapacityEvidenceKind,
    CloudCapacityAssessment, CloudCapacitySnapshot, CAPACITY_SCHEMA_VERSION,
};
use std::collections::BTreeSet;
use std::path::{Component, Path};

pub const INCOMPLETE_DOWNLOAD_DESTINATION_PLAN_VERSION: u32 = 1;
pub const MAX_CAPACITY_AGE_MS: u64 = 5 * 60 * 1_000;
const MAX_DESTINATION_SUBDIRECTORY_BYTES: usize = 1_024;
const MAX_DESTINATION_DEPTH: usize = 16;
const MAX_REVIEW_TEXT_BYTES: usize = 2_048;
const HUMAN_APPROVAL_BLOCKER: &str = "human-materialization-destination-approval-required";

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncompleteDownloadDestinationUnit {
    pub materialization_unit_fingerprint: String,
    pub kind: MaterializationUnitKind,
    pub output_bytes: u64,
    pub destination_relative_path: String,
    pub destination_exists: bool,
    pub write_performed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncompleteDownloadDestinationPlan {
    pub schema_version: u32,
    pub observed_at_ms: u64,
    pub source_scope_fingerprint: String,
    pub audit_fingerprint: String,
    pub validation_fingerprint: String,
    pub materialization_plan_fingerprint: String,
    pub provider: CloudProvider,
    pub account_scope: CloudAccountScope,
    pub cloud_root_id: String,
    pub cloud_root: String,
    pub destination_subdirectory: String,
    pub capacity: CloudCapacityAssessment,
    pub source_file_count: usize,
    pub unit_count: usize,
    pub planned_output_bytes: u64,
    pub destination_plan_fingerprint: String,
    pub eligible_after_human_approval: bool,
    pub blockers: Vec<String>,
    pub notices: Vec<String>,
    pub exact_approval_available: bool,
    pub approval_issued: bool,
    pub mutation_performed: bool,
    pub units: Vec<IncompleteDownloadDestinationUnit>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncompleteDownloadDestinationApproval {
    pub schema_version: u32,
    pub approval_id: String,
    pub destination_plan_fingerprint: String,
    pub approved_at_ms: u64,
    pub approved_by: String,
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DestinationCapacitySummary {
    pub evidence_kind: CapacityEvidenceKind,
    pub evidence_fingerprint: Option<String>,
    pub observed_at_ms: u64,
    pub remaining_bytes: Option<u64>,
    pub requested_bytes: u64,
    pub reserve_bytes: u64,
    pub required_bytes: Option<u64>,
    pub can_fit: Option<bool>,
    pub blockers: Vec<String>,
    pub notices: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncompleteDownloadDestinationPlanSummary {
    pub schema_version: u32,
    pub output_mode: String,
    pub observed_at_ms: u64,
    pub source_scope_fingerprint: String,
    pub audit_fingerprint: String,
    pub validation_fingerprint: String,
    pub materialization_plan_fingerprint: String,
    pub provider: CloudProvider,
    pub account_scope: CloudAccountScope,
    pub capacity: DestinationCapacitySummary,
    pub source_file_count: usize,
    pub unit_count: usize,
    pub planned_output_bytes: u64,
    pub destination_plan_fingerprint: String,
    pub eligible_after_human_approval: bool,
    pub blockers: Vec<String>,
    pub notices: Vec<String>,
    pub exact_approval_available: bool,
    pub exact_approval_phrase: Option<String>,
    pub approval_issued: bool,
    pub mutation_performed: bool,
    pub production_time_assigned: bool,
    pub filename_date_used_as_production_time: bool,
    pub filesystem_times_used_only_for_source_stability: bool,
    pub destination_paths_and_names_redacted: bool,
    pub redacted_from_summary: Vec<String>,
}

fn valid_hex64(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_destination_subdirectory(value: &str) -> bool {
    let path = Path::new(value);
    !value.is_empty()
        && value.len() <= MAX_DESTINATION_SUBDIRECTORY_BYTES
        && !path.is_absolute()
        && path.components().count() <= MAX_DESTINATION_DEPTH
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn valid_output_name(value: &str) -> bool {
    let path = Path::new(value);
    !value.is_empty()
        && value.len() <= 255
        && !value.bytes().any(|byte| byte.is_ascii_control())
        && matches!(
            path.components().collect::<Vec<_>>().as_slice(),
            [Component::Normal(_)]
        )
}

fn validate_existing_directory_prefix(root: &Path, relative: &Path) -> Result<(), String> {
    let mut current = root.to_path_buf();
    let mut missing_parent = false;
    for component in relative.components() {
        let Component::Normal(value) = component else {
            return Err("materialization-destination-subdirectory-unsafe".into());
        };
        current.push(value);
        if missing_parent {
            continue;
        }
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err("materialization-destination-symlink-component".into())
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Err("materialization-destination-parent-not-directory".into())
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing_parent = true;
            }
            Err(_) => return Err("materialization-destination-parent-unavailable".into()),
        }
    }
    Ok(())
}

fn destination_plan_fingerprint(
    source_scope_fingerprint: &str,
    audit_fingerprint: &str,
    validation_fingerprint: &str,
    materialization_plan_fingerprint: &str,
    provider: CloudProvider,
    account_scope: CloudAccountScope,
    cloud_root_id: &str,
    cloud_root: &str,
    destination_subdirectory: &str,
    capacity: &CloudCapacityAssessment,
    source_file_count: usize,
    unit_count: usize,
    planned_output_bytes: u64,
    units: &[IncompleteDownloadDestinationUnit],
    eligible_after_human_approval: bool,
    blockers: &[String],
    notices: &[String],
) -> Result<String, String> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-incomplete-download-destination-plan-v1\0");
    for value in [
        source_scope_fingerprint,
        audit_fingerprint,
        validation_fingerprint,
        materialization_plan_fingerprint,
        cloud_root_id,
        provider.as_str(),
        account_scope.as_str(),
        cloud_root,
        destination_subdirectory,
    ] {
        hasher.update(value.as_bytes());
        hasher.update(&[0]);
    }
    let capacity_json = serde_json::to_vec(capacity)
        .map_err(|_| "materialization-destination-capacity-json-invalid".to_string())?;
    hasher.update(&capacity_json);
    hasher.update(&[0]);
    hasher.update(&(source_file_count as u64).to_le_bytes());
    hasher.update(&(unit_count as u64).to_le_bytes());
    hasher.update(&planned_output_bytes.to_le_bytes());
    for unit in units {
        hasher.update(unit.materialization_unit_fingerprint.as_bytes());
        hasher.update(&[0]);
        hasher.update(unit.destination_relative_path.as_bytes());
        hasher.update(&[0]);
        hasher.update(&[u8::from(unit.destination_exists)]);
        hasher.update(&[u8::from(unit.write_performed)]);
    }
    hasher.update(&[u8::from(eligible_after_human_approval)]);
    for value in blockers.iter().chain(notices) {
        hasher.update(value.as_bytes());
        hasher.update(&[0]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

fn approval_id_for(
    destination_plan_fingerprint: &str,
    approved_at_ms: u64,
    approved_by: &str,
    rationale: &str,
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-incomplete-download-destination-approval-v1\0");
    hasher.update(destination_plan_fingerprint.as_bytes());
    hasher.update(&[0]);
    hasher.update(&approved_at_ms.to_le_bytes());
    hasher.update(approved_by.as_bytes());
    hasher.update(&[0]);
    hasher.update(rationale.as_bytes());
    hasher.finalize().to_hex().to_string()
}

pub fn destination_approval_phrase(plan: &IncompleteDownloadDestinationPlan) -> Option<String> {
    if !incomplete_download_destination_plan_integrity_valid(plan)
        || !plan.eligible_after_human_approval
        || !plan.exact_approval_available
        || !valid_hex64(&plan.destination_plan_fingerprint)
    {
        return None;
    }
    Some(format!(
        "DiskSage 복구 materialization {} {} {} {} 승인 {}",
        plan.provider.as_str(),
        plan.account_scope.as_str(),
        plan.unit_count,
        plan.planned_output_bytes,
        plan.destination_plan_fingerprint
    ))
}

pub fn plan_incomplete_download_destination(
    materialization: &IncompleteDownloadMaterializationReport,
    discovered_root: &CloudRoot,
    destination_subdirectory: &str,
    capacity_snapshot: CloudCapacitySnapshot,
    reserve_bytes: u64,
    observed_at_ms: u64,
) -> Result<IncompleteDownloadDestinationPlan, String> {
    if !incomplete_download_materialization_integrity_valid(materialization) {
        return Err("materialization-destination-source-plan-integrity-invalid".into());
    }
    if !valid_destination_subdirectory(destination_subdirectory) {
        return Err("materialization-destination-subdirectory-unsafe".into());
    }
    if capacity_snapshot.schema_version != CAPACITY_SCHEMA_VERSION
        || capacity_snapshot.observed_at_ms == 0
        || capacity_snapshot.observed_at_ms > observed_at_ms
    {
        return Err("materialization-destination-capacity-snapshot-invalid".into());
    }
    let refined_root = root_with_verified_capacity_scope(discovered_root, &capacity_snapshot)?;
    let root_path = Path::new(&refined_root.path);
    if !root_path.is_absolute()
        || root_path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err("materialization-destination-cloud-root-unsafe".into());
    }
    let root_metadata = std::fs::symlink_metadata(root_path)
        .map_err(|_| "materialization-destination-cloud-root-unavailable".to_string())?;
    if !root_metadata.is_dir() || root_metadata.file_type().is_symlink() {
        return Err("materialization-destination-cloud-root-unsafe".into());
    }
    let canonical_root = std::fs::canonicalize(root_path)
        .map_err(|_| "materialization-destination-cloud-root-unavailable".to_string())?;
    let relative_directory = Path::new(destination_subdirectory);
    validate_existing_directory_prefix(&canonical_root, relative_directory)?;

    let largest_output_bytes = materialization
        .units
        .iter()
        .map(|unit| unit.output_bytes)
        .max()
        .unwrap_or_default();
    let capacity = assess_capacity(
        capacity_snapshot,
        materialization.planned_output_bytes,
        largest_output_bytes,
        reserve_bytes,
    );
    let mut blockers = Vec::new();
    let mut notices = vec![
        "read-only-no-destination-created".into(),
        "source-rename-discard-and-cloud-eviction-not-authorized".into(),
        "provider-sync-attestation-required-before-source-eviction".into(),
        "production-time-remains-unassigned".into(),
    ];
    if !refined_root.readable {
        blockers.push("materialization-destination-cloud-root-not-readable".into());
    }
    if refined_root.access_issue.is_some() {
        blockers.push("materialization-destination-cloud-root-access-issue".into());
    }
    if refined_root.account_scope == CloudAccountScope::Unknown {
        blockers.push("materialization-destination-account-scope-unverified".into());
    }
    if observed_at_ms.saturating_sub(capacity.snapshot.observed_at_ms) > MAX_CAPACITY_AGE_MS {
        blockers.push("materialization-destination-capacity-stale".into());
    }
    if !capacity
        .snapshot
        .evidence_fingerprint
        .as_deref()
        .is_some_and(valid_hex64)
    {
        blockers.push("materialization-destination-capacity-fingerprint-missing".into());
    }
    if capacity.can_fit != Some(true) {
        blockers.extend(capacity.blockers.iter().cloned());
        if capacity.blockers.is_empty() {
            blockers.push("materialization-destination-capacity-unverified".into());
        }
    }
    notices.extend(capacity.notices.iter().cloned());

    let mut units = Vec::with_capacity(materialization.units.len());
    let mut destination_paths = BTreeSet::new();
    for unit in &materialization.units {
        if !valid_output_name(&unit.suggested_filename) {
            return Err("materialization-destination-output-name-unsafe".into());
        }
        let relative_path = relative_directory.join(&unit.suggested_filename);
        let relative_path_text = relative_path.to_string_lossy().into_owned();
        if !destination_paths.insert(relative_path_text.clone()) {
            return Err("materialization-destination-output-name-duplicate".into());
        }
        let destination = canonical_root.join(&relative_path);
        let destination_exists = match std::fs::symlink_metadata(&destination) {
            Ok(_) => true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(_) => return Err("materialization-destination-output-probe-failed".into()),
        };
        if destination_exists {
            blockers.push("materialization-destination-output-exists".into());
        }
        units.push(IncompleteDownloadDestinationUnit {
            materialization_unit_fingerprint: unit.unit_fingerprint.clone(),
            kind: unit.kind,
            output_bytes: unit.output_bytes,
            destination_relative_path: relative_path_text,
            destination_exists,
            write_performed: false,
        });
    }

    blockers.sort();
    blockers.dedup();
    notices.sort();
    notices.dedup();
    let eligible_after_human_approval = blockers.is_empty();
    if eligible_after_human_approval {
        blockers.push(HUMAN_APPROVAL_BLOCKER.into());
    }
    let canonical_root_text = canonical_root.to_string_lossy().into_owned();
    let fingerprint = destination_plan_fingerprint(
        &materialization.source_scope_fingerprint,
        &materialization.audit_fingerprint,
        &materialization.validation_fingerprint,
        &materialization.plan_fingerprint,
        refined_root.provider,
        refined_root.account_scope,
        &refined_root.id,
        &canonical_root_text,
        destination_subdirectory,
        &capacity,
        materialization.source_file_count,
        units.len(),
        materialization.planned_output_bytes,
        &units,
        eligible_after_human_approval,
        &blockers,
        &notices,
    )?;

    Ok(IncompleteDownloadDestinationPlan {
        schema_version: INCOMPLETE_DOWNLOAD_DESTINATION_PLAN_VERSION,
        observed_at_ms,
        source_scope_fingerprint: materialization.source_scope_fingerprint.clone(),
        audit_fingerprint: materialization.audit_fingerprint.clone(),
        validation_fingerprint: materialization.validation_fingerprint.clone(),
        materialization_plan_fingerprint: materialization.plan_fingerprint.clone(),
        provider: refined_root.provider,
        account_scope: refined_root.account_scope,
        cloud_root_id: refined_root.id,
        cloud_root: canonical_root_text,
        destination_subdirectory: destination_subdirectory.into(),
        capacity,
        source_file_count: materialization.source_file_count,
        unit_count: units.len(),
        planned_output_bytes: materialization.planned_output_bytes,
        destination_plan_fingerprint: fingerprint,
        eligible_after_human_approval,
        blockers,
        notices,
        exact_approval_available: eligible_after_human_approval,
        approval_issued: false,
        mutation_performed: false,
        units,
    })
}

fn sorted_unique(values: &[String]) -> bool {
    !values.windows(2).any(|items| items[0] >= items[1])
}

pub fn incomplete_download_destination_plan_integrity_valid(
    plan: &IncompleteDownloadDestinationPlan,
) -> bool {
    let cloud_root = Path::new(&plan.cloud_root);
    if plan.schema_version != INCOMPLETE_DOWNLOAD_DESTINATION_PLAN_VERSION
        || ![
            plan.source_scope_fingerprint.as_str(),
            plan.audit_fingerprint.as_str(),
            plan.validation_fingerprint.as_str(),
            plan.materialization_plan_fingerprint.as_str(),
            plan.destination_plan_fingerprint.as_str(),
        ]
        .iter()
        .all(|value| valid_hex64(value))
        || plan.account_scope == CloudAccountScope::Unknown
        || plan.cloud_root_id.is_empty()
        || !cloud_root.is_absolute()
        || cloud_root
            .components()
            .any(|component| matches!(component, Component::ParentDir))
        || !valid_destination_subdirectory(&plan.destination_subdirectory)
        || plan.source_file_count == 0
        || plan.unit_count != plan.units.len()
        || plan.units.is_empty()
        || plan.planned_output_bytes == 0
        || plan.approval_issued
        || plan.mutation_performed
        || !sorted_unique(&plan.blockers)
        || !sorted_unique(&plan.notices)
    {
        return false;
    }
    let approval_state_valid = if plan.eligible_after_human_approval {
        plan.exact_approval_available
            && plan.blockers == [HUMAN_APPROVAL_BLOCKER.to_string()]
            && plan.capacity.can_fit == Some(true)
            && plan.capacity.blockers.is_empty()
            && plan.capacity.snapshot.evidence_kind != CapacityEvidenceKind::Unavailable
            && plan
                .capacity
                .snapshot
                .evidence_fingerprint
                .as_deref()
                .is_some_and(valid_hex64)
            && plan.capacity.snapshot.observed_at_ms <= plan.observed_at_ms
            && plan
                .observed_at_ms
                .saturating_sub(plan.capacity.snapshot.observed_at_ms)
                <= MAX_CAPACITY_AGE_MS
            && plan.units.iter().all(|unit| !unit.destination_exists)
    } else {
        !plan.exact_approval_available
            && !plan.blockers.is_empty()
            && !plan
                .blockers
                .iter()
                .any(|value| value == HUMAN_APPROVAL_BLOCKER)
    };
    if !approval_state_valid
        || plan.capacity.snapshot.schema_version != CAPACITY_SCHEMA_VERSION
        || plan.capacity.snapshot.provider != plan.provider
        || plan
            .capacity
            .snapshot
            .account_scope
            .is_some_and(|scope| scope != plan.account_scope)
        || plan.capacity.requested_bytes != plan.planned_output_bytes
    {
        return false;
    }

    let mut unit_fingerprints = BTreeSet::new();
    let mut destination_paths = BTreeSet::new();
    let mut byte_total = 0u64;
    let mut largest_output = 0u64;
    for unit in &plan.units {
        let relative = Path::new(&unit.destination_relative_path);
        let file_name = relative
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        if !valid_hex64(&unit.materialization_unit_fingerprint)
            || unit.output_bytes == 0
            || relative.is_absolute()
            || relative.parent() != Some(Path::new(&plan.destination_subdirectory))
            || !valid_output_name(file_name)
            || unit.write_performed
            || !unit_fingerprints.insert(unit.materialization_unit_fingerprint.as_str())
            || !destination_paths.insert(unit.destination_relative_path.as_str())
        {
            return false;
        }
        byte_total = byte_total.saturating_add(unit.output_bytes);
        largest_output = largest_output.max(unit.output_bytes);
    }
    if byte_total != plan.planned_output_bytes
        || largest_output != plan.capacity.largest_candidate_bytes
    {
        return false;
    }
    let expected_capacity = assess_capacity(
        plan.capacity.snapshot.clone(),
        plan.capacity.requested_bytes,
        plan.capacity.largest_candidate_bytes,
        plan.capacity.reserve_bytes,
    );
    if expected_capacity != plan.capacity {
        return false;
    }

    destination_plan_fingerprint(
        &plan.source_scope_fingerprint,
        &plan.audit_fingerprint,
        &plan.validation_fingerprint,
        &plan.materialization_plan_fingerprint,
        plan.provider,
        plan.account_scope,
        &plan.cloud_root_id,
        &plan.cloud_root,
        &plan.destination_subdirectory,
        &plan.capacity,
        plan.source_file_count,
        plan.unit_count,
        plan.planned_output_bytes,
        &plan.units,
        plan.eligible_after_human_approval,
        &plan.blockers,
        &plan.notices,
    )
    .is_ok_and(|fingerprint| fingerprint == plan.destination_plan_fingerprint)
}

pub fn approve_incomplete_download_destination(
    plan: &IncompleteDownloadDestinationPlan,
    confirmed_plan_fingerprint: &str,
    approved_at_ms: u64,
    approved_by: &str,
    rationale: &str,
) -> Result<IncompleteDownloadDestinationApproval, String> {
    if !incomplete_download_destination_plan_integrity_valid(plan)
        || plan.destination_plan_fingerprint != confirmed_plan_fingerprint
    {
        return Err("materialization-destination-plan-fingerprint-mismatch".into());
    }
    if !plan.eligible_after_human_approval
        || !plan.exact_approval_available
        || plan.blockers != [HUMAN_APPROVAL_BLOCKER.to_string()]
        || plan.approval_issued
        || plan.mutation_performed
    {
        return Err("materialization-destination-plan-not-eligible".into());
    }
    if approved_at_ms < plan.observed_at_ms {
        return Err("materialization-destination-approval-predates-plan".into());
    }
    let approved_by = approved_by.trim();
    let rationale = rationale.trim();
    if !approved_by.starts_with("human:")
        || approved_by.len() <= "human:".len()
        || approved_by.len() > MAX_REVIEW_TEXT_BYTES
    {
        return Err("materialization-destination-human-attribution-required".into());
    }
    if rationale.is_empty() || rationale.len() > MAX_REVIEW_TEXT_BYTES {
        return Err("materialization-destination-rationale-invalid".into());
    }
    Ok(IncompleteDownloadDestinationApproval {
        schema_version: INCOMPLETE_DOWNLOAD_DESTINATION_PLAN_VERSION,
        approval_id: approval_id_for(
            &plan.destination_plan_fingerprint,
            approved_at_ms,
            approved_by,
            rationale,
        ),
        destination_plan_fingerprint: plan.destination_plan_fingerprint.clone(),
        approved_at_ms,
        approved_by: approved_by.into(),
        rationale: rationale.into(),
    })
}

pub fn validate_incomplete_download_destination_approval(
    plan: &IncompleteDownloadDestinationPlan,
    approval: &IncompleteDownloadDestinationApproval,
    confirmed_plan_fingerprint: &str,
) -> Result<(), String> {
    if !incomplete_download_destination_plan_integrity_valid(plan)
        || approval.schema_version != INCOMPLETE_DOWNLOAD_DESTINATION_PLAN_VERSION
        || approval.destination_plan_fingerprint != plan.destination_plan_fingerprint
        || approval.destination_plan_fingerprint != confirmed_plan_fingerprint
        || approval.approval_id
            != approval_id_for(
                &approval.destination_plan_fingerprint,
                approval.approved_at_ms,
                &approval.approved_by,
                &approval.rationale,
            )
    {
        return Err("materialization-destination-approval-integrity-mismatch".into());
    }
    if approval.approved_at_ms < plan.observed_at_ms
        || !approval.approved_by.starts_with("human:")
        || approval.approved_by.len() <= "human:".len()
        || approval.approved_by.len() > MAX_REVIEW_TEXT_BYTES
        || approval.rationale.trim().is_empty()
        || approval.rationale.len() > MAX_REVIEW_TEXT_BYTES
    {
        return Err("materialization-destination-approval-invalid".into());
    }
    Ok(())
}

pub fn summarize_incomplete_download_destination(
    plan: &IncompleteDownloadDestinationPlan,
) -> IncompleteDownloadDestinationPlanSummary {
    IncompleteDownloadDestinationPlanSummary {
        schema_version: plan.schema_version,
        output_mode: "incomplete-download-destination-plan-summary".into(),
        observed_at_ms: plan.observed_at_ms,
        source_scope_fingerprint: plan.source_scope_fingerprint.clone(),
        audit_fingerprint: plan.audit_fingerprint.clone(),
        validation_fingerprint: plan.validation_fingerprint.clone(),
        materialization_plan_fingerprint: plan.materialization_plan_fingerprint.clone(),
        provider: plan.provider,
        account_scope: plan.account_scope,
        capacity: DestinationCapacitySummary {
            evidence_kind: plan.capacity.snapshot.evidence_kind,
            evidence_fingerprint: plan.capacity.snapshot.evidence_fingerprint.clone(),
            observed_at_ms: plan.capacity.snapshot.observed_at_ms,
            remaining_bytes: plan.capacity.snapshot.remaining_bytes,
            requested_bytes: plan.capacity.requested_bytes,
            reserve_bytes: plan.capacity.reserve_bytes,
            required_bytes: plan.capacity.required_bytes,
            can_fit: plan.capacity.can_fit,
            blockers: plan.capacity.blockers.clone(),
            notices: plan.capacity.notices.clone(),
        },
        source_file_count: plan.source_file_count,
        unit_count: plan.unit_count,
        planned_output_bytes: plan.planned_output_bytes,
        destination_plan_fingerprint: plan.destination_plan_fingerprint.clone(),
        eligible_after_human_approval: plan.eligible_after_human_approval,
        blockers: plan.blockers.clone(),
        notices: plan.notices.clone(),
        exact_approval_available: plan.exact_approval_available,
        exact_approval_phrase: destination_approval_phrase(plan),
        approval_issued: false,
        mutation_performed: false,
        production_time_assigned: false,
        filename_date_used_as_production_time: false,
        filesystem_times_used_only_for_source_stability: true,
        destination_paths_and_names_redacted: true,
        redacted_from_summary: vec![
            "absolute-source-root".into(),
            "relative-source-path".into(),
            "source-range-offsets".into(),
            "source-content-digests".into(),
            "cloud-root-id".into(),
            "absolute-cloud-root".into(),
            "destination-subdirectory".into(),
            "destination-relative-paths".into(),
            "destination-filenames".into(),
        ],
    }
}

#[cfg(all(test, not(coverage)))]
mod tests {
    use super::*;
    use crate::incomplete_download::{
        collect_incomplete_download_audit, DEFAULT_MAX_ENTRIES, DEFAULT_STALE_AFTER_DAYS,
    };
    use crate::incomplete_download_materialization::{
        plan_incomplete_download_materialization, MATERIALIZATION_ACTIVE_USE_TEST_LOCK,
    };
    use crate::incomplete_download_recovery::{
        validate_incomplete_download_recovery, RecoveryValidationLimits,
    };
    use crate::provider_capacity::parse_icloud_brctl_quota;
    use crate::provider_capacity::DEFAULT_CAPACITY_RESERVE_BYTES;
    use std::io::Write;

    fn write_zip(path: &Path, payload: &[u8]) {
        let file = std::fs::File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file(
                "payload.bin",
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(payload).unwrap();
        let file = writer.finish().unwrap();
        file.sync_all().unwrap();
        drop(file);
    }

    fn materialization(root: &Path) -> IncompleteDownloadMaterializationReport {
        let created = std::fs::metadata(root).unwrap().modified().unwrap();
        let observed_at_ms = created
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
            + 31 * 86_400_000;
        let audit = collect_incomplete_download_audit(
            root,
            observed_at_ms,
            DEFAULT_MAX_ENTRIES,
            DEFAULT_STALE_AFTER_DAYS,
        )
        .unwrap();
        let recovery = validate_incomplete_download_recovery(
            root,
            &audit,
            observed_at_ms + 1,
            RecoveryValidationLimits::default(),
        )
        .unwrap();
        plan_incomplete_download_materialization(root, &audit, &recovery, observed_at_ms + 2)
            .unwrap()
    }

    fn root(path: &Path) -> CloudRoot {
        CloudRoot {
            id: "icloud:test".into(),
            provider: CloudProvider::Icloud,
            account_scope: CloudAccountScope::Unknown,
            label: "iCloud".into(),
            path: path.to_string_lossy().into_owned(),
            readable: true,
            access_issue: None,
        }
    }

    #[test]
    fn plans_capacity_bound_destination_and_approval_without_writes() {
        let _active_use_guard = MATERIALIZATION_ACTIVE_USE_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let source = tempfile::tempdir().unwrap();
        let cloud = tempfile::tempdir().unwrap();
        write_zip(
            &source.path().join("whole.zip.crdownload"),
            b"validated payload",
        );
        let materialization = materialization(source.path());
        let observed_at_ms = materialization.observed_at_ms + 10;
        let capacity = parse_icloud_brctl_quota(
            "10000000000 bytes of quota remaining in personal account\n",
            observed_at_ms,
        )
        .unwrap();
        let plan = plan_incomplete_download_destination(
            &materialization,
            &root(cloud.path()),
            "DiskSage/Recovered/IncompleteDownloads",
            capacity,
            DEFAULT_CAPACITY_RESERVE_BYTES,
            observed_at_ms,
        )
        .unwrap();

        assert_eq!(plan.account_scope, CloudAccountScope::Personal);
        assert!(plan.eligible_after_human_approval);
        assert_eq!(plan.blockers, [HUMAN_APPROVAL_BLOCKER.to_string()]);
        assert!(plan.exact_approval_available);
        assert_eq!(plan.unit_count, 1);
        assert!(!cloud.path().join("DiskSage").exists());
        assert!(!plan.units[0].destination_exists);
        assert!(!plan.units[0].write_performed);
        assert!(destination_approval_phrase(&plan)
            .unwrap()
            .ends_with(&plan.destination_plan_fingerprint));

        let approval = approve_incomplete_download_destination(
            &plan,
            &plan.destination_plan_fingerprint,
            observed_at_ms + 1,
            "human:test",
            "reviewed exact destination plan",
        )
        .unwrap();
        validate_incomplete_download_destination_approval(
            &plan,
            &approval,
            &plan.destination_plan_fingerprint,
        )
        .unwrap();
        assert!(approve_incomplete_download_destination(
            &plan,
            &plan.destination_plan_fingerprint,
            observed_at_ms + 1,
            "human:test",
            &"x".repeat(MAX_REVIEW_TEXT_BYTES + 1),
        )
        .is_err());
        let mut tampered_plan = plan.clone();
        tampered_plan.source_file_count += 1;
        assert!(!incomplete_download_destination_plan_integrity_valid(
            &tampered_plan
        ));
        assert!(approve_incomplete_download_destination(
            &tampered_plan,
            &tampered_plan.destination_plan_fingerprint,
            observed_at_ms + 2,
            "human:test",
            "must reject tampered plan",
        )
        .is_err());
        assert!(!cloud.path().join("DiskSage").exists());

        let summary = summarize_incomplete_download_destination(&plan);
        let encoded = serde_json::to_string(&summary).unwrap();
        assert!(!encoded.contains(cloud.path().to_string_lossy().as_ref()));
        assert!(!encoded.contains(&plan.destination_subdirectory));
        assert!(!encoded.contains(materialization.units[0].suggested_filename.as_str()));
        assert!(!summary.production_time_assigned);
        assert!(!summary.filename_date_used_as_production_time);
    }

    #[test]
    fn rejects_tampered_materialization_and_stale_capacity() {
        let _active_use_guard = MATERIALIZATION_ACTIVE_USE_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let source = tempfile::tempdir().unwrap();
        let cloud = tempfile::tempdir().unwrap();
        write_zip(
            &source.path().join("whole.zip.crdownload"),
            b"validated payload",
        );
        let materialization = materialization(source.path());
        let observed_at_ms = materialization.observed_at_ms + MAX_CAPACITY_AGE_MS + 10;
        let stale_capacity = parse_icloud_brctl_quota(
            "10000000000 bytes of quota remaining in personal account\n",
            materialization.observed_at_ms,
        )
        .unwrap();
        let stale_plan = plan_incomplete_download_destination(
            &materialization,
            &root(cloud.path()),
            "DiskSage/Recovered",
            stale_capacity,
            DEFAULT_CAPACITY_RESERVE_BYTES,
            observed_at_ms,
        )
        .unwrap();
        assert!(!stale_plan.eligible_after_human_approval);
        assert!(stale_plan
            .blockers
            .contains(&"materialization-destination-capacity-stale".into()));

        let mut tampered = materialization;
        tampered.units[0].output_bytes += 1;
        let capacity = parse_icloud_brctl_quota(
            "10000000000 bytes of quota remaining in personal account\n",
            observed_at_ms,
        )
        .unwrap();
        assert!(plan_incomplete_download_destination(
            &tampered,
            &root(cloud.path()),
            "DiskSage/Recovered",
            capacity,
            DEFAULT_CAPACITY_RESERVE_BYTES,
            observed_at_ms,
        )
        .is_err());
    }

    #[test]
    fn existing_destination_blocks_exact_approval() {
        let _active_use_guard = MATERIALIZATION_ACTIVE_USE_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let source = tempfile::tempdir().unwrap();
        let cloud = tempfile::tempdir().unwrap();
        write_zip(
            &source.path().join("whole.zip.crdownload"),
            b"validated payload",
        );
        let materialization = materialization(source.path());
        let destination_directory = cloud.path().join("DiskSage/Recovered");
        std::fs::create_dir_all(&destination_directory).unwrap();
        std::fs::write(
            destination_directory.join(&materialization.units[0].suggested_filename),
            b"existing",
        )
        .unwrap();
        let observed_at_ms = materialization.observed_at_ms + 10;
        let capacity = parse_icloud_brctl_quota(
            "10000000000 bytes of quota remaining in personal account\n",
            observed_at_ms,
        )
        .unwrap();
        let plan = plan_incomplete_download_destination(
            &materialization,
            &root(cloud.path()),
            "DiskSage/Recovered",
            capacity,
            DEFAULT_CAPACITY_RESERVE_BYTES,
            observed_at_ms,
        )
        .unwrap();
        assert!(!plan.eligible_after_human_approval);
        assert!(!plan.exact_approval_available);
        assert!(destination_approval_phrase(&plan).is_none());
        assert!(plan
            .blockers
            .contains(&"materialization-destination-output-exists".into()));
    }
}
