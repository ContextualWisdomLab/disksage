use crate::{container_orphan_public, container_orphan_reclaim, podman_reclaim};
use std::path::PathBuf;

fn docker_binary() -> PathBuf {
    [
        "/opt/homebrew/bin/docker",
        "/usr/local/bin/docker",
        "/usr/bin/docker",
    ]
    .into_iter()
    .map(PathBuf::from)
    .find(|path| {
        std::fs::metadata(path).is_ok_and(|metadata| metadata.is_file())
    })
    .unwrap_or_else(|| PathBuf::from("docker"))
}

fn podman_binary() -> PathBuf {
    [
        "/opt/homebrew/bin/podman",
        "/usr/local/bin/podman",
        "/usr/bin/podman",
    ]
    .into_iter()
    .map(PathBuf::from)
    .find(|path| {
        std::fs::metadata(path).is_ok_and(|metadata| metadata.is_file())
    })
    .unwrap_or_else(|| PathBuf::from("podman"))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn valid_rationale(value: &str) -> bool {
    let trimmed = value.trim();
    value == trimmed
        && !trimmed.is_empty()
        && trimmed.chars().count() <= 1_000
        && !trimmed.chars().any(char::is_control)
}

fn parse_runtime_kind(
    value: &str,
) -> Result<container_orphan_reclaim::ContainerRuntimeKind, String> {
    match value {
        "docker-native" => Ok(container_orphan_reclaim::ContainerRuntimeKind::DockerNative),
        "docker-colima-context" => {
            Ok(container_orphan_reclaim::ContainerRuntimeKind::DockerColimaContext)
        }
        "podman-machine" => Ok(container_orphan_reclaim::ContainerRuntimeKind::PodmanMachine),
        _ => Err("unknown-runtime-kind".into()),
    }
}

fn parse_category(value: &str) -> Result<container_orphan_reclaim::OrphanCategory, String> {
    match value {
        "container" => Ok(container_orphan_reclaim::OrphanCategory::Container),
        "image" => Ok(container_orphan_reclaim::OrphanCategory::Image),
        "volume" => Ok(container_orphan_reclaim::OrphanCategory::Volume),
        "network" => Ok(container_orphan_reclaim::OrphanCategory::Network),
        _ => Err("unknown-orphan-category".into()),
    }
}

fn target_for_kind(
    kind: container_orphan_reclaim::ContainerRuntimeKind,
) -> Result<container_orphan_reclaim::ContainerRuntimeTarget, String> {
    use container_orphan_reclaim::{ContainerRuntimeKind, ContainerRuntimeTarget};
    match kind {
        ContainerRuntimeKind::DockerNative => {
            ContainerRuntimeTarget::new(kind, docker_binary(), None)
        }
        ContainerRuntimeKind::DockerColimaContext => ContainerRuntimeTarget::new(
            kind,
            docker_binary(),
            Some("colima".to_string()),
        ),
        ContainerRuntimeKind::PodmanMachine => ContainerRuntimeTarget::new(
            kind,
            podman_binary(),
            Some(podman_reclaim::DEFAULT_PODMAN_MACHINE.to_string()),
        ),
    }
}

fn validate_requested_scope(
    target: &container_orphan_reclaim::ContainerRuntimeTarget,
    requested_scope: &Option<String>,
) -> Result<(), String> {
    if &target.scope_name != requested_scope {
        return Err("orphan-prune-runtime-scope-mismatch".into());
    }
    Ok(())
}

/// Probes every supported container runtime target read-only and audits all four orphan
/// categories on each healthy target. This shipped IPC surface remains present under coverage.
#[tauri::command(async)]
pub fn inspect_container_orphans() -> Vec<container_orphan_reclaim::ContainerOrphanPlan> {
    use container_orphan_reclaim::ContainerRuntimeKind;
    [
        ContainerRuntimeKind::DockerNative,
        ContainerRuntimeKind::DockerColimaContext,
        ContainerRuntimeKind::PodmanMachine,
    ]
    .into_iter()
    .filter_map(|kind| target_for_kind(kind).ok())
    .map(|target| container_orphan_reclaim::probe_container_orphans(&target))
    .map(container_orphan_public::sanitize_plan)
    .collect()
}

/// Re-audits one runtime/category immediately before exact identity-bound deletion. Runtime scope
/// is validated against the same fixed target used by inspection, so client display metadata can
/// never expand mutation authority. This shipped IPC surface remains present under coverage.
#[tauri::command(async)]
pub fn execute_container_orphan_prune(
    runtime_kind: String,
    scope_name: Option<String>,
    category: String,
    confirmation_phrase: String,
    rationale: String,
) -> Result<container_orphan_reclaim::ContainerOrphanPruneExecution, String> {
    if !valid_rationale(&rationale) {
        return Err("orphan-prune-rationale-invalid".into());
    }
    let kind = parse_runtime_kind(&runtime_kind)?;
    let category = parse_category(&category)?;
    let target = target_for_kind(kind)?;
    validate_requested_scope(&target, &scope_name)?;
    container_orphan_reclaim::execute_container_orphan_prune(
        &target,
        category,
        &confirmation_phrase,
        &rationale,
        now_ms(),
    )
    .map(container_orphan_public::sanitize_execution)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_inputs_fail_closed_without_reflecting_untrusted_tokens() {
        assert_eq!(parse_runtime_kind("secret-runtime").unwrap_err(), "unknown-runtime-kind");
        assert_eq!(parse_category("secret-category").unwrap_err(), "unknown-orphan-category");
        assert!(!valid_rationale(""));
        assert!(!valid_rationale(" leading"));
        assert!(!valid_rationale("bad\nline"));
        assert!(valid_rationale("Reviewed the fresh candidate-bound plan."));
    }

    #[test]
    fn execution_targets_use_fixed_server_side_scopes() {
        use container_orphan_reclaim::ContainerRuntimeKind;

        let docker = target_for_kind(ContainerRuntimeKind::DockerNative).unwrap();
        let colima = target_for_kind(ContainerRuntimeKind::DockerColimaContext).unwrap();
        let podman = target_for_kind(ContainerRuntimeKind::PodmanMachine).unwrap();

        assert_eq!(docker.scope_name, None);
        assert_eq!(colima.scope_name.as_deref(), Some("colima"));
        assert_eq!(
            podman.scope_name.as_deref(),
            Some(podman_reclaim::DEFAULT_PODMAN_MACHINE),
        );
        assert!(validate_requested_scope(&docker, &None).is_ok());
        assert!(validate_requested_scope(&colima, &Some("colima".into())).is_ok());
        assert!(validate_requested_scope(
            &podman,
            &Some(podman_reclaim::DEFAULT_PODMAN_MACHINE.into())
        )
        .is_ok());
        assert_eq!(
            validate_requested_scope(&podman, &Some("attacker-controlled-machine".into()))
                .unwrap_err(),
            "orphan-prune-runtime-scope-mismatch"
        );
    }
}
