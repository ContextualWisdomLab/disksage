//! Public transparent-compression contract.
//!
//! Planning is read-only. Mutation is fail-closed until DiskSage can bind an approved root to a
//! private authorization and an execution receipt that cannot be replayed for arbitrary paths.

pub use crate::transparent_compression_impl::{
    plan, TransparentCompressionCandidate, TransparentCompressionPlan, TransparentCompressionResult,
};

const MUTATION_UNAVAILABLE: &str = "transparent-compression-root-authorization-unavailable";

pub fn execute(
    _approved_plan: &TransparentCompressionPlan,
    _expected_fingerprint: &str,
    _confirmation_phrase: &str,
    rationale: &str,
    _now_ms: u64,
) -> Result<TransparentCompressionResult, String> {
    if rationale.trim() != rationale
        || rationale.is_empty()
        || rationale.len() > 1_000
        || rationale.chars().any(char::is_control)
    {
        return Err("transparent-compression-rationale-invalid".into());
    }
    Err(MUTATION_UNAVAILABLE.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_execution_is_fail_closed() {
        let plan = TransparentCompressionPlan {
            schema_kind: "disksage.transparent-compression-plan",
            schema_version: 1,
            ontology_class: "https://disksage.app/ontology#StructuredLogArtifact",
            root: "/not-observed".into(),
            minimum_age_days: 30,
            max_files: 1,
            compression_concurrency: 4,
            candidate_count: 0,
            logical_bytes: 0,
            allocated_bytes_before: 0,
            candidates: Vec::new(),
            plan_fingerprint: "fingerprint".into(),
            exact_approval_phrase: Some("phrase".into()),
            filesystem_mutation_executed: false,
        };
        assert_eq!(
            execute(&plan, "fingerprint", "phrase", "reviewed", 1),
            Err(MUTATION_UNAVAILABLE.into())
        );
    }
}
