//! Public validation-boundary coverage for semantic catalog preview export.
//!
//! These tests exercise the shipped read-only export boundary with synthetic metadata only. They
//! never access a provider, network, model, credential, or filesystem mutation path.

use disksage_lib::cloud::{
    ArchiveKind, CloudAccountScope, CloudCandidate, CloudPlanOptions, CloudPlanReport, CloudProvider,
    CloudRoot, ExactDuplicateSummary, MetadataEvidence,
};
use disksage_lib::semantic_catalog::export_semantic_catalog_candidate_batch;
use disksage_lib::{DatasetColumnProfile, DatasetProfile};

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
        src: "/private/source/report.pdf".into(),
        dst: "/private/destination/report.pdf".into(),
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
        source_root: "/private/source".into(),
        relative_path: "report.pdf".into(),
        source_context: "coverage-context".into(),
        requires_review: false,
        review_reasons: Vec::new(),
        content_title: Some("Coverage title".into()),
        content_authors: vec!["Coverage author".into()],
        content_context: vec!["Coverage context".into()],
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
            id: "coverage-account".into(),
            provider: CloudProvider::Icloud,
            account_scope: CloudAccountScope::Personal,
            label: "coverage-root".into(),
            path: "/private/destination".into(),
            readable: true,
            access_issue: None,
        },
        generated_at_ms: 1_784_900_000_000,
        source_selection_policy: Some(CloudPlanOptions::default()),
        candidate_bytes: candidates.iter().map(|item| item.bytes).sum(),
        potentially_reclaimable_bytes: candidates
            .iter()
            .filter(|item| item.blocked_reason.is_none())
            .map(|item| item.bytes)
            .sum(),
        candidates,
        exact_duplicates: ExactDuplicateSummary::default(),
        capacity: None,
        notices: vec!["coverage-only".into()],
    }
}

fn candidate_error(
    mutate: impl FnOnce(&mut CloudCandidate),
    expected: &str,
) {
    let mut item = candidate();
    mutate(&mut item);
    assert_eq!(
        export_semantic_catalog_candidate_batch(&report(vec![item])).unwrap_err(),
        expected
    );
}

#[test]
fn rejects_invalid_batch_and_fingerprint_boundaries() {
    let mut invalid_generated_at = report(vec![candidate()]);
    invalid_generated_at.generated_at_ms = 0;
    assert_eq!(
        export_semantic_catalog_candidate_batch(&invalid_generated_at).unwrap_err(),
        "semantic-catalog-generated-time-out-of-bounds"
    );

    invalid_generated_at.generated_at_ms = u64::MAX;
    assert_eq!(
        export_semantic_catalog_candidate_batch(&invalid_generated_at).unwrap_err(),
        "semantic-catalog-generated-time-out-of-bounds"
    );

    assert_eq!(
        export_semantic_catalog_candidate_batch(&report(Vec::new())).unwrap_err(),
        "semantic-catalog-candidates-empty"
    );

    candidate_error(
        |item| item.metadata_fingerprint = "a".repeat(63),
        "semantic-catalog-fingerprint-invalid",
    );
    candidate_error(
        |item| item.metadata_fingerprint = "A".repeat(64),
        "semantic-catalog-fingerprint-invalid",
    );
    candidate_error(
        |item| item.review_fingerprint = "z".repeat(64),
        "semantic-catalog-fingerprint-invalid",
    );
}

#[test]
fn rejects_every_time_and_scalar_text_boundary() {
    candidate_error(
        |item| item.modified_ms = 0,
        "semantic-catalog-time-out-of-bounds",
    );
    candidate_error(
        |item| item.production_time_ms = 0,
        "semantic-catalog-time-out-of-bounds",
    );
    candidate_error(
        |item| item.created_ms = u64::MAX,
        "semantic-catalog-time-out-of-bounds",
    );
    candidate_error(
        |item| item.modified_ms = u64::MAX,
        "semantic-catalog-time-out-of-bounds",
    );
    candidate_error(
        |item| item.production_time_ms = u64::MAX,
        "semantic-catalog-time-out-of-bounds",
    );
    candidate_error(
        |item| item.production_time_source.clear(),
        "semantic-catalog-production-time-invalid",
    );
    candidate_error(
        |item| item.production_time_confidence = "certain".into(),
        "semantic-catalog-production-time-invalid",
    );
    candidate_error(
        |item| item.content_title = Some(String::new()),
        "semantic-catalog-optional-text-out-of-bounds",
    );
    candidate_error(
        |item| item.blocked_reason = Some(String::new()),
        "semantic-catalog-optional-text-out-of-bounds",
    );
}

#[test]
fn rejects_collection_and_review_state_boundaries() {
    candidate_error(
        |item| {
            item.requires_review = true;
            item.review_reasons = vec!["review".into(); 129];
        },
        "semantic-catalog-review-reasons-out-of-bounds",
    );
    candidate_error(
        |item| {
            item.requires_review = true;
            item.review_reasons = vec![String::new()];
        },
        "semantic-catalog-review-reasons-out-of-bounds",
    );
    candidate_error(
        |item| item.content_authors = vec!["author".into(); 65],
        "semantic-catalog-content-authors-out-of-bounds",
    );
    candidate_error(
        |item| item.content_authors = vec![String::new()],
        "semantic-catalog-content-authors-out-of-bounds",
    );
    candidate_error(
        |item| item.content_context = vec!["context".into(); 129],
        "semantic-catalog-content-context-out-of-bounds",
    );
    candidate_error(
        |item| item.content_context = vec!["x".repeat(1025)],
        "semantic-catalog-content-context-out-of-bounds",
    );
    candidate_error(
        |item| item.requires_review = true,
        "semantic-catalog-review-state-mismatch",
    );
}

#[test]
fn rejects_metadata_evidence_boundary_variants() {
    candidate_error(
        |item| item.metadata_evidence = vec![item.metadata_evidence[0].clone(); 257],
        "semantic-catalog-metadata-evidence-out-of-bounds",
    );
    candidate_error(
        |item| item.metadata_evidence[0].field.clear(),
        "semantic-catalog-metadata-evidence-out-of-bounds",
    );
    candidate_error(
        |item| item.metadata_evidence[0].value = "x".repeat(2049),
        "semantic-catalog-metadata-evidence-out-of-bounds",
    );
    candidate_error(
        |item| item.metadata_evidence[0].source.clear(),
        "semantic-catalog-metadata-evidence-out-of-bounds",
    );
    candidate_error(
        |item| item.metadata_evidence[0].confidence = "certain".into(),
        "semantic-catalog-metadata-evidence-out-of-bounds",
    );
}

#[test]
fn validates_dataset_profile_conversion_and_bounds() {
    let mut valid = candidate();
    valid.kind = ArchiveKind::Dataset;
    valid.dataset_profile = Some(DatasetProfile {
        format: "csv".into(),
        sampled_rows: 12,
        sampled_worksheets: 1,
        worksheet_names: vec!["Sheet1".into()],
        profile_complete: true,
        sample_truncated: false,
        columns: vec![DatasetColumnProfile {
            name: "amount".into(),
            inferred_type: "number".into(),
            observed_values: 12,
            missing_values: 0,
            sensitive_name: false,
        }],
        quality_warnings: vec!["coverage-warning".into()],
    });
    let batch = export_semantic_catalog_candidate_batch(&report(vec![valid])).unwrap();
    let profile = batch.candidates[0].dataset_profile.as_ref().unwrap();
    assert_eq!(profile.format, "csv");
    assert_eq!(profile.sampled_rows, 12);
    assert_eq!(profile.sampled_worksheets, 1);
    assert_eq!(profile.worksheet_names, vec!["Sheet1"]);
    assert!(profile.profile_complete);
    assert!(!profile.sample_truncated);
    assert_eq!(profile.columns[0].name, "amount");
    assert_eq!(profile.columns[0].inferred_type, "number");
    assert_eq!(profile.columns[0].observed_values, 12);
    assert_eq!(profile.columns[0].missing_values, 0);
    assert!(!profile.columns[0].sensitive_name);
    assert_eq!(profile.quality_warnings, vec!["coverage-warning"]);

    candidate_error(
        |item| item.dataset_profile = Some(DatasetProfile::default()),
        "semantic-catalog-dataset-profile-out-of-bounds",
    );
    candidate_error(
        |item| {
            item.dataset_profile = Some(DatasetProfile {
                format: "csv".into(),
                worksheet_names: vec!["sheet".into(); 129],
                ..DatasetProfile::default()
            });
        },
        "semantic-catalog-dataset-profile-out-of-bounds",
    );
    candidate_error(
        |item| {
            item.dataset_profile = Some(DatasetProfile {
                format: "csv".into(),
                columns: vec![
                    DatasetColumnProfile {
                        name: "column".into(),
                        inferred_type: "text".into(),
                        ..DatasetColumnProfile::default()
                    };
                    513
                ],
                ..DatasetProfile::default()
            });
        },
        "semantic-catalog-dataset-profile-out-of-bounds",
    );
    candidate_error(
        |item| {
            item.dataset_profile = Some(DatasetProfile {
                format: "csv".into(),
                quality_warnings: vec!["warning".into(); 129],
                ..DatasetProfile::default()
            });
        },
        "semantic-catalog-dataset-profile-out-of-bounds",
    );
    candidate_error(
        |item| {
            item.dataset_profile = Some(DatasetProfile {
                format: "csv".into(),
                worksheet_names: vec![String::new()],
                ..DatasetProfile::default()
            });
        },
        "semantic-catalog-dataset-profile-out-of-bounds",
    );
    candidate_error(
        |item| {
            item.dataset_profile = Some(DatasetProfile {
                format: "csv".into(),
                columns: vec![DatasetColumnProfile {
                    name: String::new(),
                    inferred_type: "text".into(),
                    ..DatasetColumnProfile::default()
                }],
                ..DatasetProfile::default()
            });
        },
        "semantic-catalog-dataset-profile-out-of-bounds",
    );
    candidate_error(
        |item| {
            item.dataset_profile = Some(DatasetProfile {
                format: "csv".into(),
                columns: vec![DatasetColumnProfile {
                    name: "column".into(),
                    inferred_type: String::new(),
                    ..DatasetColumnProfile::default()
                }],
                ..DatasetProfile::default()
            });
        },
        "semantic-catalog-dataset-profile-out-of-bounds",
    );
}

#[test]
fn validates_confidence_and_source_class_boundaries() {
    for confidence in ["medium", "low", "unknown"] {
        let mut item = candidate();
        item.production_time_confidence = confidence.into();
        assert!(export_semantic_catalog_candidate_batch(&report(vec![item])).is_ok());
    }

    candidate_error(
        |item| item.production_time_source = "manual:operator".into(),
        "semantic-catalog-production-time-source-unsupported",
    );

    candidate_error(
        |item| {
            item.production_time_source = "filename:path-token".into();
            item.production_time_confidence = "medium".into();
        },
        "semantic-catalog-non-embedded-confidence-invalid",
    );
}
