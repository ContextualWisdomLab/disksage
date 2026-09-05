use disksage_lib::transparent_compression::{execute, TransparentCompressionPlan};

#[test]
fn library_execution_fails_closed_before_root_observation() {
    let approved_plan = TransparentCompressionPlan {
        schema_kind: "disksage.transparent-compression-plan",
        schema_version: 1,
        ontology_class: "https://disksage.app/ontology#StructuredLogArtifact",
        root: "/definitely-not-an-authorized-disksage-root".into(),
        minimum_age_days: 30,
        max_files: 1,
        compression_concurrency: 4,
        candidate_count: 0,
        logical_bytes: 0,
        allocated_bytes_before: 0,
        candidates: Vec::new(),
        plan_fingerprint: "reviewed-plan".into(),
        exact_approval_phrase: Some("reviewed-phrase".into()),
        filesystem_mutation_executed: false,
    };

    let result = execute(
        &approved_plan,
        "reviewed-plan",
        "reviewed-phrase",
        "reviewed by operator",
        1,
    );

    assert_eq!(
        result,
        Err("transparent-compression-root-authorization-unavailable".to_string())
    );
}
