use disksage_lib::container_orphan_public::sanitize_plan;
use disksage_lib::container_orphan_reclaim::{
    execute_container_orphan_prune, probe_container_orphans_with_receipt_dir, ContainerRuntimeKind,
    ContainerRuntimeTarget, OrphanCategory,
};
use std::path::PathBuf;

const USAGE: &str = "Usage: disksage-container-orphan-plan --runtime <docker-native|docker-colima-context|podman-machine> --receipt-dir ABSOLUTE_PRIVATE_DIR [--scope NAME] [--bin PATH] [--pretty] [--execute CATEGORY --confirm EXACT_PHRASE --rationale TEXT]\n\
Builds orphan evidence for containers, images, volumes, and networks. Execution re-audits and removes only the exact approved candidate set.";

fn next_utf8_argument(
    args: &mut impl Iterator<Item = std::ffi::OsString>,
    missing_message: &str,
    invalid_message: &str,
) -> Result<String, String> {
    args.next()
        .ok_or_else(|| missing_message.to_string())?
        .into_string()
        .map_err(|_| invalid_message.to_string())
}

fn parse_category(value: &str) -> Result<OrphanCategory, String> {
    match value {
        "container" => Ok(OrphanCategory::Container),
        "image" => Ok(OrphanCategory::Image),
        "volume" => Ok(OrphanCategory::Volume),
        "network" => Ok(OrphanCategory::Network),
        "build_cache" => Ok(OrphanCategory::BuildCache),
        _ => Err(format!("unsupported category\n{USAGE}")),
    }
}

fn ensure_cli_execution_authority(runtime: ContainerRuntimeKind) -> Result<(), String> {
    match runtime {
        ContainerRuntimeKind::DockerNative => {
            Err("docker-native-cli-execution-requires-authority-binding".into())
        }
        ContainerRuntimeKind::DockerColimaContext => {
            Err("docker-context-cli-execution-requires-immutable-authority".into())
        }
        ContainerRuntimeKind::PodmanMachine => Ok(()),
    }
}

fn suppress_unexecutable_docker_plan(
    mut plan: disksage_lib::container_orphan_reclaim::ContainerOrphanPlan,
    runtime: ContainerRuntimeKind,
) -> disksage_lib::container_orphan_reclaim::ContainerOrphanPlan {
    if matches!(
        runtime,
        ContainerRuntimeKind::DockerNative | ContainerRuntimeKind::DockerColimaContext
    ) {
        for category in &mut plan.categories {
            category.approval_phrase = None;
            category.prune_command = None;
        }
    }
    plan
}

fn run() -> Result<(), String> {
    let raw_args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    let help_count = raw_args
        .iter()
        .filter(|arg| matches!(arg.to_str(), Some("-h" | "--help")))
        .count();
    if help_count > 0 {
        if raw_args.len() == 1 && help_count == 1 {
            println!("{USAGE}");
            return Ok(());
        }
        return Err(format!("help must be used alone\n{USAGE}"));
    }

    let mut runtime: Option<ContainerRuntimeKind> = None;
    let mut scope: Option<String> = None;
    let mut binary_path: Option<PathBuf> = None;
    let mut pretty = false;
    let mut execute = None;
    let mut confirmation = None;
    let mut rationale = None;
    let mut receipt_dir: Option<PathBuf> = None;
    let mut args = raw_args.into_iter();
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--runtime") => {
                if runtime.is_some() {
                    return Err(format!("--runtime may be supplied once\n{USAGE}"));
                }
                let value = next_utf8_argument(
                    &mut args,
                    "--runtime requires a kind",
                    "--runtime requires a UTF-8 kind",
                )?;
                runtime = Some(match value.as_str() {
                    "docker-native" => ContainerRuntimeKind::DockerNative,
                    "docker-colima-context" => ContainerRuntimeKind::DockerColimaContext,
                    "podman-machine" => ContainerRuntimeKind::PodmanMachine,
                    _ => return Err(format!("unsupported runtime kind\n{USAGE}")),
                });
            }
            Some("--scope") => {
                if scope.is_some() {
                    return Err(format!("--scope may be supplied once\n{USAGE}"));
                }
                scope = Some(next_utf8_argument(
                    &mut args,
                    "--scope requires a name",
                    "--scope requires a UTF-8 name",
                )?);
            }
            Some("--bin") => {
                if binary_path.is_some() {
                    return Err(format!("--bin may be supplied once\n{USAGE}"));
                }
                binary_path = Some(PathBuf::from(
                    args.next()
                        .ok_or_else(|| "--bin requires a path".to_string())?,
                ));
            }
            Some("--pretty") => {
                if pretty {
                    return Err(format!("--pretty may be supplied once\n{USAGE}"));
                }
                pretty = true;
            }
            Some("--execute") => {
                if execute.is_some() {
                    return Err(format!("--execute may be supplied once\n{USAGE}"));
                }
                execute = Some(parse_category(&next_utf8_argument(
                    &mut args,
                    "--execute requires a category",
                    "--execute requires a UTF-8 category",
                )?)?);
            }
            Some("--confirm") if confirmation.is_none() => {
                confirmation = Some(next_utf8_argument(
                    &mut args,
                    "--confirm requires the exact phrase",
                    "--confirm requires a UTF-8 phrase",
                )?)
            }
            Some("--rationale") if rationale.is_none() => {
                rationale = Some(next_utf8_argument(
                    &mut args,
                    "--rationale requires text",
                    "--rationale requires UTF-8 text",
                )?)
            }
            Some("--receipt-dir") if receipt_dir.is_none() => {
                receipt_dir =
                    Some(PathBuf::from(args.next().ok_or_else(|| {
                        "--receipt-dir requires a path".to_string()
                    })?))
            }
            Some(_) => return Err(format!("unknown option\n{USAGE}")),
            None => return Err(format!("non-UTF-8 argument\n{USAGE}")),
        }
    }
    let runtime = runtime.ok_or_else(|| format!("--runtime is required\n{USAGE}"))?;
    match runtime {
        ContainerRuntimeKind::DockerNative if scope.is_some() => {
            return Err(format!("--scope is not valid for docker-native\n{USAGE}"));
        }
        ContainerRuntimeKind::DockerColimaContext if scope.is_none() => {
            return Err(format!(
                "--scope is required for docker-colima-context\n{USAGE}"
            ));
        }
        ContainerRuntimeKind::PodmanMachine if scope.is_none() => {
            return Err(format!("--scope is required for podman-machine\n{USAGE}"));
        }
        _ => {}
    }
    if execute.is_some() {
        ensure_cli_execution_authority(runtime)?;
    }
    let binary_path = binary_path.unwrap_or_else(|| {
        PathBuf::from(match runtime {
            ContainerRuntimeKind::PodmanMachine => "podman",
            ContainerRuntimeKind::DockerNative | ContainerRuntimeKind::DockerColimaContext => {
                "docker"
            }
        })
    });
    let target = ContainerRuntimeTarget::new(runtime, binary_path, scope)?;
    if let Some(category) = execute {
        let receipt_dir =
            receipt_dir.ok_or_else(|| format!("--execute requires --receipt-dir\n{USAGE}"))?;
        let confirmation =
            confirmation.ok_or_else(|| format!("--execute requires --confirm\n{USAGE}"))?;
        let rationale =
            rationale.ok_or_else(|| format!("--execute requires --rationale\n{USAGE}"))?;
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "system time is before epoch".to_string())?
            .as_millis() as u64;
        let result = execute_container_orphan_prune(
            &target,
            category,
            &confirmation,
            &rationale,
            now_ms,
            &receipt_dir,
        )?;
        println!(
            "{}",
            if pretty {
                serde_json::to_string_pretty(&result)
            } else {
                serde_json::to_string(&result)
            }
            .map_err(|error| error.to_string())?
        );
        return Ok(());
    }
    if confirmation.is_some() || rationale.is_some() {
        return Err(format!(
            "--confirm and --rationale require --execute\n{USAGE}"
        ));
    }
    let plan = sanitize_plan(receipt_dir.as_ref().map_or_else(
        || disksage_lib::container_orphan_reclaim::probe_container_orphans(&target),
        |dir| probe_container_orphans_with_receipt_dir(&target, dir),
    ));
    let plan = suppress_unexecutable_docker_plan(plan, runtime);
    if pretty {
        println!(
            "{}",
            serde_json::to_string_pretty(&plan).map_err(|error| error.to_string())?
        );
    } else {
        println!(
            "{}",
            serde_json::to_string(&plan).map_err(|error| error.to_string())?
        );
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_exposes_every_backend_orphan_category() {
        assert_eq!(
            parse_category("container").unwrap(),
            OrphanCategory::Container
        );
        assert_eq!(parse_category("image").unwrap(), OrphanCategory::Image);
        assert_eq!(parse_category("volume").unwrap(), OrphanCategory::Volume);
        assert_eq!(parse_category("network").unwrap(), OrphanCategory::Network);
        assert_eq!(
            parse_category("build_cache").unwrap(),
            OrphanCategory::BuildCache
        );
        assert!(parse_category("all").is_err());
    }

    #[test]
    fn cli_rejects_mutable_docker_context_execution_before_runtime_access() {
        assert_eq!(
            ensure_cli_execution_authority(ContainerRuntimeKind::DockerNative).unwrap_err(),
            "docker-native-cli-execution-requires-authority-binding"
        );
        assert_eq!(
            ensure_cli_execution_authority(ContainerRuntimeKind::DockerColimaContext).unwrap_err(),
            "docker-context-cli-execution-requires-immutable-authority"
        );
        assert!(ensure_cli_execution_authority(ContainerRuntimeKind::PodmanMachine).is_ok());
    }
}
