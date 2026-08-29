use disksage_lib::podman_reclaim::{
    execute_podman_storage_repair, plan_podman_storage_repair, DEFAULT_PODMAN_MACHINE,
};
use std::path::PathBuf;

const USAGE: &str = "Usage: disksage-podman-storage-repair [--machine NAME] [--podman-bin PATH] [--execute --confirm PHRASE --rationale TEXT]\nWithout --execute, prints a read-only native storage-check plan.";

fn run() -> Result<(), String> {
    let mut machine = DEFAULT_PODMAN_MACHINE.to_string();
    let mut podman_bin = PathBuf::from("podman");
    let mut execute = false;
    let mut confirmation = None;
    let mut rationale = None;
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        let value = arg.to_str().ok_or("invalid-utf8-option")?;
        match value {
            "--machine" => {
                machine = args
                    .next()
                    .ok_or("--machine requires NAME")?
                    .into_string()
                    .map_err(|_| "invalid-machine-name")?
            }
            "--podman-bin" => podman_bin = args.next().ok_or("--podman-bin requires PATH")?.into(),
            "--execute" => execute = true,
            "--confirm" => {
                confirmation = Some(
                    args.next()
                        .ok_or("--confirm requires PHRASE")?
                        .into_string()
                        .map_err(|_| "invalid-confirmation")?,
                )
            }
            "--rationale" => {
                rationale = Some(
                    args.next()
                        .ok_or("--rationale requires TEXT")?
                        .into_string()
                        .map_err(|_| "invalid-rationale")?,
                )
            }
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            _ => return Err(format!("unknown option: {value}\n{USAGE}")),
        }
    }
    let value = if execute {
        serde_json::to_value(execute_podman_storage_repair(
            &podman_bin,
            &machine,
            confirmation
                .as_deref()
                .ok_or("--execute requires --confirm")?,
            rationale
                .as_deref()
                .ok_or("--execute requires --rationale")?,
            disksage_lib::cloud::system_now_ms(),
        )?)
        .map_err(|error| error.to_string())?
    } else {
        serde_json::to_value(plan_podman_storage_repair(&podman_bin, &machine)?)
            .map_err(|error| error.to_string())?
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&value).map_err(|error| error.to_string())?
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("disksage-podman-storage-repair: {error}");
        std::process::exit(2);
    }
}
