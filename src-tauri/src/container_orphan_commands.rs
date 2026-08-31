use crate::{container_orphan_public, container_orphan_reclaim, podman_reclaim};
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use tauri::Manager;

fn ensure_container_receipt_dir(dir: &Path) -> Result<(), String> {
    if !dir.exists() {
        std::fs::create_dir_all(dir)
            .map_err(|_| "orphan-receipt-directory-create-failed".to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
                .map_err(|_| "orphan-receipt-directory-permission-failed".to_string())?;
        }
    }
    Ok(())
}

fn container_receipt_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|_| "orphan-receipt-directory-unavailable".to_string())?
        .join("container-orphan-receipts");
    ensure_container_receipt_dir(&dir)?;
    Ok(dir)
}

const MAX_DOCKER_CONFIG_BYTES: usize = 64 * 1024;
const MAX_DOCKER_CONTEXT_BYTES: usize = 128;
const MAX_DOCKER_HOST_BYTES: usize = 2 * 1024;
const DOCKER_AUTHORITY_APPROVAL_DOMAIN: &[u8] = b"disksage.container-orphan-docker-authority.v1";
const IMMUTABLE_CONTEXT_REQUIRED: &str = "docker-context-authority-not-immutable";

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
        "build_cache" => Ok(container_orphan_reclaim::OrphanCategory::BuildCache),
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

/// Only an explicit Docker host is an immutable-enough mutation authority at this layer. A named
/// Docker context can be replaced between `context inspect` and a later `--context` mutation; the
/// CLI does not offer a conditional delete tied to the inspected context definition. Contexts are
/// therefore read-only until DiskSage can snapshot the full context/TLS material and execute every
/// command against that private snapshot.
fn pin_docker_authority(
    _binary_path: &std::path::Path,
    authority: &DockerAmbientAuthority,
) -> Result<DockerAmbientAuthority, String> {
    match authority {
        DockerAmbientAuthority::Host(host) => Ok(DockerAmbientAuthority::Host(host.clone())),
        DockerAmbientAuthority::Context(_) | DockerAmbientAuthority::Default => {
            Err(IMMUTABLE_CONTEXT_REQUIRED.into())
        }
    }
}

fn pinned_docker_target(
    authority: &DockerAmbientAuthority,
) -> Result<container_orphan_reclaim::ContainerRuntimeTarget, String> {
    match authority {
        DockerAmbientAuthority::Host(host) => {
            container_orphan_reclaim::ContainerRuntimeTarget::docker_native_host(
                docker_binary(),
                host.clone(),
            )
        }
        DockerAmbientAuthority::Context(_) | DockerAmbientAuthority::Default => {
            Err("docker-authority-not-pinned".into())
        }
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

fn bounded_docker_host(value: &str) -> Option<String> {
    (!value.is_empty()
        && value.len() <= MAX_DOCKER_HOST_BYTES
        && !value.chars().any(char::is_control))
    .then(|| value.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DockerContextEnvironment {
    /// The Docker CLI treats an absent or empty override as no override and consults config.
    AbsentOrEmpty,
    /// A bounded non-empty override takes precedence over Docker's config file and DOCKER_HOST.
    Context(String),
    /// A present non-empty override that DiskSage cannot represent safely must fail closed.
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DockerHostEnvironment {
    AbsentOrEmpty,
    Host(String),
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DockerAmbientAuthority {
    Default,
    Context(String),
    Host(String),
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

fn docker_host_environment(value: Option<OsString>) -> DockerHostEnvironment {
    match value {
        None => DockerHostEnvironment::AbsentOrEmpty,
        Some(value) if value.is_empty() => DockerHostEnvironment::AbsentOrEmpty,
        Some(value) => match value.into_string() {
            Ok(host) => bounded_docker_host(&host)
                .map(DockerHostEnvironment::Host)
                .unwrap_or(DockerHostEnvironment::Invalid),
            Err(_) => DockerHostEnvironment::Invalid,
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

fn docker_config_current_context() -> Option<String> {
    let path = docker_config_path()?;
    let metadata = std::fs::symlink_metadata(&path).ok()?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() > MAX_DOCKER_CONFIG_BYTES as u64
    {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    parse_docker_current_context(&bytes)
}

fn resolve_docker_ambient_authority(
    context_environment: DockerContextEnvironment,
    host_environment: DockerHostEnvironment,
    configured_context: Option<String>,
) -> Result<DockerAmbientAuthority, String> {
    match context_environment {
        // Docker documents DOCKER_CONTEXT as overriding DOCKER_HOST and the configured default.
        DockerContextEnvironment::Context(context) => Ok(DockerAmbientAuthority::Context(context)),
        DockerContextEnvironment::Invalid => Err("docker-context-invalid".to_string()),
        DockerContextEnvironment::AbsentOrEmpty => match host_environment {
            // With no explicit context, DOCKER_HOST overrides config.json.currentContext.
            DockerHostEnvironment::Host(host) => Ok(DockerAmbientAuthority::Host(host)),
            DockerHostEnvironment::Invalid => Err("docker-host-invalid".to_string()),
            DockerHostEnvironment::AbsentOrEmpty => Ok(configured_context
                .map(DockerAmbientAuthority::Context)
                .unwrap_or(DockerAmbientAuthority::Default)),
        },
    }
}

fn docker_ambient_authority() -> Result<DockerAmbientAuthority, String> {
    let context_environment = docker_context_environment(std::env::var_os("DOCKER_CONTEXT"));
    let host_environment = docker_host_environment(std::env::var_os("DOCKER_HOST"));
    let configured_context = match context_environment {
        DockerContextEnvironment::AbsentOrEmpty => match host_environment {
            DockerHostEnvironment::AbsentOrEmpty => docker_config_current_context(),
            DockerHostEnvironment::Host(_) | DockerHostEnvironment::Invalid => None,
        },
        DockerContextEnvironment::Context(_) | DockerContextEnvironment::Invalid => None,
    };
    resolve_docker_ambient_authority(context_environment, host_environment, configured_context)
}

fn runtime_kinds_for_docker_authority(
    authority: &Result<DockerAmbientAuthority, String>,
) -> Vec<container_orphan_reclaim::ContainerRuntimeKind> {
    use container_orphan_reclaim::ContainerRuntimeKind::{
        DockerColimaContext, DockerNative, PodmanMachine,
    };

    let mut kinds = Vec::with_capacity(3);
    if matches!(authority, Ok(DockerAmbientAuthority::Host(_))) {
        kinds.push(DockerNative);
    }
    // Named Docker contexts remain visible through the explicit Colima read-only target, but do
    // not acquire destructive authority. Podman remains independent of Docker ambient authority.
    kinds.push(DockerColimaContext);
    kinds.push(PodmanMachine);
    kinds
}

fn docker_authority_binding(authority: &DockerAmbientAuthority) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut hasher = Sha256::new();
    hasher.update(DOCKER_AUTHORITY_APPROVAL_DOMAIN);
    hasher.update([0]);
    match authority {
        DockerAmbientAuthority::Default => hasher.update(b"default"),
        DockerAmbientAuthority::Context(context) => {
            hasher.update(b"context");
            hasher.update([0]);
            hasher.update(context.as_bytes());
        }
        DockerAmbientAuthority::Host(host) => {
            hasher.update(b"host");
            hasher.update([0]);
            hasher.update(host.as_bytes());
        }
    }
    let digest = hasher.finalize();
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

fn bind_docker_authority_approval(base_phrase: &str, authority: &DockerAmbientAuthority) -> String {
    format!(
        "{base_phrase} docker-authority {}",
        docker_authority_binding(authority)
    )
}

fn unbind_docker_authority_approval(
    bound_phrase: &str,
    authority: &DockerAmbientAuthority,
) -> Result<String, String> {
    let suffix = format!(" docker-authority {}", docker_authority_binding(authority));
    bound_phrase
        .strip_suffix(&suffix)
        .map(str::to_string)
        .ok_or_else(|| "orphan-prune-docker-authority-mismatch".to_string())
}

fn bind_docker_authority_plan(
    mut plan: container_orphan_reclaim::ContainerOrphanPlan,
    authority: &DockerAmbientAuthority,
) -> container_orphan_reclaim::ContainerOrphanPlan {
    for category in &mut plan.categories {
        if let Some(base_phrase) = category.approval_phrase.take() {
            category.approval_phrase =
                Some(bind_docker_authority_approval(&base_phrase, authority));
        }
    }
    plan
}

fn suppress_context_mutation_authority(
    mut plan: container_orphan_reclaim::ContainerOrphanPlan,
) -> container_orphan_reclaim::ContainerOrphanPlan {
    for category in &mut plan.categories {
        category.approval_phrase = None;
        category.prune_command = None;
    }
    plan
}

/// Probes every supported container runtime target read-only and audits all orphan categories.
/// An explicit DOCKER_HOST may acquire mutation authority because every later command is pinned to
/// that exact endpoint. Named/default Docker contexts and the explicit Colima context remain
/// read-only because a context name is mutable and cannot safely authorize a later delete without
/// a private immutable context/TLS snapshot.
#[tauri::command(async)]
pub fn inspect_container_orphans(
    app: tauri::AppHandle,
) -> Vec<container_orphan_reclaim::ContainerOrphanPlan> {
    let receipt_dir = container_receipt_dir(&app).ok();
    let docker_authority = docker_ambient_authority();
    let pinned_docker_authority = docker_authority
        .as_ref()
        .map_err(Clone::clone)
        .and_then(|authority| pin_docker_authority(&docker_binary(), authority));
    runtime_kinds_for_docker_authority(&docker_authority)
        .into_iter()
        .filter_map(|kind| {
            let target = if kind == container_orphan_reclaim::ContainerRuntimeKind::DockerNative {
                pinned_docker_target(pinned_docker_authority.as_ref().ok()?).ok()?
            } else {
                target_for_kind(kind).ok()?
            };
            let plan = container_orphan_public::sanitize_plan(receipt_dir.as_ref().map_or_else(
                || container_orphan_reclaim::probe_container_orphans(&target),
                |dir| {
                    container_orphan_reclaim::probe_container_orphans_with_receipt_dir(&target, dir)
                },
            ));
            match kind {
                container_orphan_reclaim::ContainerRuntimeKind::DockerNative => {
                    let authority = pinned_docker_authority.as_ref().ok()?;
                    Some(bind_docker_authority_plan(plan, authority))
                }
                container_orphan_reclaim::ContainerRuntimeKind::DockerColimaContext => {
                    Some(suppress_context_mutation_authority(plan))
                }
                container_orphan_reclaim::ContainerRuntimeKind::PodmanMachine => Some(plan),
            }
        })
        .collect()
}

/// Re-audits one runtime/category immediately before exact identity-bound deletion. Docker-native
/// mutation is permitted only for an explicit, bounded DOCKER_HOST that can be reused verbatim for
/// every audit and delete command. Named/default Docker contexts and the Colima named context fail
/// closed because re-resolving a mutable context after approval can redirect deletion to another
/// daemon.
#[tauri::command(async)]
pub fn execute_container_orphan_prune(
    app: tauri::AppHandle,
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
    if kind == container_orphan_reclaim::ContainerRuntimeKind::DockerColimaContext {
        return Err(format!("orphan-prune-{IMMUTABLE_CONTEXT_REQUIRED}"));
    }
    let category = parse_category(&category)?;
    let (target, docker_authority) =
        if kind == container_orphan_reclaim::ContainerRuntimeKind::DockerNative {
            let ambient =
                docker_ambient_authority().map_err(|error| format!("orphan-prune-{error}"))?;
            let pinned = pin_docker_authority(&docker_binary(), &ambient)
                .map_err(|error| format!("orphan-prune-{error}"))?;
            (pinned_docker_target(&pinned)?, Some(pinned))
        } else {
            (target_for_kind(kind)?, None)
        };
    validate_requested_scope(&target, &scope_name)?;
    let engine_confirmation =
        if kind == container_orphan_reclaim::ContainerRuntimeKind::DockerNative {
            unbind_docker_authority_approval(
                &confirmation_phrase,
                docker_authority
                    .as_ref()
                    .ok_or("docker-authority-not-pinned")?,
            )?
        } else {
            confirmation_phrase
        };
    let receipt_dir = container_receipt_dir(&app)?;
    container_orphan_reclaim::execute_container_orphan_prune(
        &target,
        category,
        &engine_confirmation,
        &rationale,
        now_ms(),
        &receipt_dir,
    )
    .map(container_orphan_public::sanitize_execution)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receipt_directory_creation_handles_missing_app_data_parent() {
        let temp = tempfile::tempdir().unwrap();
        let receipt_dir = temp
            .path()
            .join("not-created-yet")
            .join("app-data")
            .join("container-orphan-receipts");

        ensure_container_receipt_dir(&receipt_dir).unwrap();

        assert!(receipt_dir.is_dir());
    }

    #[test]
    fn command_inputs_fail_closed_without_reflecting_untrusted_tokens() {
        assert_eq!(
            parse_runtime_kind("secret-runtime").unwrap_err(),
            "unknown-runtime-kind"
        );
        assert_eq!(
            parse_category("secret-category").unwrap_err(),
            "unknown-orphan-category"
        );
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
    fn docker_host_overrides_configured_colima_and_keeps_ambient_target() {
        use container_orphan_reclaim::ContainerRuntimeKind::{
            DockerColimaContext, DockerNative, PodmanMachine,
        };

        let authority = resolve_docker_ambient_authority(
            DockerContextEnvironment::AbsentOrEmpty,
            DockerHostEnvironment::Host("unix:///tmp/customer-docker.sock".to_string()),
            Some("colima".to_string()),
        );
        assert_eq!(
            authority,
            Ok(DockerAmbientAuthority::Host(
                "unix:///tmp/customer-docker.sock".to_string()
            ))
        );
        assert_eq!(
            runtime_kinds_for_docker_authority(&authority),
            vec![DockerNative, DockerColimaContext, PodmanMachine]
        );
    }

    #[test]
    fn named_contexts_are_read_only_and_do_not_duplicate_colima_target() {
        use container_orphan_reclaim::ContainerRuntimeKind::{DockerColimaContext, PodmanMachine};

        let authority = resolve_docker_ambient_authority(
            DockerContextEnvironment::Context("colima".to_string()),
            DockerHostEnvironment::Host("unix:///tmp/ignored-by-context.sock".to_string()),
            Some("desktop-linux".to_string()),
        );
        assert_eq!(
            authority,
            Ok(DockerAmbientAuthority::Context("colima".to_string()))
        );
        assert_eq!(
            runtime_kinds_for_docker_authority(&authority),
            vec![DockerColimaContext, PodmanMachine]
        );
        assert_eq!(
            pin_docker_authority(&docker_binary(), authority.as_ref().unwrap()).unwrap_err(),
            IMMUTABLE_CONTEXT_REQUIRED
        );
        assert_eq!(
            pin_docker_authority(&docker_binary(), &DockerAmbientAuthority::Default).unwrap_err(),
            IMMUTABLE_CONTEXT_REQUIRED
        );
    }

    #[test]
    fn invalid_explicit_docker_authority_must_not_fall_through_to_native_target() {
        use container_orphan_reclaim::ContainerRuntimeKind::{
            DockerColimaContext, DockerNative, PodmanMachine,
        };

        let invalid_context = resolve_docker_ambient_authority(
            DockerContextEnvironment::Invalid,
            DockerHostEnvironment::AbsentOrEmpty,
            Some("desktop-linux".to_string()),
        );
        let invalid_host = resolve_docker_ambient_authority(
            DockerContextEnvironment::AbsentOrEmpty,
            DockerHostEnvironment::Invalid,
            Some("desktop-linux".to_string()),
        );

        assert!(!runtime_kinds_for_docker_authority(&invalid_context).contains(&DockerNative));
        assert!(!runtime_kinds_for_docker_authority(&invalid_host).contains(&DockerNative));
        assert_eq!(
            runtime_kinds_for_docker_authority(&invalid_host),
            vec![DockerColimaContext, PodmanMachine]
        );
    }

    #[test]
    fn docker_native_approval_is_authority_bound_without_disclosing_endpoint() {
        let base = "DiskSage image orphan prune 승인 abcdef";
        let host_a = DockerAmbientAuthority::Host("unix:///tmp/customer-a.sock".to_string());
        let host_b = DockerAmbientAuthority::Host("unix:///tmp/customer-b.sock".to_string());
        let context = DockerAmbientAuthority::Context("desktop-linux".to_string());
        let default = DockerAmbientAuthority::Default;
        let bound = bind_docker_authority_approval(base, &host_a);

        assert_ne!(bound, bind_docker_authority_approval(base, &host_b));
        assert_ne!(bound, bind_docker_authority_approval(base, &context));
        assert_ne!(bound, bind_docker_authority_approval(base, &default));
        assert!(!bound.contains("customer-a.sock"));
        assert_eq!(
            unbind_docker_authority_approval(&bound, &host_a).unwrap(),
            base
        );
        assert_eq!(
            unbind_docker_authority_approval(&bound, &host_b).unwrap_err(),
            "orphan-prune-docker-authority-mismatch"
        );
        let target =
            pinned_docker_target(&pin_docker_authority(&docker_binary(), &host_a).unwrap())
                .unwrap();
        let prefix = target.command_prefix().unwrap();
        assert_eq!(
            &prefix[prefix.len() - 2..],
            ["--host", "unix:///tmp/customer-a.sock"]
        );
    }

    #[cfg(unix)]
    #[test]
    fn named_context_never_produces_a_mutable_context_target() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let docker = temp.path().join("docker");
        std::fs::write(
            &docker,
            r#"#!/bin/sh
printf '%s\n' 'a mutable named context must never be consulted for mutation' >&2
exit 41
"#,
        )
        .unwrap();
        std::fs::set_permissions(&docker, std::fs::Permissions::from_mode(0o700)).unwrap();

        let authority = DockerAmbientAuthority::Context("customer-local".to_string());
        assert_eq!(
            pin_docker_authority(&docker, &authority).unwrap_err(),
            IMMUTABLE_CONTEXT_REQUIRED
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
            docker_context_environment(Some(OsString::from(
                "x".repeat(MAX_DOCKER_CONTEXT_BYTES + 1)
            ))),
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
    fn docker_host_environment_is_bounded_and_fail_closed() {
        assert_eq!(
            docker_host_environment(None),
            DockerHostEnvironment::AbsentOrEmpty
        );
        assert_eq!(
            docker_host_environment(Some(OsString::new())),
            DockerHostEnvironment::AbsentOrEmpty
        );
        assert_eq!(
            docker_host_environment(Some(OsString::from("unix:///tmp/docker.sock"))),
            DockerHostEnvironment::Host("unix:///tmp/docker.sock".to_string())
        );
        assert_eq!(
            docker_host_environment(Some(OsString::from("bad\nhost"))),
            DockerHostEnvironment::Invalid
        );
        assert_eq!(
            docker_host_environment(Some(OsString::from("x".repeat(MAX_DOCKER_HOST_BYTES + 1)))),
            DockerHostEnvironment::Invalid
        );

        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            assert_eq!(
                docker_host_environment(Some(OsString::from_vec(vec![0xff]))),
                DockerHostEnvironment::Invalid
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
