use disksage_lib::podman_reclaim::{
    plan_empty_dangling_volumes, prune_empty_dangling_volumes, DEFAULT_PODMAN_MACHINE,
};
use std::path::PathBuf;

const USAGE: &str = "usage: disksage-podman-empty-volumes [--machine NAME] [--podman-bin PATH]\n\
       disksage-podman-empty-volumes --execute --confirmation-phrase TEXT --rationale TEXT [--machine NAME] [--podman-bin PATH]";

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
    let mut execute = false;
    let mut phrase = None;
    let mut rationale = None;
    let mut machine = DEFAULT_PODMAN_MACHINE.to_string();
    let mut podman = PathBuf::from("podman");
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--execute") => execute = true,
            Some("--confirmation-phrase") => {
                phrase = Some(next_utf8_argument(
                    &mut args,
                    "--confirmation-phrase requires a value",
                    "--confirmation-phrase requires a UTF-8 value",
                )?);
            }
            Some("--rationale") => {
                rationale = Some(next_utf8_argument(
                    &mut args,
                    "--rationale requires a value",
                    "--rationale requires a UTF-8 value",
                )?);
            }
            Some("--machine") => {
                machine = next_utf8_argument(
                    &mut args,
                    "--machine requires a name",
                    "--machine requires a UTF-8 name",
                )?;
            }
            Some("--podman-bin") => {
                podman = PathBuf::from(
                    args.next()
                        .ok_or_else(|| "--podman-bin requires a path".to_string())?,
                );
            }
            Some("--help" | "-h") => {
                println!("{USAGE}");
                return Ok(());
            }
            Some(_) => return Err(format!("unknown argument\n{USAGE}")),
            None => return Err(format!("unknown argument (non-UTF-8)\n{USAGE}")),
        }
    }

    let result = if execute {
        prune_empty_dangling_volumes(
            &podman,
            &machine,
            phrase.as_deref().unwrap_or_default(),
            rationale.as_deref().unwrap_or_default(),
        )
        .and_then(|value| serde_json::to_value(value).map_err(|error| error.to_string()))
    } else {
        plan_empty_dangling_volumes(&podman, &machine)
            .and_then(|value| serde_json::to_value(value).map_err(|error| error.to_string()))
    };
    match result {
        Ok(value) => println!("{}", serde_json::to_string_pretty(&value).unwrap()),
        Err(error) => return Err(error),
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("disksage-podman-empty-volumes: {error}");
        std::process::exit(2);
    }
}
