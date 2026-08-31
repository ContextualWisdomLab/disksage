use crate::container_orphan_reclaim::{
    ContainerOrphanPlan, ContainerOrphanPruneExecution, OrphanCategory,
};

const FALLBACK_ISSUE: &str = "container-runtime-evidence-unavailable";
const INDETERMINATE_PRUNE_OUTCOME: &str = "container-orphan-prune-outcome-indeterminate";

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

fn public_command_shape(category: OrphanCategory, has_candidates: bool) -> Vec<String> {
    if category == OrphanCategory::BuildCache {
        return Vec::new();
    }
    let mut command = vec![category.as_str().to_string(), "rm".to_string()];
    if has_candidates {
        command.push("<candidate-set>".to_string());
    }
    command
}

/// Removes runtime stderr, paths, socket details, local machine names, and record fragments from
/// the machine-readable public plan while retaining stable fail-closed issue categories.
pub fn sanitize_plan(mut plan: ContainerOrphanPlan) -> ContainerOrphanPlan {
    plan.runtime.detail_issue = plan.runtime.detail_issue.as_deref().map(stable_issue);
    plan.runtime.display_name = plan.runtime.kind.as_str().to_string();
    for category in &mut plan.categories {
        category.issue = category.issue.as_deref().map(stable_issue);
        if category.prune_command.is_some() {
            let has_candidates = category
                .evidence
                .as_ref()
                .is_some_and(|evidence| evidence.candidate_records > 0);
            let public_command = public_command_shape(category.category, has_candidates);
            if public_command.is_empty() {
                category.prune_command = None;
                category.approval_phrase = None;
            } else {
                category.prune_command = Some(public_command);
            }
        }
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
/// runtime stdout/stderr, local executable paths, runtime scope names, or capacity observations
/// whose filesystem has not been proven to contain the runtime store. A non-zero multi-target
/// remove command cannot prove that no target was removed, so its public receipt keeps a stable
/// indeterminate-outcome code instead of presenting the sanitized runtime failure as a clean
/// no-mutation result. Callers must refresh runtime evidence before making a new decision.
pub fn sanitize_execution(
    mut execution: ContainerOrphanPruneExecution,
) -> ContainerOrphanPruneExecution {
    execution.runtime_display_name = "container-runtime".to_string();
    execution.command = public_command_shape(execution.category, true);
    execution.stdout.clear();
    execution.stderr.clear();
    // The reclaim engine currently has no authoritative runtime-store filesystem path for native
    // Docker, Colima, or Podman. Its internal current-working-directory snapshot therefore cannot
    // support a customer-facing capacity attribution. Fail closed until store-bound evidence
    // exists rather than serializing an unrelated host-volume delta as reclaim evidence.
    execution.before_available_bytes = None;
    execution.after_available_bytes = None;
    execution.observed_available_gain_bytes = None;
    if execution.status_code != 0 {
        execution.stderr = INDETERMINATE_PRUNE_OUTCOME.to_string();
    }
    execution
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container_orphan_reclaim::{
        probe_container_orphans, probe_container_orphans_with_receipt_dir, ContainerRuntimeKind,
        ContainerRuntimeTarget, RuntimeHealthEvidence,
    };
    use std::path::PathBuf;

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
            receipt_directory_sha256: None,
        };
        let sanitized = sanitize_plan(plan);
        assert_eq!(
            sanitized.runtime.detail_issue.as_deref(),
            Some("runtime-info-failed")
        );
        assert_eq!(sanitized.issues, vec!["runtime-info-failed"]);
        let json = serde_json::to_string(&sanitized).unwrap();
        assert!(!json.contains(secret));
        assert!(!json.contains("bearer-token"));
    }

    #[test]
    fn public_plan_never_returns_runtime_scope_name() {
        let secret_scope = "customer-colima-secret";
        let target = ContainerRuntimeTarget::new(
            ContainerRuntimeKind::DockerColimaContext,
            PathBuf::from("__disksage_missing_runtime__"),
            Some(secret_scope.into()),
        )
        .unwrap();

        let sanitized = sanitize_plan(probe_container_orphans(&target));
        let json = serde_json::to_string(&sanitized).unwrap();

        assert_eq!(sanitized.runtime.display_name, "docker-colima-context");
        assert!(!json.contains(secret_scope));
    }

    #[test]
    fn public_execution_never_returns_runtime_output_local_identity_or_unbound_capacity() {
        let secret_binary = "/Users/customer/private/bin/docker";
        let secret_scope = "customer-colima-secret";
        let execution = ContainerOrphanPruneExecution {
            schema_version: 1,
            runtime_display_name: format!("docker {secret_scope}"),
            category: OrphanCategory::Container,
            candidate_set_sha256: "a".repeat(64),
            command: vec![
                secret_binary.into(),
                "--context".into(),
                secret_scope.into(),
                "container".into(),
                "rm".into(),
                "<candidate-set>".into(),
            ],
            status_code: 1,
            stdout: "container-secret-id".into(),
            stderr: "/Users/customer/private.sock".into(),
            output_truncated: false,
            executed: false,
            executed_at_ms: 1,
            before_available_bytes: Some(1_000),
            after_available_bytes: Some(1_200),
            observed_available_gain_bytes: Some(200),
            rationale: "Reviewed exact evidence.".into(),
            receipt_sha256: None,
            receipt_recorded: false,
            receipt_record_error: Some("orphan-receipt-create-failed".into()),
        };
        let sanitized = sanitize_execution(execution);
        let json = serde_json::to_string(&sanitized).unwrap();
        assert_eq!(sanitized.runtime_display_name, "container-runtime");
        assert_eq!(
            sanitized.command,
            vec!["container", "rm", "<candidate-set>"]
        );
        assert!(sanitized.stdout.is_empty());
        assert_eq!(sanitized.stderr, INDETERMINATE_PRUNE_OUTCOME);
        assert_eq!(sanitized.before_available_bytes, None);
        assert_eq!(sanitized.after_available_bytes, None);
        assert_eq!(sanitized.observed_available_gain_bytes, None);
        assert!(!json.contains(secret_binary));
        assert!(!json.contains(secret_scope));
    }

    #[test]
    fn build_cache_public_command_has_no_mutation_authority() {
        assert!(public_command_shape(OrphanCategory::BuildCache, true).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn approval_is_removed_even_when_internal_mutation_command_is_missing() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let receipt_dir = temp.path().join("receipts");
        std::fs::create_dir(&receipt_dir).unwrap();
        std::fs::set_permissions(&receipt_dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        let docker = temp.path().join("docker");
        std::fs::write(
            &docker,
            "#!/bin/sh\ncase \"$*\" in\n  *\" info\") exit 0 ;;\n  *\"buildx du --format json\"*) printf '%s\\n' '{\"ID\":\"cache123\",\"Reclaimable\":true}' ;;\n  *) exit 0 ;;\nesac\n",
        )
        .unwrap();
        std::fs::set_permissions(&docker, std::fs::Permissions::from_mode(0o700)).unwrap();
        let target = ContainerRuntimeTarget::new(
            ContainerRuntimeKind::DockerNative,
            docker,
            None,
        )
        .unwrap();
        let mut plan = probe_container_orphans_with_receipt_dir(&target, &receipt_dir);
        let build_cache = plan
            .categories
            .iter_mut()
            .find(|category| category.category == OrphanCategory::BuildCache)
            .unwrap();
        assert!(build_cache.approval_phrase.is_some());
        build_cache.prune_command = None;

        let sanitized = sanitize_plan(plan);
        let build_cache = sanitized
            .categories
            .iter()
            .find(|category| category.category == OrphanCategory::BuildCache)
            .unwrap();
        assert!(build_cache.approval_phrase.is_none());
        assert!(build_cache.prune_command.is_none());
    }

    #[test]
    fn malformed_issue_tokens_fall_back_without_reflection() {
        assert_eq!(stable_issue("Bad Token:/secret"), FALLBACK_ISSUE);
        assert_eq!(stable_issue(""), FALLBACK_ISSUE);
        assert_eq!(
            stable_issue("orphan-list-container-failed:/secret"),
            "orphan-list-container-failed"
        );
    }
}
