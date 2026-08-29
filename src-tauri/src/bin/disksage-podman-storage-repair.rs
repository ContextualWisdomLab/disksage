use disksage_lib::podman_reclaim::{
    execute_podman_storage_repair, plan_podman_storage_repair, DEFAULT_PODMAN_MACHINE,
};
use std::ffi::OsString;
use std::path::PathBuf;

const USAGE: &str = "Usage: disksage-podman-storage-repair [--machine NAME] [--podman-bin PATH] [--execute --confirm PHRASE --rationale TEXT]\nWithout --execute, prints a read-only native storage-check plan.";

fn is_help(arg: &OsString) -> bool {
    matches!(arg.to_str(), Some("-h" | "--help"))
}

fn run() -> Result<(), String> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.len() == 1 && is_help(&args[0]) {
        println!("{USAGE}");
        return Ok(());
    }
    if args.iter().any(is_help) {
        return Err("invalid-mixed-help-request".into());
    }

    let mut machine = DEFAULT_PODMAN_MACHINE.to_string();
    let mut podman_bin = PathBuf::from("podman");
    let mut execute = false;
    let mut confirmation = None;
    let mut rationale = None;
    let mut seen_machine = false;
    let mut seen_podman_bin = false;
    let mut seen_execute = false;
    let mut seen_confirmation = false;
    let mut seen_rationale = false;
    let mut index = 0usize;

    while index < args.len() {
        let value = args[index].to_str().ok_or("invalid-utf8-option")?;
        index += 1;
        match value {
            "--machine" => {
                if seen_machine {
                    return Err("duplicate-machine-option".into());
                }
                seen_machine = true;
                machine = args
                    .get(index)
                    .ok_or("machine-value-required")?
                    .clone()
                    .into_string()
                    .map_err(|_| "invalid-machine-name")?;
                index += 1;
            }
            "--podman-bin" => {
                if seen_podman_bin {
                    return Err("duplicate-podman-bin-option".into());
                }
                seen_podman_bin = true;
                podman_bin = args
                    .get(index)
                    .ok_or("podman-bin-value-required")?
                    .into();
                index += 1;
            }
            "--execute" => {
                if seen_execute {
                    return Err("duplicate-execute-option".into());
                }
                seen_execute = true;
                execute = true;
            }
            "--confirm" => {
                if seen_confirmation {
                    return Err("duplicate-confirm-option".into());
                }
                seen_confirmation = true;
                confirmation = Some(
                    args.get(index)
                        .ok_or("confirmation-value-required")?
                        .clone()
                        .into_string()
                        .map_err(|_| "invalid-confirmation")?,
                );
                index += 1;
            }
            "--rationale" => {
                if seen_rationale {
                    return Err("duplicate-rationale-option".into());
                }
                seen_rationale = true;
                rationale = Some(
                    args.get(index)
                        .ok_or("rationale-value-required")?
                        .clone()
                        .into_string()
                        .map_err(|_| "invalid-rationale")?,
                );
                index += 1;
            }
            _ => return Err("unknown-option".into()),
        }
    }

    if !execute && (confirmation.is_some() || rationale.is_some()) {
        return Err("execution-authority-option-without-execute".into());
    }

    let value = if execute {
        serde_json::to_value(execute_podman_storage_repair(
            &podman_bin,
            &machine,
            confirmation
                .as_deref()
                .ok_or("execute-confirmation-required")?,
            rationale.as_deref().ok_or("execute-rationale-required")?,
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
