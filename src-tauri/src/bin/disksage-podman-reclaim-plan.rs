use disksage_lib::podman_reclaim::{
    probe_podman_reclaim, DEFAULT_PODMAN_MACHINE, DEFAULT_PROBE_TIMEOUT,
};
use std::path::PathBuf;
use std::time::Duration;

const USAGE: &str = "Usage: disksage-podman-reclaim-plan [--machine NAME] [--podman-bin PATH] [--timeout-seconds N] [--pretty]\n\
Builds read-only Podman guest/raw allocation evidence. It never prunes, removes, trims, or stops anything.";

fn run() -> Result<(), String> {
    let mut machine = DEFAULT_PODMAN_MACHINE.to_string();
    let mut podman_bin = PathBuf::from("podman");
    let mut timeout = DEFAULT_PROBE_TIMEOUT;
    let mut pretty = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--machine" => {
                machine = args
                    .next()
                    .ok_or_else(|| "--machine requires a name".to_string())?;
            }
            "--podman-bin" => {
                podman_bin = PathBuf::from(
                    args.next()
                        .ok_or_else(|| "--podman-bin requires a path".to_string())?,
                );
            }
            "--timeout-seconds" => {
                let seconds = args
                    .next()
                    .ok_or_else(|| "--timeout-seconds requires an integer".to_string())?
                    .parse::<u64>()
                    .map_err(|_| "--timeout-seconds requires an integer".to_string())?;
                if !(1..=60).contains(&seconds) {
                    return Err("--timeout-seconds must be between 1 and 60".to_string());
                }
                timeout = Duration::from_secs(seconds);
            }
            "--pretty" => pretty = true,
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            value => return Err(format!("unknown option: {value}\n{USAGE}")),
        }
    }

    let plan = probe_podman_reclaim(&podman_bin, &machine, timeout);
    let json = if pretty {
        serde_json::to_string_pretty(&plan)
    } else {
        serde_json::to_string(&plan)
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
