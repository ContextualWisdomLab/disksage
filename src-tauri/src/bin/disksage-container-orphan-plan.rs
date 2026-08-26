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
    let mut runtime: Option<ContainerRuntimeKind> = None;
    let mut scope: Option<String> = None;
    let mut binary_path = PathBuf::from("docker");
    let mut pretty = false;
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--runtime") => {
                let value = next_utf8_argument(
                    &mut args,
                    "--runtime requires a kind",
                    "--runtime requires a UTF-8 kind",
                )?;
                runtime = Some(match value.as_str() {
                    "docker-native" => ContainerRuntimeKind::DockerNative,
                    "docker-colima-context" => ContainerRuntimeKind::DockerColimaContext,
                    "podman-machine" => ContainerRuntimeKind::PodmanMachine,
                    other => return Err(format!("unknown runtime kind: {other}\n{USAGE}")),
                });
            }
            Some("--scope") => {
                scope = Some(next_utf8_argument(
                    &mut args,
                    "--scope requires a name",
                    "--scope requires a UTF-8 name",
                )?);
            }
            Some("--bin") => {
                binary_path = PathBuf::from(
                    args.next()
                        .ok_or_else(|| "--bin requires a path".to_string())?,
                );
            }
            Some("--pretty") => pretty = true,
            Some("-h" | "--help") => {
                println!("{USAGE}");
                return Ok(());
            }
            Some(_) => return Err(format!("unknown option\n{USAGE}")),
            None => return Err(format!("non-UTF-8 argument\n{USAGE}")),
        }
    }
    let runtime = runtime.ok_or_else(|| format!("--runtime is required\n{USAGE}"))?;
    let target = ContainerRuntimeTarget::new(runtime, binary_path, scope)?;
    let plan = probe_container_orphans(&target);
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
