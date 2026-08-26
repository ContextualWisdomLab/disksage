use crate::container_orphan_reclaim::{
    ContainerOrphanPlan, ContainerOrphanPruneExecution,
};

const FALLBACK_ISSUE: &str = "container-runtime-evidence-unavailable";

fn stable_issue(raw: &str) -> String {
    let token = raw.split(':').next().unwrap_or_default();
    if !token.is_empty()
        && token.len() <= 128
        && token
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        token.to_string()
    } else {
        FALLBACK_ISSUE.to_string()
    }
}

/// Removes runtime stderr, paths, socket details, local machine names, and record fragments from
/// the machine-readable public plan while retaining stable fail-closed issue categories.
pub fn sanitize_plan(mut plan: ContainerOrphanPlan) -> ContainerOrphanPlan {
    plan.runtime.detail_issue = plan.runtime.detail_issue.as_deref().map(stable_issue);
    for category in &mut plan.categories {
        category.issue = category.issue.as_deref().map(stable_issue);
    }
    let mut issues = plan
        .categories
        .iter()
        .filter_map(|category| {
            category
                .issue
                .as_ref()
                .map(|issue| format!("{}:{issue}", category.category.as_str()))
        })
        .collect::<Vec<_>>();
    if let Some(issue) = plan.runtime.detail_issue.clone() {
        issues.push(issue);
    }
    plan.issues = issues;
    plan
}

/// Keeps the mutation receipt useful for authorization/accounting without returning arbitrary
/// runtime stdout/stderr (which can contain local identifiers, paths, or engine diagnostics).
pub fn sanitize_execution(
    mut execution: ContainerOrphanPruneExecution,
) -> ContainerOrphanPruneExecution {
    execution.stdout.clear();
    execution.stderr.clear();
    execution
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container_orphan_reclaim::{
        ContainerRuntimeKind, OrphanCategory, RuntimeHealthEvidence,
    };

    #[test]
    fn public_plan_keeps_only_stable_runtime_issue_codes() {
        let secret = "/Users/customer/private.sock bearer-token";
        let plan = ContainerOrphanPlan {
            schema_kind: "disksage.container-orphan-plan",
            schema_version: 1,
            platform: "test",
            evidence_complete: false,
            elapsed_ms: 1,
            runtime: RuntimeHealthEvidence {
                kind: ContainerRuntimeKind::DockerNative,
                display_name: "docker (docker-native)".into(),
                healthy: false,
                detail_issue: Some(format!("runtime-info-failed:{secret}")),
            },
            categories: Vec::new(),
            issues: vec![format!("runtime-info-failed:{secret}")],
        };
        let sanitized = sanitize_plan(plan);
        assert_eq!(sanitized.runtime.detail_issue.as_deref(), Some("runtime-info-failed"));
        assert_eq!(sanitized.issues, vec!["runtime-info-failed"]);
        let json = serde_json::to_string(&sanitized).unwrap();
        assert!(!json.contains(secret));
        assert!(!json.contains("bearer-token"));
    }

    #[test]
    fn public_execution_never_returns_runtime_output() {
        let execution = ContainerOrphanPruneExecution {
            schema_version: 1,
            runtime_display_name: "docker (docker-native)".into(),
            category: OrphanCategory::Container,
            candidate_set_sha256: "a".repeat(64),
            command: vec!["docker".into(), "container".into(), "rm".into(), "<candidate-set>".into()],
            status_code: 1,
            stdout: "container-secret-id".into(),
            stderr: "/Users/customer/private.sock".into(),
            output_truncated: false,
            executed: false,
            executed_at_ms: 1,
            before_available_bytes: None,
            after_available_bytes: None,
            observed_available_gain_bytes: None,
            rationale: "Reviewed exact evidence.".into(),
        };
        let sanitized = sanitize_execution(execution);
        assert!(sanitized.stdout.is_empty());
        assert!(sanitized.stderr.is_empty());
    }

    #[test]
    fn malformed_issue_tokens_fall_back_without_reflection() {
        assert_eq!(stable_issue("Bad Token:/secret"), FALLBACK_ISSUE);
        assert_eq!(stable_issue(""), FALLBACK_ISSUE);
        assert_eq!(stable_issue("orphan-list-container-failed:/secret"), "orphan-list-container-failed");
    }
}
