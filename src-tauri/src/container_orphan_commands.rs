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
        std::fs::symlink_metadata(path)
            .is_ok_and(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
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
        std::fs::symlink_metadata(path)
            .is_ok_and(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
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

/// Probes every supported container runtime target read-only and audits all four orphan
/// categories on each healthy target. This shipped IPC surface remains present under coverage.
#[tauri::command(async)]
pub fn inspect_container_orphans() -> Vec<container_orphan_reclaim::ContainerOrphanPlan> {
    use container_orphan_reclaim::{ContainerRuntimeKind, ContainerRuntimeTarget};
    let targets = [
        ContainerRuntimeTarget::new(
            ContainerRuntimeKind::DockerNative,
            docker_binary(),
            None,
        ),
        ContainerRuntimeTarget::new(
            ContainerRuntimeKind::DockerColimaContext,
            docker_binary(),
            Some("colima".to_string()),
        ),
        ContainerRuntimeTarget::new(
            ContainerRuntimeKind::PodmanMachine,
            podman_binary(),
            Some(podman_reclaim::DEFAULT_PODMAN_MACHINE.to_string()),
        ),
    ];
    targets
        .iter()
        .filter_map(|target| target.as_ref().ok())
        .map(container_orphan_reclaim::probe_container_orphans)
        .map(container_orphan_public::sanitize_plan)
        .collect()
}

/// Re-audits one runtime/category immediately before exact identity-bound deletion. This shipped
/// IPC surface remains present under coverage so coverage cannot silently compile out authority.
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
    let binary_path = match kind {
        container_orphan_reclaim::ContainerRuntimeKind::PodmanMachine => podman_binary(),
        _ => docker_binary(),
    };
    let target = container_orphan_reclaim::ContainerRuntimeTarget::new(
        kind,
        binary_path,
        scope_name,
    )?;
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
}
