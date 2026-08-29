//! Read-only, path-free OneDrive provider-cache pressure diagnostic.

#[cfg(not(coverage))]
fn run() -> Result<(), String> {
    use disksage_lib::onedrive_internal_pressure::{
        assess, collect, OneDriveInternalPressureObservation,
    };
    use std::path::PathBuf;

    let mut previous = None;
    let mut stall_after_ms = None;
    let mut args = std::env::args_os().skip(1);
    while let Some(argument) = args.next() {
        match argument.to_str() {
            Some("--previous") => {
                previous = Some(PathBuf::from(
                    args.next().ok_or("--previous requires JSON")?,
                ))
            }
            Some("--stall-after-ms") => {
                stall_after_ms = Some(
                    args.next()
                        .and_then(|value| value.to_str().and_then(|value| value.parse().ok()))
                        .filter(|value| *value > 0)
                        .ok_or("--stall-after-ms requires a positive integer")?,
                )
            }
            Some("--help" | "-h") => {
                println!("usage: disksage-onedrive-pressure [--previous ABSOLUTE_JSON --stall-after-ms POSITIVE_INTEGER]");
                return Ok(());
            }
            _ => return Err("unknown or invalid argument".into()),
        }
    }
    if previous.is_some() != stall_after_ms.is_some() {
        return Err("--previous and --stall-after-ms must be supplied together".into());
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or("home-directory-unavailable")?;
    let now = disksage_lib::cloud::system_now_ms();
    let current = collect(&home, now)?;
    let prior: Option<OneDriveInternalPressureObservation> = previous
        .map(|path| {
            if !path.is_absolute() {
                return Err("--previous must be absolute".into());
            }
            let bytes =
                std::fs::read(path).map_err(|_| "previous-observation-unreadable".to_string())?;
            serde_json::from_slice(&bytes).map_err(|_| "previous-observation-invalid".to_string())
        })
        .transpose()?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "observation": current,
            "report": assess(&current, prior.as_ref(), stall_after_ms),
            "mutation_executed": false
        }))
        .map_err(|_| "onedrive-pressure-report-encode-failed")?
    );
    Ok(())
}

#[cfg(not(coverage))]
fn main() {
    if let Err(error) = run() {
        eprintln!("disksage-onedrive-pressure: {error}");
        std::process::exit(2);
    }
}

#[cfg(coverage)]
fn main() {}
