use disksage_lib::shared_temp_reclaim::plan_shared_temp_reclaim;
use std::path::PathBuf;

const USAGE: &str = "Usage: disksage-shared-temp-reclaim-plan --path ABSOLUTE_PATH [--pretty]\n\
Inspects one top-level completed DiskSage temporary artifact. It never deletes or modifies it.";

fn run() -> Result<(), String> {
    let mut path = None;
    let mut pretty = false;
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--path") => {
                path = Some(PathBuf::from(
                    args.next()
                        .ok_or_else(|| "--path requires a value".to_string())?,
                ));
            }
            Some("--pretty") => pretty = true,
            Some("-h" | "--help") => {
                println!("{USAGE}");
                return Ok(());
            }
            Some(_) => return Err(format!("unknown option\n{USAGE}")),
            None => return Err(format!("unknown non-UTF-8 option\n{USAGE}")),
        }
    }
    let path = path.ok_or_else(|| format!("--path is required\n{USAGE}"))?;
    let plan = plan_shared_temp_reclaim(&path, disksage_lib::cloud::system_now_ms())?;
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
        eprintln!("disksage-shared-temp-reclaim-plan: {error}");
        std::process::exit(2);
    }
}
