use disksage_lib::{cloud, onedrive_temp_reclaim};
use std::path::PathBuf;

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or("home-unavailable")?;
    let first = args.next();
    let output = if first.as_deref() == Some("--apply") {
        let fingerprint = args.next().ok_or("plan-fingerprint-required")?;
        let approval = args.next().ok_or("approval-phrase-required")?;
        if args.next().is_some() {
            return Err("unexpected-argument".into());
        }
        serde_json::to_value(onedrive_temp_reclaim::execute(
            &home,
            &fingerprint,
            &approval,
            cloud::system_now_ms(),
        )?)
    } else if first.is_none() {
        serde_json::to_value(onedrive_temp_reclaim::plan(&home, cloud::system_now_ms())?)
    } else {
        return Err(
            "usage: disksage-onedrive-temp-reclaim [--apply FINGERPRINT APPROVAL_PHRASE]".into(),
        );
    }
    .map_err(|error| error.to_string())?;
    println!(
        "{}",
        serde_json::to_string_pretty(&output).map_err(|error| error.to_string())?
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("disksage-onedrive-temp-reclaim: {error}");
        std::process::exit(2);
    }
}
