//! Path-free handoff for local ontology organization plans.
//!
//! This is a metadata contract only. It never includes source/destination paths, file names,
//! content, or provider credentials, and it cannot authorize a move or source eviction.

#![deny(missing_docs)]

use sha2::{Digest, Sha256};

use crate::organize::MovePlan;

/// Stable schema identifier for a path-free organization-lineage batch.
pub const ORGANIZATION_LINEAGE_SCHEMA: &str = "disksage.organization-lineage-batch";
/// Current version of the organization-lineage handoff schema.
pub const ORGANIZATION_LINEAGE_SCHEMA_VERSION: u32 = 1;
/// Maximum number of lineage items admitted into one exported batch.
pub const ORGANIZATION_LINEAGE_MAX_ITEMS: usize = 200;
/// Maximum serialized size of one exported organization-lineage batch.
pub const ORGANIZATION_LINEAGE_MAX_BODY_BYTES: usize = 512 * 1024;

const MAX_DATETIME_EPOCH_MS: u64 = 253_402_300_799_999;

/// Path-free lineage facts derived from one reviewed local organization plan.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrganizationLineageItem {
    /// Stable fingerprint binding this item to the source evidence used by the plan.
    pub lineage_fingerprint: String,
    /// Source byte size observed when the organization plan was materialized.
    pub source_size: u64,
    /// Source modification time observed when the organization plan was materialized.
    pub source_mtime_ms: u64,
    /// Selected production timestamp used by the organization decision.
    pub production_time_ms: u64,
    /// Evidence source from which the selected production time was derived.
    pub production_time_source: String,
    /// Confidence label attached to the selected production-time evidence.
    pub production_time_confidence: String,
    /// HTTPS ontology class assigned by the reviewed organization plan.
    pub ontology_class: String,
    /// Semantic relation represented by the destination without disclosing its path.
    pub destination_relation: String,
    /// Planned organization action represented by this lineage item.
    pub action: String,
}

/// Complete immutable handoff containing a bounded set of path-free organization-lineage items.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrganizationLineageBatch {
    /// Stable schema identifier serialized as `schema`.
    #[serde(rename = "schema")]
    pub schema_kind: String,
    /// Version of the organization-lineage handoff schema.
    pub version: u32,
    /// Time at which DiskSage generated this complete lineage batch.
    pub generated_at_ms: u64,
    /// Whether the batch represents the complete bounded organization-plan input.
    pub complete: bool,
    /// SHA-256 digest of the canonical unsigned batch used to bind the exported item set.
    pub batch_fingerprint_sha256: String,
    /// Path-free lineage items included in this batch.
    pub items: Vec<OrganizationLineageItem>,
}

fn valid_lower_hex_64(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn bounded_text(value: &str, max_chars: usize) -> bool {
    let count = value.chars().count();
    count > 0 && count <= max_chars && !value.chars().any(|character| character.is_control())
}

fn unsigned_batch(
    generated_at_ms: u64,
    items: Vec<OrganizationLineageItem>,
) -> OrganizationLineageBatch {
    OrganizationLineageBatch {
        schema_kind: ORGANIZATION_LINEAGE_SCHEMA.into(),
        version: ORGANIZATION_LINEAGE_SCHEMA_VERSION,
        generated_at_ms,
        complete: true,
        batch_fingerprint_sha256: String::new(),
        items,
    }
}

/// Export a complete, path-free organization plan for Naruon/semantic-data-portal.
///
/// The export validates materialized lineage metadata, rejects duplicate source fingerprints,
/// and binds the resulting item set with a SHA-256 batch fingerprint. It carries no path or
/// content authority and cannot execute, approve, or undo an organization move.
pub fn export_move_plans(
    plans: &[MovePlan],
    generated_at_ms: u64,
) -> Result<OrganizationLineageBatch, String> {
    if generated_at_ms == 0 || generated_at_ms > MAX_DATETIME_EPOCH_MS {
        return Err("organization-lineage-generated-time-out-of-bounds".into());
    }
    if plans.is_empty() {
        return Err("organization-lineage-items-empty".into());
    }
    if plans.len() > ORGANIZATION_LINEAGE_MAX_ITEMS {
        return Err("organization-lineage-item-limit-exceeded".into());
    }

    let mut items = Vec::with_capacity(plans.len());
    for plan in plans {
        let lineage = &plan.lineage;
        let production_time_ms = lineage
            .production_time_ms
            .ok_or_else(|| "organization-lineage-production-time-missing".to_string())?;
        let production_time_source = lineage
            .production_time_source
            .as_deref()
            .ok_or_else(|| "organization-lineage-production-source-missing".to_string())?;
        let production_time_confidence = lineage
            .production_time_confidence
            .as_deref()
            .ok_or_else(|| "organization-lineage-production-confidence-missing".to_string())?;

        if !valid_lower_hex_64(&lineage.lineage_fingerprint)
            || production_time_ms == 0
            || production_time_ms > MAX_DATETIME_EPOCH_MS
            || !bounded_text(production_time_source, 256)
            || !matches!(
                production_time_confidence,
                "high" | "medium" | "low" | "unknown"
            )
            || !bounded_text(&plan.class_id, 512)
            || !plan.class_id.starts_with("https://")
        {
            return Err("organization-lineage-metadata-invalid".into());
        }
        let source_size = plan
            .source_size
            .ok_or_else(|| "organization-lineage-source-size-missing".to_string())?;
        let source_mtime_ms = plan
            .source_mtime_ms
            .ok_or_else(|| "organization-lineage-source-mtime-missing".to_string())?;
        items.push(OrganizationLineageItem {
            lineage_fingerprint: lineage.lineage_fingerprint.clone(),
            source_size,
            source_mtime_ms,
            production_time_ms,
            production_time_source: production_time_source.into(),
            production_time_confidence: production_time_confidence.into(),
            ontology_class: plan.class_id.clone(),
            destination_relation: "targetFolder".into(),
            action: "move".into(),
        });
    }

    let mut fingerprints = std::collections::BTreeSet::new();
    if items
        .iter()
        .any(|item| !fingerprints.insert(item.lineage_fingerprint.as_str()))
    {
        return Err("organization-lineage-fingerprint-duplicate".into());
    }

    let mut batch = unsigned_batch(generated_at_ms, items);
    let unsigned = serde_json::to_vec(&batch).map_err(|_| "organization-lineage-json-invalid")?;
    let digest = Sha256::digest(unsigned);
    batch.batch_fingerprint_sha256 = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    let encoded = serde_json::to_vec(&batch).map_err(|_| "organization-lineage-json-invalid")?;
    if encoded.len() > ORGANIZATION_LINEAGE_MAX_BODY_BYTES {
        return Err("organization-lineage-body-limit-exceeded".into());
    }
    Ok(batch)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::organize::LineageMetadata;

    fn plan(fingerprint: &str) -> MovePlan {
        MovePlan {
            src: "/private/source/secret.mov".into(),
            dst: "/Users/example/Media/Media/secret.mov".into(),
            class_id: "https://disksage.app/ontology#Media".into(),
            source_size: Some(42),
            source_mtime_ms: Some(123),
            lineage: LineageMetadata {
                production_time_ms: Some(456),
                production_time_source: Some("embedded:exiftool:MediaCreateDate".into()),
                production_time_confidence: Some("high".into()),
                lineage_fingerprint: fingerprint.into(),
            },
        }
    }

    #[test]
    fn export_is_path_free_and_self_fingerprinted() {
        let batch = export_move_plans(&[plan(&"a".repeat(64))], 1_000).unwrap();
        let json = serde_json::to_string(&batch).unwrap();
        assert!(!json.contains("secret.mov"));
        assert!(!json.contains("/private/source"));
        assert_eq!(batch.items[0].destination_relation, "targetFolder");
        assert_eq!(batch.batch_fingerprint_sha256.len(), 64);
    }

    #[test]
    fn export_rejects_unmaterialized_plan_metadata() {
        let mut candidate = plan(&"b".repeat(64));
        candidate.lineage.production_time_ms = None;
        assert_eq!(
            export_move_plans(&[candidate], 1_000).unwrap_err(),
            "organization-lineage-production-time-missing"
        );
    }

    #[test]
    fn export_accepts_a_realistic_multi_file_batch() {
        let plans = (0..33)
            .map(|index| plan(&format!("{index:064x}")))
            .collect::<Vec<_>>();
        let batch = export_move_plans(&plans, 1_000).unwrap();
        assert_eq!(batch.items.len(), 33);
        assert!(batch.complete);
    }
}
