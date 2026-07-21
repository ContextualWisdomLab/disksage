use disksage_lib::reclaim::{plan_reclaim, PlannedOperation};
use std::path::PathBuf;

const USAGE: &str = "Usage: disksage-reclaim-plan [--operation trash|delete] [--pretty] PATH...\n\
Builds read-only logical/allocation evidence. It never moves or deletes files.";

fn run() -> Result<(), String> {
    let mut operation = PlannedOperation::Trash;
    let mut pretty = false;
    let mut paths = Vec::new();
    let mut args = std::env::args().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--operation" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--operation requires trash or delete".to_string())?;
                operation = value.parse()?;
            }
            "--pretty" => pretty = true,
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            "--" => {
                paths.extend(args.map(PathBuf::from));
                break;
            }
            value if value.starts_with('-') => {
                return Err(format!("unknown option: {value}\n{USAGE}"));
            }
            value => paths.push(PathBuf::from(value)),
        }
    }

    let plan = plan_reclaim(&paths, operation)?;
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
        eprintln!("disksage-reclaim-plan: {error}");
        std::process::exit(2);
    }
}
