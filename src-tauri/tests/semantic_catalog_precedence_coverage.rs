//! Coverage for semantic-catalog precedence, uniqueness, and bounded-body contracts.
//!
//! All fixtures are synthetic and exercise only the read-only export boundary.

use disksage_lib::cloud::{
    ArchiveKind, CloudAccountScope, CloudCandidate, CloudPlanOptions, CloudPlanReport, CloudProvider,
    CloudRoot, ExactDuplicateSummary, MetadataEvidence,
};
use disksage_lib::semantic_catalog::{
    export_semantic_catalog_candidate_batch, SEMANTIC_CATALOG_MAX_CANDIDATES,
};

fn evidence(field: &str, value: &str, source: &str, confidence: &str) -> MetadataEvidence {
    MetadataEvidence {
        field: field.into(),
        value: value.into(),
        source: source.into(),
        confidence: confidence.into(),
    }
}

fn candidate(fingerprint_char: char) -> CloudCandidate {
    CloudCandidate {
        metadata_fingerprint: fingerprint_char.to_string().repeat(64),
        review_fingerprint: "f".repeat(64),
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
        metadata_evidence: vec![evidence(
            "production-date",
            "2026-01-02",
            "embedded:ooxml:created",
            "high",
        )],
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
        notices: Vec::new(),
    }
}

#[test]
fn accepts_each_supported_production_time_source_class() {
    let embedded = candidate('a');

    let mut filename = candidate('b');
    filename.production_time_source = "filename:path-token".into();
    filename.production_time_confidence = "low".into();
    filename.metadata_evidence = vec![evidence(
        "filename-date-hint",
        "2026-01-02",
        "filename:path-token",
        "low",
    )];

    let mut created = candidate('c');
    created.created_ms = 0;
    created.production_time_source = "filesystem:created".into();
    created.production_time_confidence = "low".into();
    created.metadata_evidence = vec![evidence(
        "filesystem-created-date",
        "2026-01-02",
        "filesystem:created",
        "low",
    )];

    let mut modified = candidate('d');
    modified.production_time_source = "filesystem:modified-fallback".into();
    modified.production_time_confidence = "low".into();
    modified.duration_ms = Some(1234);
    modified.blocked_reason = Some("coverage-blocker".into());
    modified.requires_review = true;
    modified.review_reasons = vec!["coverage-review".into()];
    modified.metadata_evidence = vec![evidence(
        "filesystem-modified-date",
        "2026-01-02",
        "filesystem:modified",
        "low",
    )];

    let batch = export_semantic_catalog_candidate_batch(&report(vec![
        embedded, filename, created, modified,
    ]))
    .unwrap();
    assert_eq!(batch.candidates.len(), 4);
    assert_eq!(batch.candidates[1].production_time_source, "filename:path-token");
    assert_eq!(batch.candidates[2].production_time_source, "filesystem:created");
    assert_eq!(
        batch.candidates[3].production_time_source,
        "filesystem:modified-fallback"
    );
    assert_eq!(batch.candidates[3].duration_ms, Some(1234));
    assert_eq!(batch.candidates[3].blocked_reason.as_deref(), Some("coverage-blocker"));
    assert!(batch.candidates[3].requires_review);
}

#[test]
fn rejects_selected_evidence_mismatch_and_precedence_violation() {
    let mut mismatch = candidate('a');
    mismatch.metadata_evidence[0].value = "2026-01-01".into();
    assert_eq!(
        export_semantic_catalog_candidate_batch(&report(vec![mismatch])).unwrap_err(),
        "semantic-catalog-selected-production-evidence-mismatch"
    );

    let mut lower_priority = candidate('b');
    lower_priority.production_time_source = "filename:path-token".into();
    lower_priority.production_time_confidence = "low".into();
    lower_priority.metadata_evidence = vec![
        evidence(
            "filename-date-hint",
            "2026-01-02",
            "filename:path-token",
            "low",
        ),
        evidence(
            "production-date",
            "2026-01-02",
            "embedded:ooxml:created",
            "high",
        ),
    ];
    assert_eq!(
        export_semantic_catalog_candidate_batch(&report(vec![lower_priority])).unwrap_err(),
        "semantic-catalog-production-time-precedence-violation"
    );
}

#[test]
fn rejects_duplicate_and_excess_candidate_sets() {
    let duplicate = candidate('a');
    assert_eq!(
        export_semantic_catalog_candidate_batch(&report(vec![duplicate.clone(), duplicate]))
            .unwrap_err(),
        "semantic-catalog-candidate-fingerprint-duplicate"
    );

    let too_many = (0..=SEMANTIC_CATALOG_MAX_CANDIDATES)
        .map(|index| {
            let mut item = candidate('a');
            item.metadata_fingerprint = format!("{index:064x}");
            item
        })
        .collect();
    assert_eq!(
        export_semantic_catalog_candidate_batch(&report(too_many)).unwrap_err(),
        "semantic-catalog-candidate-limit-exceeded"
    );
}

#[test]
fn rejects_serialized_batches_above_the_public_body_limit() {
    let candidates = ['a', 'b', 'c', 'd']
        .into_iter()
        .map(|fingerprint| {
            let mut item = candidate(fingerprint);
            let mut metadata = vec![item.metadata_evidence[0].clone()];
            metadata.extend((0..255).map(|index| {
                evidence(
                    &format!("coverage-field-{index}"),
                    &"x".repeat(2048),
                    "coverage:synthetic",
                    "unknown",
                )
            }));
            item.metadata_evidence = metadata;
            item
        })
        .collect();
    assert_eq!(
        export_semantic_catalog_candidate_batch(&report(candidates)).unwrap_err(),
        "semantic-catalog-body-limit-exceeded"
    );
}
