use disksage_lib::podman_reclaim::{
    probe_podman_reclaim, prune_dangling_images, DEFAULT_PODMAN_MACHINE, DEFAULT_PROBE_TIMEOUT,
};
use std::path::PathBuf;
use std::time::Duration;

const USAGE: &str = "Usage: disksage-podman-reclaim-plan [--machine NAME] [--podman-bin PATH] [--timeout-seconds N] [--pretty]\n\
       disksage-podman-reclaim-plan --execute-dangling --confirmation-phrase PHRASE --rationale TEXT [--machine NAME] [--podman-bin PATH] [--pretty]\n\
Builds read-only Podman evidence by default. The execution form deletes only the exact revalidated dangling image IDs.";

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
    let mut machine = DEFAULT_PODMAN_MACHINE.to_string();
    let mut podman_bin = PathBuf::from("podman");
    let mut timeout = DEFAULT_PROBE_TIMEOUT;
    let mut pretty = false;
    let mut execute_dangling = false;
    let mut confirmation_phrase = None;
    let mut rationale = None;
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--machine") => {
                machine = next_utf8_argument(
                    &mut args,
                    "--machine requires a name",
                    "--machine requires a UTF-8 name",
                )?;
            }
            Some("--podman-bin") => {
                podman_bin = PathBuf::from(
                    args.next()
                        .ok_or_else(|| "--podman-bin requires a path".to_string())?,
                );
            }
            Some("--timeout-seconds") => {
                let seconds = next_utf8_argument(
                    &mut args,
                    "--timeout-seconds requires an integer",
                    "--timeout-seconds requires a UTF-8 integer",
                )?
                .parse::<u64>()
                .map_err(|_| "--timeout-seconds requires an integer".to_string())?;
                if !(1..=60).contains(&seconds) {
                    return Err("--timeout-seconds must be between 1 and 60".to_string());
                }
                timeout = Duration::from_secs(seconds);
            }
            Some("--pretty") => pretty = true,
            Some("--execute-dangling") => execute_dangling = true,
            Some("--confirmation-phrase") => {
                confirmation_phrase = Some(next_utf8_argument(
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
            Some("-h" | "--help") => {
                println!("{USAGE}");
                return Ok(());
            }
            Some(_) => return Err(format!("unknown option\n{USAGE}")),
            None => return Err(format!("unknown option (non-UTF-8)\n{USAGE}")),
        }
    }

    if !execute_dangling && (confirmation_phrase.is_some() || rationale.is_some()) {
        return Err(format!(
            "execution arguments require --execute-dangling\n{USAGE}"
        ));
    }
    let value = if execute_dangling {
        let confirmation_phrase = confirmation_phrase
            .as_deref()
            .ok_or_else(|| "--execute-dangling requires --confirmation-phrase".to_string())?;
        let rationale = rationale
            .as_deref()
            .ok_or_else(|| "--execute-dangling requires --rationale".to_string())?;
        let executed_at_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "system clock is before the Unix epoch".to_string())?
            .as_millis() as u64;
        serde_json::to_value(prune_dangling_images(
            &podman_bin,
            &machine,
            confirmation_phrase,
            rationale,
            executed_at_ms,
        )?)
    } else {
        serde_json::to_value(probe_podman_reclaim(&podman_bin, &machine, timeout))
    }
    .map_err(|error| error.to_string())?;
    let json = if pretty {
        serde_json::to_string_pretty(&value)
    } else {
        serde_json::to_string(&value)
    }
    .map_err(|error| error.to_string())?;
    println!("{json}");
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("disksage-podman-reclaim-plan: {error}");
        std::process::exit(2);
    }
}
