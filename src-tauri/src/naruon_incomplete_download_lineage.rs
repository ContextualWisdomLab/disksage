//! Redacted, read-only export of verified incomplete-download materialization lineage.
//!
//! The immutable execution receipt contains private local paths. This contract keeps the
//! fingerprints, byte ranges, content digests, capacity evidence, and approval chronology needed
//! by Naruon while replacing every source and destination path with a receipt-scoped opaque
//! reference. It performs no provider, filesystem, or source mutation.

use std::collections::BTreeMap;
use std::fmt::Write;

use sha2::{Digest, Sha256};

use crate::cloud::{CloudAccountScope, CloudProvider};
use crate::content_digest::ContentDigests;
use crate::incomplete_download_materialization::{
    incomplete_download_materialization_integrity_valid, IncompleteDownloadMaterializationReport,
    MaterializationUnitKind,
};
use crate::incomplete_download_materialization_destination::{
    incomplete_download_destination_plan_integrity_valid,
    validate_incomplete_download_destination_approval, IncompleteDownloadDestinationApproval,
    IncompleteDownloadDestinationPlan,
};
use crate::incomplete_download_materialization_execution::{
    incomplete_download_materialization_receipt_integrity_valid,
    IncompleteDownloadMaterializationReceipt,
};
use crate::provider_capacity::CloudCapacityAssessment;

pub const NARUON_INCOMPLETE_DOWNLOAD_LINEAGE_SCHEMA_VERSION: u32 = 1;
pub const NARUON_INCOMPLETE_DOWNLOAD_LINEAGE_SCHEMA_KIND: &str =
    "disksage.incomplete-download-materialization-lineage";

const EVIDENCE_PRECEDENCE: [&str; 4] = [
    "embedded_metadata",
    "explicit_filename_date",
    "filesystem_created_at",
    "filesystem_modified_at",
];

const REDACTED_FIELDS: [&str; 7] = [
    "cloud-root",
    "destination-subdirectory",
    "source-relative-paths",
    "destination-relative-paths",
    "source-and-destination-file-names",
    "reviewer-identity",
    "review-rationale",
];

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NaruonIncompleteDownloadApprovalLineage {
    pub approval_id: String,
    pub destination_plan_observed_at_ms: u64,
    pub approved_at_ms: u64,
    pub actor_kind: String,
    pub attribution_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NaruonUnassignedProductionTimeLineage {
    pub assigned: bool,
    pub selected_value_ms: Option<u64>,
    pub selected_source: Option<String>,
    pub evidence_precedence: Vec<String>,
    pub filename_date_used_as_production_time: bool,
    pub filesystem_times_used_only_for_source_stability: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NaruonIncompleteDownloadMaterializedUnitLineage {
    pub materialization_unit_fingerprint: String,
    pub source_candidate_fingerprint: String,
    pub source_ref: String,
    pub source_logical_bytes: u64,
    pub kind: MaterializationUnitKind,
    pub range_start: u64,
    pub range_end: u64,
    pub output_bytes: u64,
    pub content_digests: ContentDigests,
    pub destination_ref: String,
    pub source_stable: bool,
    pub output_verified: bool,
    pub write_performed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NaruonIncompleteDownloadRedactionLineage {
    pub cloud_root_redacted: bool,
    pub source_paths_redacted: bool,
    pub destination_paths_redacted: bool,
    pub review_text_redacted: bool,
    pub redacted_fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NaruonIncompleteDownloadMaterializationLineageEnvelope {
    pub schema_version: u32,
    pub schema_kind: String,
    pub receipt_id: String,
    pub source_scope_fingerprint: String,
    pub audit_fingerprint: String,
    pub validation_fingerprint: String,
    pub materialization_plan_fingerprint: String,
    pub destination_plan_fingerprint: String,
    pub provider: CloudProvider,
    pub destination_account_scope: CloudAccountScope,
    pub executed_at_ms: u64,
    pub capacity: CloudCapacityAssessment,
    pub approval: NaruonIncompleteDownloadApprovalLineage,
    pub source_file_count: usize,
    pub unit_count: usize,
    pub materialized_bytes: u64,
    pub units: Vec<NaruonIncompleteDownloadMaterializedUnitLineage>,
    pub all_outputs_verified: bool,
    pub provider_write_executed: bool,
    pub provider_sync_confirmed: bool,
    pub source_eviction_authorized: bool,
    pub source_mutation_performed: bool,
    pub production_time: NaruonUnassignedProductionTimeLineage,
    pub redaction: NaruonIncompleteDownloadRedactionLineage,
}

fn opaque_ref(domain: &[u8], receipt_id: &str, value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(receipt_id.as_bytes());
    hasher.update(&[0]);
    hasher.update(value.as_bytes());
    let digest = hasher.finalize();
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        write!(&mut encoded, "{byte:02x}").expect("writing a digest to String cannot fail");
    }
    encoded
}

fn attribution_fingerprint(
    receipt_id: &str,
    approval: &IncompleteDownloadDestinationApproval,
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-naruon-incomplete-download-approval-attribution-v1\0");
    hasher.update(receipt_id.as_bytes());
    hasher.update(&[0]);
    hasher.update(approval.approved_by.as_bytes());
    hasher.update(&[0]);
    hasher.update(approval.rationale.as_bytes());
    hasher.finalize().to_hex().to_string()
}

/// Export a path-free lineage envelope from a verified materialization execution.
///
/// The original report, destination plan, and attributed approval are required so the exporter can
/// prove the immutable receipt is bound to the exact source audit and human-approved destination
/// decision. Only receipt-scoped opaque references leave this function.
pub fn export_naruon_incomplete_download_materialization_lineage(
    receipt: &IncompleteDownloadMaterializationReceipt,
    materialization: &IncompleteDownloadMaterializationReport,
    plan: &IncompleteDownloadDestinationPlan,
    approval: &IncompleteDownloadDestinationApproval,
) -> Result<NaruonIncompleteDownloadMaterializationLineageEnvelope, String> {
    if !incomplete_download_materialization_receipt_integrity_valid(receipt)
        || !incomplete_download_materialization_integrity_valid(materialization)
        || !incomplete_download_destination_plan_integrity_valid(plan)
    {
        return Err("naruon-incomplete-download-lineage-integrity-invalid".into());
    }
    validate_incomplete_download_destination_approval(
        plan,
        approval,
        &receipt.destination_plan_fingerprint,
    )
    .map_err(|_| "naruon-incomplete-download-lineage-approval-invalid".to_string())?;

    if receipt.receipt_id.is_empty()
        || receipt.materialization_plan_fingerprint != materialization.plan_fingerprint
        || receipt.materialization_plan_fingerprint != plan.materialization_plan_fingerprint
        || receipt.destination_plan_fingerprint != plan.destination_plan_fingerprint
        || receipt.approval_id != approval.approval_id
        || receipt.provider != plan.provider
        || receipt.account_scope != plan.account_scope
        || receipt.executed_at_ms < approval.approved_at_ms
        || receipt.source_file_count != materialization.source_file_count
        || receipt.unit_count != materialization.unit_count
        || receipt.materialized_bytes != materialization.planned_output_bytes
        || receipt.units.len() != materialization.units.len()
        || receipt.units.len() != plan.units.len()
    {
        return Err("naruon-incomplete-download-lineage-binding-mismatch".into());
    }

    let source_units = materialization
        .units
        .iter()
        .map(|unit| (unit.unit_fingerprint.as_str(), unit))
        .collect::<BTreeMap<_, _>>();
    let destination_units = plan
        .units
        .iter()
        .map(|unit| (unit.materialization_unit_fingerprint.as_str(), unit))
        .collect::<BTreeMap<_, _>>();
    if source_units.len() != materialization.units.len()
        || destination_units.len() != plan.units.len()
    {
        return Err("naruon-incomplete-download-lineage-unit-duplicate".into());
    }

    let mut units = Vec::with_capacity(receipt.units.len());
    for receipt_unit in &receipt.units {
        let source = source_units
            .get(receipt_unit.materialization_unit_fingerprint.as_str())
            .ok_or_else(|| "naruon-incomplete-download-lineage-source-unit-missing".to_string())?;
        let destination = destination_units
            .get(receipt_unit.materialization_unit_fingerprint.as_str())
            .ok_or_else(|| {
                "naruon-incomplete-download-lineage-destination-unit-missing".to_string()
            })?;
        if receipt_unit.kind != source.kind
            || receipt_unit.kind != destination.kind
            || receipt_unit.source_relative_path != source.source_relative_path
            || receipt_unit.destination_relative_path != destination.destination_relative_path
            || receipt_unit.range_start != source.range_start
            || receipt_unit.range_end != source.range_end
            || receipt_unit.output_bytes != source.output_bytes
            || receipt_unit.output_bytes != destination.output_bytes
            || receipt_unit.content_digests != source.content_digests
        {
            return Err("naruon-incomplete-download-lineage-unit-binding-mismatch".into());
        }
        units.push(NaruonIncompleteDownloadMaterializedUnitLineage {
            materialization_unit_fingerprint: receipt_unit.materialization_unit_fingerprint.clone(),
            source_candidate_fingerprint: source.candidate_fingerprint.clone(),
            source_ref: opaque_ref(
                b"disksage-naruon-incomplete-download-source-ref-v1\0",
                &receipt.receipt_id,
                &source.candidate_fingerprint,
            ),
            source_logical_bytes: source.source_logical_bytes,
            kind: receipt_unit.kind,
            range_start: receipt_unit.range_start,
            range_end: receipt_unit.range_end,
            output_bytes: receipt_unit.output_bytes,
            content_digests: receipt_unit.content_digests.clone(),
            destination_ref: opaque_ref(
                b"disksage-naruon-incomplete-download-destination-ref-v1\0",
                &receipt.receipt_id,
                &receipt_unit.materialization_unit_fingerprint,
            ),
            source_stable: receipt_unit.source_stable,
            output_verified: receipt_unit.output_verified,
            write_performed: receipt_unit.write_performed,
        });
    }

    Ok(NaruonIncompleteDownloadMaterializationLineageEnvelope {
        schema_version: NARUON_INCOMPLETE_DOWNLOAD_LINEAGE_SCHEMA_VERSION,
        schema_kind: NARUON_INCOMPLETE_DOWNLOAD_LINEAGE_SCHEMA_KIND.into(),
        receipt_id: receipt.receipt_id.clone(),
        source_scope_fingerprint: materialization.source_scope_fingerprint.clone(),
        audit_fingerprint: materialization.audit_fingerprint.clone(),
        validation_fingerprint: materialization.validation_fingerprint.clone(),
        materialization_plan_fingerprint: materialization.plan_fingerprint.clone(),
        destination_plan_fingerprint: plan.destination_plan_fingerprint.clone(),
        provider: receipt.provider,
        destination_account_scope: receipt.account_scope,
        executed_at_ms: receipt.executed_at_ms,
        capacity: receipt.fresh_capacity.clone(),
        approval: NaruonIncompleteDownloadApprovalLineage {
            approval_id: approval.approval_id.clone(),
            destination_plan_observed_at_ms: plan.observed_at_ms,
            approved_at_ms: approval.approved_at_ms,
            actor_kind: "human".into(),
            attribution_fingerprint: attribution_fingerprint(&receipt.receipt_id, approval),
        },
        source_file_count: receipt.source_file_count,
        unit_count: receipt.unit_count,
        materialized_bytes: receipt.materialized_bytes,
        units,
        all_outputs_verified: receipt.all_outputs_verified,
        provider_write_executed: false,
        provider_sync_confirmed: receipt.provider_sync_confirmed,
        source_eviction_authorized: receipt.source_eviction_authorized,
        source_mutation_performed: receipt.source_mutation_performed,
        production_time: NaruonUnassignedProductionTimeLineage {
            assigned: false,
            selected_value_ms: None,
            selected_source: None,
            evidence_precedence: EVIDENCE_PRECEDENCE.map(str::to_string).to_vec(),
            filename_date_used_as_production_time: false,
            filesystem_times_used_only_for_source_stability: true,
        },
        redaction: NaruonIncompleteDownloadRedactionLineage {
            cloud_root_redacted: true,
            source_paths_redacted: true,
            destination_paths_redacted: true,
            review_text_redacted: true,
            redacted_fields: REDACTED_FIELDS.map(str::to_string).to_vec(),
        },
    })
}
