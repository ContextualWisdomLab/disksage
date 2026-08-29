//! Path-redacted, pre-copy catalog preview export for semantic-data-portal.
//!
//! This module is deterministic and read-only. It does not call a model, persist catalog data,
//! copy a file, or authorize source eviction.

use std::collections::BTreeSet;

use crate::cloud::{
    self, ArchiveKind, CloudAccountScope, CloudPlanReport, CloudProvider, MetadataEvidence,
};

pub const SEMANTIC_CATALOG_SCHEMA: &str = "disksage.file-catalog-candidate-batch";
pub const SEMANTIC_CATALOG_SCHEMA_VERSION: u32 = 1;
pub const SEMANTIC_CATALOG_MAX_BODY_BYTES: usize = 2 * 1024 * 1024;
pub const SEMANTIC_CATALOG_MAX_CANDIDATES: usize = 200;
pub const PRODUCTION_TIME_PRECEDENCE: [&str; 4] = [
    "embedded_metadata",
    "explicit_filename_date",
    "filesystem_created",
    "filesystem_modified",
];

const MAX_DATETIME_EPOCH_MS: u64 = 253_402_300_799_999;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticCatalogMetadataEvidence {
    pub field: String,
    pub value: String,
    pub source: String,
    pub confidence: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticCatalogDatasetColumnProfile {
    pub name: String,
    pub inferred_type: String,
    pub observed_values: u64,
    pub missing_values: u64,
    pub sensitive_name: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticCatalogDatasetProfile {
    pub format: String,
    pub sampled_rows: u64,
    pub sampled_worksheets: u64,
    pub worksheet_names: Vec<String>,
    pub profile_complete: bool,
    pub sample_truncated: bool,
    pub columns: Vec<SemanticCatalogDatasetColumnProfile>,
    pub quality_warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticCatalogCandidate {
    pub candidate_fingerprint: String,
    pub review_fingerprint: String,
    pub destination_provider: CloudProvider,
    pub destination_account_scope: CloudAccountScope,
    pub archive_kind: ArchiveKind,
    pub bytes: u64,
    pub created_ms: u64,
    pub modified_ms: u64,
    pub production_time_ms: u64,
    pub production_time_source: String,
    pub production_time_confidence: String,
    pub requires_review: bool,
    pub review_reasons: Vec<String>,
    pub content_title: Option<String>,
    pub content_authors: Vec<String>,
    pub content_context: Vec<String>,
    pub duration_ms: Option<u64>,
    pub dataset_profile: Option<SemanticCatalogDatasetProfile>,
    pub metadata_evidence: Vec<SemanticCatalogMetadataEvidence>,
    pub blocked_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticCatalogCandidateBatch {
    #[serde(rename = "schema")]
    pub schema_kind: String,
    pub version: u32,
    pub production_time_precedence: Vec<String>,
    pub generated_at_ms: u64,
    pub candidates: Vec<SemanticCatalogCandidate>,
}

fn valid_lower_hex_64(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn bounded_text(value: &str, max_chars: usize) -> bool {
    let count = value.chars().count();
    count > 0 && count <= max_chars
}

fn validate_text_list(
    values: &[String],
    max_items: usize,
    max_chars: usize,
    error: &str,
) -> Result<(), String> {
    if values.len() > max_items || values.iter().any(|value| !bounded_text(value, max_chars)) {
        return Err(error.into());
    }
    Ok(())
}

fn valid_confidence(value: &str) -> bool {
    matches!(value, "high" | "medium" | "low" | "unknown")
}

fn production_time_source_class(source: &str) -> Result<&'static str, String> {
    if source.starts_with("embedded:") {
        Ok("embedded_metadata")
    } else {
        match source {
            "filename:path-token" => Ok("explicit_filename_date"),
            "filesystem:created" => Ok("filesystem_created"),
            "filesystem:modified-fallback" => Ok("filesystem_modified"),
            _ => Err("semantic-catalog-production-time-source-unsupported".into()),
        }
    }
}

fn evidence_source_class(evidence: &SemanticCatalogMetadataEvidence) -> Option<&'static str> {
    if evidence.field == "production-date" && evidence.source.starts_with("embedded:") {
        Some("embedded_metadata")
    } else {
        match (evidence.field.as_str(), evidence.source.as_str()) {
            ("filename-date-hint", "filename:path-token") => Some("explicit_filename_date"),
            ("filesystem-created-date", "filesystem:created") => Some("filesystem_created"),
            ("filesystem-modified-date", "filesystem:modified") => Some("filesystem_modified"),
            _ => None,
        }
    }
}

fn source_rank(source_class: &str) -> Result<usize, String> {
    PRODUCTION_TIME_PRECEDENCE
        .iter()
        .position(|value| *value == source_class)
        .ok_or_else(|| "semantic-catalog-production-time-source-unsupported".to_string())
}

fn validate_dataset_profile(profile: &SemanticCatalogDatasetProfile) -> Result<(), String> {
    if !bounded_text(&profile.format, 64)
        || profile.worksheet_names.len() > 128
        || profile.columns.len() > 512
        || profile.quality_warnings.len() > 128
        || profile
            .worksheet_names
            .iter()
            .chain(profile.quality_warnings.iter())
            .any(|value| !bounded_text(value, 256))
        || profile.columns.iter().any(|column| {
            !bounded_text(&column.name, 256) || !bounded_text(&column.inferred_type, 64)
        })
    {
        return Err("semantic-catalog-dataset-profile-out-of-bounds".into());
    }
    Ok(())
}

fn validate_candidate(candidate: &SemanticCatalogCandidate) -> Result<(), String> {
    if !valid_lower_hex_64(&candidate.candidate_fingerprint)
        || !valid_lower_hex_64(&candidate.review_fingerprint)
    {
        return Err("semantic-catalog-fingerprint-invalid".into());
    }
    if candidate.modified_ms == 0
        || candidate.production_time_ms == 0
        || candidate.created_ms > MAX_DATETIME_EPOCH_MS
        || candidate.modified_ms > MAX_DATETIME_EPOCH_MS
        || candidate.production_time_ms > MAX_DATETIME_EPOCH_MS
    {
        return Err("semantic-catalog-time-out-of-bounds".into());
    }
    if !bounded_text(&candidate.production_time_source, 256)
        || !valid_confidence(&candidate.production_time_confidence)
    {
        return Err("semantic-catalog-production-time-invalid".into());
    }
    if candidate
        .content_title
        .as_deref()
        .is_some_and(|value| !bounded_text(value, 1024))
        || candidate
            .blocked_reason
            .as_deref()
            .is_some_and(|value| !bounded_text(value, 256))
    {
        return Err("semantic-catalog-optional-text-out-of-bounds".into());
    }
    validate_text_list(
        &candidate.review_reasons,
        128,
        256,
        "semantic-catalog-review-reasons-out-of-bounds",
    )?;
    validate_text_list(
        &candidate.content_authors,
        64,
        256,
        "semantic-catalog-content-authors-out-of-bounds",
    )?;
    validate_text_list(
        &candidate.content_context,
        128,
        1024,
        "semantic-catalog-content-context-out-of-bounds",
    )?;
    if candidate.requires_review != !candidate.review_reasons.is_empty() {
        return Err("semantic-catalog-review-state-mismatch".into());
    }
    if candidate.metadata_evidence.len() > 256
        || candidate.metadata_evidence.iter().any(|evidence| {
            !bounded_text(&evidence.field, 128)
                || !bounded_text(&evidence.value, 2048)
                || !bounded_text(&evidence.source, 256)
                || !valid_confidence(&evidence.confidence)
        })
    {
        return Err("semantic-catalog-metadata-evidence-out-of-bounds".into());
    }
    if let Some(profile) = &candidate.dataset_profile {
        validate_dataset_profile(profile)?;
    }

    let source_class = production_time_source_class(&candidate.production_time_source)?;
    if source_class != "embedded_metadata" && candidate.production_time_confidence != "low" {
        return Err("semantic-catalog-non-embedded-confidence-invalid".into());
    }
    let (expected_field, expected_source) = match source_class {
        "embedded_metadata" => ("production-date", candidate.production_time_source.as_str()),
        "explicit_filename_date" => ("filename-date-hint", "filename:path-token"),
        "filesystem_created" => ("filesystem-created-date", "filesystem:created"),
        "filesystem_modified" => ("filesystem-modified-date", "filesystem:modified"),
        _ => return Err("semantic-catalog-production-time-source-unsupported".into()),
    };
    let selected_date = cloud::date_value(candidate.production_time_ms);
    if !candidate.metadata_evidence.iter().any(|evidence| {
        evidence.field == expected_field
            && evidence.source == expected_source
            && evidence.value == selected_date
    }) {
        return Err("semantic-catalog-selected-production-evidence-mismatch".into());
    }

    let selected_rank = source_rank(source_class)?;
    if candidate
        .metadata_evidence
        .iter()
        .filter_map(evidence_source_class)
        .any(|evidence_class| {
            source_rank(evidence_class)
                .map(|rank| rank < selected_rank)
                .unwrap_or(true)
        })
    {
        return Err("semantic-catalog-production-time-precedence-violation".into());
    }
    Ok(())
}

fn export_candidate(candidate: &crate::cloud::CloudCandidate) -> SemanticCatalogCandidate {
    SemanticCatalogCandidate {
        candidate_fingerprint: candidate.metadata_fingerprint.clone(),
        review_fingerprint: candidate.review_fingerprint.clone(),
        destination_provider: candidate.provider,
        destination_account_scope: candidate.destination_account_scope,
        archive_kind: candidate.kind,
        bytes: candidate.bytes,
        created_ms: candidate.created_ms,
        modified_ms: candidate.modified_ms,
        production_time_ms: candidate.production_time_ms,
        production_time_source: candidate.production_time_source.clone(),
        production_time_confidence: candidate.production_time_confidence.clone(),
        requires_review: candidate.requires_review,
        review_reasons: candidate.review_reasons.clone(),
        content_title: candidate.content_title.clone(),
        content_authors: candidate.content_authors.clone(),
        content_context: candidate.content_context.clone(),
        duration_ms: candidate.duration_ms,
        dataset_profile: candidate.dataset_profile.as_ref().map(|profile| {
            SemanticCatalogDatasetProfile {
                format: profile.format.clone(),
                sampled_rows: profile.sampled_rows,
                sampled_worksheets: profile.sampled_worksheets,
                worksheet_names: profile.worksheet_names.clone(),
                profile_complete: profile.profile_complete,
                sample_truncated: profile.sample_truncated,
                columns: profile
                    .columns
                    .iter()
                    .map(|column| SemanticCatalogDatasetColumnProfile {
                        name: column.name.clone(),
                        inferred_type: column.inferred_type.clone(),
                        observed_values: column.observed_values,
                        missing_values: column.missing_values,
                        sensitive_name: column.sensitive_name,
                    })
                    .collect(),
                quality_warnings: profile.quality_warnings.clone(),
            }
        }),
        metadata_evidence: candidate
            .metadata_evidence
            .iter()
            .map(
                |MetadataEvidence {
                     field,
                     value,
                     source,
                     confidence,
                 }| SemanticCatalogMetadataEvidence {
                    field: field.clone(),
                    value: value.clone(),
                    source: source.clone(),
                    confidence: confidence.clone(),
                },
            )
            .collect(),
        blocked_reason: candidate.blocked_reason.clone(),
    }
}

/// Export one destination-specific dry-run plan for semantic-data-portal preview.
///
/// Storage coordinates and names are deliberately absent from the output type. The output still
/// contains private embedded content metadata and must be sent only to an approved catalog endpoint.
pub fn export_semantic_catalog_candidate_batch(
    report: &CloudPlanReport,
) -> Result<SemanticCatalogCandidateBatch, String> {
    if report.generated_at_ms == 0 || report.generated_at_ms > MAX_DATETIME_EPOCH_MS {
        return Err("semantic-catalog-generated-time-out-of-bounds".into());
    }
    if report.candidates.is_empty() {
        return Err("semantic-catalog-candidates-empty".into());
    }
    if report.candidates.len() > SEMANTIC_CATALOG_MAX_CANDIDATES {
        return Err("semantic-catalog-candidate-limit-exceeded".into());
    }

    let candidates = report
        .candidates
        .iter()
        .map(export_candidate)
        .collect::<Vec<_>>();
    let mut fingerprints = BTreeSet::new();
    for candidate in &candidates {
        validate_candidate(candidate)?;
        if !fingerprints.insert(candidate.candidate_fingerprint.as_str()) {
            return Err("semantic-catalog-candidate-fingerprint-duplicate".into());
        }
    }

    let batch = SemanticCatalogCandidateBatch {
        schema_kind: SEMANTIC_CATALOG_SCHEMA.into(),
        version: SEMANTIC_CATALOG_SCHEMA_VERSION,
        production_time_precedence: PRODUCTION_TIME_PRECEDENCE
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
        generated_at_ms: report.generated_at_ms,
        candidates,
    };
    let encoded = serde_json::to_vec_pretty(&batch).map_err(|_| "semantic-catalog-json-invalid")?;
    if encoded.len().saturating_add(1) > SEMANTIC_CATALOG_MAX_BODY_BYTES {
        return Err("semantic-catalog-body-limit-exceeded".into());
    }
    Ok(batch)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::{CloudCandidate, CloudRoot, ExactDuplicateSummary};

    fn evidence(field: &str, value: &str, source: &str, confidence: &str) -> MetadataEvidence {
        MetadataEvidence {
            field: field.into(),
            value: value.into(),
            source: source.into(),
            confidence: confidence.into(),
        }
    }

    fn candidate() -> CloudCandidate {
        CloudCandidate {
            metadata_fingerprint: "a".repeat(64),
            review_fingerprint: "b".repeat(64),
            src: "/Users/private/Downloads/report.pdf".into(),
            dst: "/Users/private/Cloud/report.pdf".into(),
            provider: CloudProvider::Icloud,
            destination_account_scope: CloudAccountScope::Personal,
            kind: ArchiveKind::Document,
            bytes: 4096,
            age_days: 90,
            created_ms: 1_767_355_001_000,
            modified_ms: 1_767_355_002_000,
            production_time_ms: 1_767_355_000_000,
            production_time_source: "embedded:ooxml:created".into(),
            production_time_confidence: "high".into(),
            source_root: "/Users/private/Downloads".into(),
            relative_path: "report.pdf".into(),
            source_context: "private-source-context".into(),
            requires_review: false,
            review_reasons: Vec::new(),
            content_title: Some("Private title".into()),
            content_authors: vec!["Private author".into()],
            content_context: vec!["Private context".into()],
            duration_ms: None,
            dataset_profile: None,
            metadata_evidence: vec![
                evidence(
                    "production-date",
                    "2026-01-02",
                    "embedded:ooxml:created",
                    "high",
                ),
                evidence(
                    "filename-date-hint",
                    "2025-12-31",
                    "filename:path-token",
                    "low",
                ),
                evidence(
                    "filesystem-created-date",
                    "2026-01-02",
                    "filesystem:created",
                    "low",
                ),
                evidence(
                    "filesystem-modified-date",
                    "2026-01-02",
                    "filesystem:modified",
                    "medium",
                ),
            ],
            blocked_reason: None,
        }
    }

    fn report(candidates: Vec<CloudCandidate>) -> CloudPlanReport {
        CloudPlanReport {
            cloud_root: CloudRoot {
                id: "private-account-id".into(),
                provider: CloudProvider::Icloud,
                account_scope: CloudAccountScope::Personal,
                label: "private@example.com".into(),
                path: "/Users/private/Cloud".into(),
                readable: true,
                access_issue: None,
            },
            generated_at_ms: 1_784_900_000_000,
            source_selection_policy: Some(crate::cloud::CloudPlanOptions::default()),
            candidate_bytes: candidates.iter().map(|candidate| candidate.bytes).sum(),
            potentially_reclaimable_bytes: candidates
                .iter()
                .filter(|candidate| candidate.blocked_reason.is_none())
                .map(|candidate| candidate.bytes)
                .sum(),
            candidates,
            exact_duplicates: ExactDuplicateSummary::default(),
            capacity: None,
            local_volume: None,
            pre_copy_evidence: None,
            notices: vec!["dry-run-only".into()],
        }
    }

    #[test]
    fn exports_contract_without_storage_coordinates_or_filenames() {
        let batch = export_semantic_catalog_candidate_batch(&report(vec![candidate()])).unwrap();
        let value = serde_json::to_value(&batch).unwrap();
        let encoded = serde_json::to_string(&batch).unwrap();

        assert_eq!(value["schema"], SEMANTIC_CATALOG_SCHEMA);
        assert_eq!(value["version"], 1);
        assert_eq!(
            value["production_time_precedence"],
            serde_json::json!(PRODUCTION_TIME_PRECEDENCE)
        );
        assert_eq!(value["candidates"][0]["content_title"], "Private title");
        let properties = value["candidates"][0].as_object().unwrap();
        for forbidden in [
            "src",
            "dst",
            "source_root",
            "relative_path",
            "source_context",
            "filename",
            "account_id",
            "object_id",
            "locator",
        ] {
            assert!(!properties.contains_key(forbidden));
        }
        for redacted in [
            "/Users/private",
            "report.pdf",
            "private-account-id",
            "private@example.com",
            "private-source-context",
        ] {
            assert!(!encoded.contains(redacted));
        }
    }

    #[test]
    fn enforces_metadata_precedence_and_selected_evidence_binding() {
        let mut filename_selected = candidate();
        filename_selected.production_time_source = "filename:path-token".into();
        filename_selected.production_time_confidence = "low".into();
        filename_selected.production_time_ms = 1_767_182_400_000;
        filename_selected.requires_review = true;
        filename_selected.review_reasons =
            vec!["production-date-not-from-embedded-metadata".into()];
        assert_eq!(
            export_semantic_catalog_candidate_batch(&report(vec![filename_selected])).unwrap_err(),
            "semantic-catalog-production-time-precedence-violation"
        );

        let mut missing_evidence = candidate();
        missing_evidence.metadata_evidence.clear();
        assert_eq!(
            export_semantic_catalog_candidate_batch(&report(vec![missing_evidence])).unwrap_err(),
            "semantic-catalog-selected-production-evidence-mismatch"
        );
    }

    #[test]
    fn accepts_only_the_next_available_metadata_fallback() {
        let baseline = candidate();
        for (source, time, retained_fields) in [
            (
                "filename:path-token",
                1_767_182_400_000,
                vec![
                    "filename-date-hint",
                    "filesystem-created-date",
                    "filesystem-modified-date",
                ],
            ),
            (
                "filesystem:created",
                1_767_355_001_000,
                vec!["filesystem-created-date", "filesystem-modified-date"],
            ),
            (
                "filesystem:modified-fallback",
                1_767_355_002_000,
                vec!["filesystem-modified-date"],
            ),
        ] {
            let mut fallback = baseline.clone();
            fallback.production_time_source = source.into();
            fallback.production_time_confidence = "low".into();
            fallback.production_time_ms = time;
            fallback.requires_review = true;
            fallback.review_reasons = vec!["production-date-not-from-embedded-metadata".into()];
            fallback
                .metadata_evidence
                .retain(|item| retained_fields.contains(&item.field.as_str()));
            assert!(export_semantic_catalog_candidate_batch(&report(vec![fallback])).is_ok());
        }
    }

    #[test]
    fn rejects_duplicate_unbounded_and_oversized_batches() {
        let repeated = candidate();
        assert_eq!(
            export_semantic_catalog_candidate_batch(&report(vec![repeated.clone(), repeated,]))
                .unwrap_err(),
            "semantic-catalog-candidate-fingerprint-duplicate"
        );

        let mut candidates = Vec::new();
        for index in 0..20u64 {
            let mut item = candidate();
            item.metadata_fingerprint = format!("{index:064x}");
            item.content_context = vec!["x".repeat(1024); 128];
            candidates.push(item);
        }
        assert_eq!(
            export_semantic_catalog_candidate_batch(&report(candidates)).unwrap_err(),
            "semantic-catalog-body-limit-exceeded"
        );

        let too_many = vec![candidate(); SEMANTIC_CATALOG_MAX_CANDIDATES + 1];
        assert_eq!(
            export_semantic_catalog_candidate_batch(&report(too_many)).unwrap_err(),
            "semantic-catalog-candidate-limit-exceeded"
        );
    }
}
