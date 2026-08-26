use disksage_lib::container_orphan_public::sanitize_plan;
use disksage_lib::container_orphan_reclaim::{
    probe_container_orphans, ContainerRuntimeKind, ContainerRuntimeTarget,
};
use std::path::PathBuf;

const USAGE: &str = "Usage: disksage-container-orphan-plan --runtime <docker-native|docker-colima-context|podman-machine> [--scope NAME] [--bin PATH] [--pretty]\n\
Builds read-only orphan evidence for containers, images, volumes, and networks across docker/podman/colima. It never prunes anything.";

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
            return Err(format!("--scope is required for docker-colima-context\n{USAGE}"));
        }
        ContainerRuntimeKind::PodmanMachine if scope.is_none() => {
            return Err(format!("--scope is required for podman-machine\n{USAGE}"));
        }
        _ => {}
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
    let plan = sanitize_plan(probe_container_orphans(&target));
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
