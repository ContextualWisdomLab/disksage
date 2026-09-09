use disksage_lib::{cloud, onedrive_temp_reclaim};
use std::path::PathBuf;

const USAGE: &str = "usage: disksage-onedrive-temp-reclaim [--apply FINGERPRINT APPROVAL_PHRASE]";

fn execution_failed(execution: &onedrive_temp_reclaim::OneDriveTempExecution) -> bool {
    execution.failure.is_some() || !execution.verification_complete
}

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let first = args.next();
    if matches!(first.as_deref(), Some("--help" | "-h")) {
        if args.next().is_some() {
            return Err(USAGE.into());
        }
        println!("{USAGE}");
        return Ok(());
    }

    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or("home-unavailable")?;
    let (output, execution_failure) = if first.as_deref() == Some("--apply") {
        let fingerprint = args.next().ok_or("plan-fingerprint-required")?;
        let approval = args.next().ok_or("approval-phrase-required")?;
        if args.next().is_some() {
            return Err("unexpected-argument".into());
        }
        let execution = onedrive_temp_reclaim::execute(
            &home,
            &fingerprint,
            &approval,
            cloud::system_now_ms(),
        )?;
        let execution_failure = execution_failed(&execution);
        (
            serde_json::to_value(execution).map_err(|error| error.to_string())?,
            execution_failure,
        )
    } else if first.is_none() {
        (
            serde_json::to_value(onedrive_temp_reclaim::plan(&home, cloud::system_now_ms())?)
                .map_err(|error| error.to_string())?,
            false,
        )
    } else {
        return Err(USAGE.into());
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&output).map_err(|error| error.to_string())?
    );
    if execution_failure {
        return Err("onedrive-temp-partial-execution".into());
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("disksage-onedrive-temp-reclaim: {error}");
        std::process::exit(2);
    }
}
