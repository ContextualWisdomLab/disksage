use crate::{container_orphan_public, container_orphan_reclaim, podman_reclaim};
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::path::PathBuf;

const MAX_DOCKER_CONFIG_BYTES: usize = 64 * 1024;
const MAX_DOCKER_CONTEXT_BYTES: usize = 128;
const DOCKER_CONTEXT_APPROVAL_DOMAIN: &[u8] = b"disksage.container-orphan-docker-context.v1";

fn docker_binary() -> PathBuf {
    [
        "/opt/homebrew/bin/docker",
        "/usr/local/bin/docker",
        "/usr/bin/docker",
    ]
    .into_iter()
    .map(PathBuf::from)
    .find(|path| std::fs::metadata(path).is_ok_and(|metadata| metadata.is_file()))
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
    .find(|path| std::fs::metadata(path).is_ok_and(|metadata| metadata.is_file()))
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
        ContainerRuntimeKind::DockerColimaContext => {
            ContainerRuntimeTarget::new(kind, docker_binary(), Some("colima".to_string()))
        }
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

fn bounded_docker_context(value: &str) -> Option<String> {
    (!value.is_empty()
        && value.len() <= MAX_DOCKER_CONTEXT_BYTES
        && !value.chars().any(char::is_control))
    .then(|| value.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DockerContextEnvironment {
    /// The Docker CLI treats an absent or empty override as no override and consults config.
    AbsentOrEmpty,
    /// A bounded non-empty override takes precedence over Docker's config file.
    Context(String),
    /// A present non-empty override that DiskSage cannot represent safely must not fall back to
    /// config, because Docker itself will not silently replace that override with currentContext.
    Invalid,
}

fn docker_context_environment(value: Option<OsString>) -> DockerContextEnvironment {
    match value {
        None => DockerContextEnvironment::AbsentOrEmpty,
        Some(value) if value.is_empty() => DockerContextEnvironment::AbsentOrEmpty,
        Some(value) => match value.into_string() {
            Ok(context) => bounded_docker_context(&context)
                .map(DockerContextEnvironment::Context)
                .unwrap_or(DockerContextEnvironment::Invalid),
            Err(_) => DockerContextEnvironment::Invalid,
        },
    }
}

fn parse_docker_current_context(bytes: &[u8]) -> Option<String> {
    if bytes.len() > MAX_DOCKER_CONFIG_BYTES {
        return None;
    }
    let document: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    bounded_docker_context(document.get("currentContext")?.as_str()?)
}

fn docker_config_path() -> Option<PathBuf> {
    if let Some(directory) = std::env::var_os("DOCKER_CONFIG").filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(directory).join("config.json"));
    }
    std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .or_else(|| std::env::var_os("USERPROFILE").filter(|value| !value.is_empty()))
        .map(PathBuf::from)
        .map(|home| home.join(".docker").join("config.json"))
}

fn docker_current_context() -> Result<Option<String>, String> {
    match docker_context_environment(std::env::var_os("DOCKER_CONTEXT")) {
        DockerContextEnvironment::Context(context) => return Ok(Some(context)),
        DockerContextEnvironment::Invalid => return Err("docker-context-invalid".to_string()),
        DockerContextEnvironment::AbsentOrEmpty => {}
    }

    let Some(path) = docker_config_path() else {
        return Ok(None);
    };
    let Ok(metadata) = std::fs::symlink_metadata(&path) else {
        return Ok(None);
    };
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() > MAX_DOCKER_CONFIG_BYTES as u64
    {
        return Ok(None);
    }
    let Ok(bytes) = std::fs::read(path) else {
        return Ok(None);
    };
    Ok(parse_docker_current_context(&bytes))
}

fn runtime_kinds_for_default_docker_context(
    current_context: Option<&str>,
) -> Vec<container_orphan_reclaim::ContainerRuntimeKind> {
    use container_orphan_reclaim::ContainerRuntimeKind::{
        DockerColimaContext, DockerNative, PodmanMachine,
    };

    let mut kinds = Vec::with_capacity(3);
    if current_context != Some("colima") {
        kinds.push(DockerNative);
    }
    kinds.push(DockerColimaContext);
    kinds.push(PodmanMachine);
    kinds
}

fn runtime_kinds_for_docker_context(
    current_context: &Result<Option<String>, String>,
) -> Vec<container_orphan_reclaim::ContainerRuntimeKind> {
    use container_orphan_reclaim::ContainerRuntimeKind::{DockerColimaContext, PodmanMachine};
    match current_context {
        Ok(context) => runtime_kinds_for_default_docker_context(context.as_deref()),
        // An inherited, non-empty Docker context that DiskSage cannot represent must never fall
        // through to plain `docker`, because that child would still inherit the opaque override.
        // Explicit `--context colima` remains safe and Podman is independent of Docker context.
        Err(_) => vec![DockerColimaContext, PodmanMachine],
    }
}

fn docker_context_binding(current_context: Option<&str>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(DOCKER_CONTEXT_APPROVAL_DOMAIN);
    hasher.update([0]);
    hasher.update(current_context.unwrap_or("<default>").as_bytes());
    format!("{:x}", hasher.finalize())
}

fn bind_docker_context_approval(base_phrase: &str, current_context: Option<&str>) -> String {
    format!(
        "{base_phrase} docker-context {}",
        docker_context_binding(current_context)
    )
}

fn unbind_docker_context_approval(
    bound_phrase: &str,
    current_context: Option<&str>,
) -> Result<String, String> {
    let suffix = format!(" docker-context {}", docker_context_binding(current_context));
    bound_phrase
        .strip_suffix(&suffix)
        .map(str::to_string)
        .ok_or_else(|| "orphan-prune-docker-context-mismatch".to_string())
}

fn bind_docker_context_plan(
    mut plan: container_orphan_reclaim::ContainerOrphanPlan,
    current_context: Option<&str>,
) -> container_orphan_reclaim::ContainerOrphanPlan {
    for category in &mut plan.categories {
        if let Some(base_phrase) = category.approval_phrase.take() {
            category.approval_phrase = Some(bind_docker_context_approval(
                &base_phrase,
                current_context,
            ));
        }
    }
    plan
}

/// Probes every supported container runtime target read-only and audits all four orphan
/// categories on each healthy target. If Docker's effective default context is already Colima,
/// the explicit Colima target is retained and the duplicate default-context probe is omitted.
/// An unrepresentable inherited Docker context suppresses the ambient Docker target fail-closed.
/// This shipped IPC surface remains present under coverage.
#[tauri::command(async)]
pub fn inspect_container_orphans() -> Vec<container_orphan_reclaim::ContainerOrphanPlan> {
    let docker_context = docker_current_context();
    runtime_kinds_for_docker_context(&docker_context)
        .into_iter()
        .filter_map(|kind| {
            let target = target_for_kind(kind).ok()?;
            let plan = container_orphan_public::sanitize_plan(
                container_orphan_reclaim::probe_container_orphans(&target),
            );
            if kind == container_orphan_reclaim::ContainerRuntimeKind::DockerNative {
                let context = docker_context.as_ref().ok()?.as_deref();
                Some(bind_docker_context_plan(plan, context))
            } else {
                Some(plan)
            }
        })
        .collect()
}

/// Re-audits one runtime/category immediately before exact identity-bound deletion. Runtime scope
/// is validated against the same fixed target used by inspection, and Docker-native approvals are
/// additionally bound to the fresh effective Docker context so context drift cannot reuse an
/// approval even when two daemons expose the same candidate IDs. This shipped IPC surface remains
/// present under coverage.
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
    let engine_confirmation = if kind == container_orphan_reclaim::ContainerRuntimeKind::DockerNative {
        let current_context = docker_current_context()
            .map_err(|_| "orphan-prune-docker-context-invalid".to_string())?;
        unbind_docker_context_approval(&confirmation_phrase, current_context.as_deref())?
    } else {
        confirmation_phrase
    };
    container_orphan_reclaim::execute_container_orphan_prune(
        &target,
        category,
        &engine_confirmation,
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

    #[test]
    fn colima_default_context_does_not_create_duplicate_runtime_targets() {
        use container_orphan_reclaim::ContainerRuntimeKind::{
            DockerColimaContext, DockerNative, PodmanMachine,
        };

        assert_eq!(
            runtime_kinds_for_default_docker_context(Some("colima")),
            vec![DockerColimaContext, PodmanMachine]
        );
        assert_eq!(
            runtime_kinds_for_default_docker_context(Some("desktop-linux")),
            vec![DockerNative, DockerColimaContext, PodmanMachine]
        );
        assert_eq!(
            runtime_kinds_for_default_docker_context(None),
            vec![DockerNative, DockerColimaContext, PodmanMachine]
        );
    }

    #[test]
    fn invalid_docker_context_must_not_fall_through_to_native_target() {
        use container_orphan_reclaim::ContainerRuntimeKind::{DockerColimaContext, DockerNative, PodmanMachine};

        let invalid = Err("docker-context-invalid".to_string());
        let kinds = runtime_kinds_for_docker_context(&invalid);

        assert!(!kinds.contains(&DockerNative));
        assert_eq!(kinds, vec![DockerColimaContext, PodmanMachine]);
    }

    #[test]
    fn docker_native_approval_is_context_bound_without_disclosing_context_name() {
        let base = "DiskSage image orphan prune 승인 abcdef";
        let desktop = bind_docker_context_approval(base, Some("desktop-linux"));
        let other = bind_docker_context_approval(base, Some("customer-context"));
        let default = bind_docker_context_approval(base, None);

        assert_ne!(desktop, other);
        assert_ne!(desktop, default);
        assert!(!desktop.contains("desktop-linux"));
        assert_eq!(
            unbind_docker_context_approval(&desktop, Some("desktop-linux")).unwrap(),
            base
        );
        assert_eq!(
            unbind_docker_context_approval(&desktop, Some("customer-context")).unwrap_err(),
            "orphan-prune-docker-context-mismatch"
        );
        assert_eq!(
            unbind_docker_context_approval(&desktop, None).unwrap_err(),
            "orphan-prune-docker-context-mismatch"
        );
    }

    #[test]
    fn docker_context_environment_precedence_is_fail_closed_and_matches_empty_override_fallback() {
        assert_eq!(
            docker_context_environment(None),
            DockerContextEnvironment::AbsentOrEmpty
        );
        assert_eq!(
            docker_context_environment(Some(OsString::new())),
            DockerContextEnvironment::AbsentOrEmpty
        );
        assert_eq!(
            docker_context_environment(Some(OsString::from("colima"))),
            DockerContextEnvironment::Context("colima".to_string())
        );
        assert_eq!(
            docker_context_environment(Some(OsString::from("bad\ncontext"))),
            DockerContextEnvironment::Invalid
        );
        assert_eq!(
            docker_context_environment(Some(OsString::from("x".repeat(MAX_DOCKER_CONTEXT_BYTES + 1)))),
            DockerContextEnvironment::Invalid
        );

        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            assert_eq!(
                docker_context_environment(Some(OsString::from_vec(vec![0xff]))),
                DockerContextEnvironment::Invalid
            );
        }
    }

    #[test]
    fn docker_current_context_parser_is_bounded_and_exact() {
        assert_eq!(
            parse_docker_current_context(br#"{"currentContext":"colima"}"#).as_deref(),
            Some("colima")
        );
        assert_eq!(
            parse_docker_current_context(br#"{"currentContext":"desktop-linux"}"#).as_deref(),
            Some("desktop-linux")
        );
        assert_eq!(parse_docker_current_context(b"not-json"), None);
        assert_eq!(
            parse_docker_current_context(br#"{"currentContext":12}"#),
            None
        );
        assert_eq!(
            parse_docker_current_context(&vec![b'x'; MAX_DOCKER_CONFIG_BYTES + 1]),
            None
        );
    }
}
