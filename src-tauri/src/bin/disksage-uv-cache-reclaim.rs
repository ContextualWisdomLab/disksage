use disksage_lib::uv_cache_reclaim::{
    execute_uv_cache_reclaim, fixed_uv_path, plan_uv_cache_reclaim,
};
use std::ffi::OsString;
use std::path::PathBuf;

const USAGE: &str = "Usage: disksage-uv-cache-reclaim [--uv-bin PATH] [--execute --approved-plan-fingerprint HEX --confirm PHRASE --approved-by ID --rationale TEXT --record-dir PATH]\nWithout --execute, prints a read-only native uv cache prune plan.";

fn value(args: &[OsString], index: &mut usize, name: &str) -> Result<OsString, String> {
    let value = args
        .get(*index)
        .ok_or_else(|| format!("{name}-value-required"))?
        .clone();
    *index += 1;
    Ok(value)
}

fn run() -> Result<(), String> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.len() == 1 && matches!(args[0].to_str(), Some("-h" | "--help")) {
        println!("{USAGE}");
        return Ok(());
    }
    let mut uv_path = None;
    let mut execute = false;
    let mut fingerprint = None;
    let mut confirmation = None;
    let mut approved_by = None;
    let mut rationale = None;
    let mut record_dir = None;
    let mut index = 0;
    while index < args.len() {
        let option = args[index].to_str().ok_or("invalid-utf8-option")?;
        index += 1;
        match option {
            "--uv-bin" if uv_path.is_none() => {
                uv_path = Some(PathBuf::from(value(&args, &mut index, "uv-bin")?))
            }
            "--execute" if !execute => execute = true,
            "--approved-plan-fingerprint" if fingerprint.is_none() => {
                fingerprint = Some(
                    value(&args, &mut index, "approved-plan-fingerprint")?
                        .into_string()
                        .map_err(|_| "invalid-fingerprint")?,
                )
            }
            "--confirm" if confirmation.is_none() => {
                confirmation = Some(
                    value(&args, &mut index, "confirm")?
                        .into_string()
                        .map_err(|_| "invalid-confirmation")?,
                )
            }
            "--approved-by" if approved_by.is_none() => {
                approved_by = Some(
                    value(&args, &mut index, "approved-by")?
                        .into_string()
                        .map_err(|_| "invalid-approved-by")?,
                )
            }
            "--rationale" if rationale.is_none() => {
                rationale = Some(
                    value(&args, &mut index, "rationale")?
                        .into_string()
                        .map_err(|_| "invalid-rationale")?,
                )
            }
            "--record-dir" if record_dir.is_none() => {
                record_dir = Some(PathBuf::from(value(&args, &mut index, "record-dir")?))
            }
            "-h" | "--help" => return Err("help-must-be-used-alone".into()),
            _ => return Err(format!("unknown-or-duplicate-option: {option}")),
        }
    }
    let uv_path = uv_path.map(Ok).unwrap_or_else(fixed_uv_path)?;
    let now = disksage_lib::cloud::system_now_ms();
    let (output, execution_failed) = if execute {
        let receipt = execute_uv_cache_reclaim(
            &uv_path,
            fingerprint
                .as_deref()
                .ok_or("approved-plan-fingerprint-required")?,
            confirmation.as_deref().ok_or("confirmation-required")?,
            approved_by.as_deref().ok_or("approved-by-required")?,
            rationale.as_deref().ok_or("rationale-required")?,
            record_dir.as_deref().ok_or("record-dir-required")?,
            now,
        )?;
        let execution_failed = receipt.status_code != 0
            || receipt.execution_error.is_some()
            || receipt.output_truncated
            || receipt.capacity_postcheck_error.is_some()
            || receipt.result_record_error.is_some();
        (serde_json::to_value(receipt), execution_failed)
    } else {
        if fingerprint.is_some()
            || confirmation.is_some()
            || approved_by.is_some()
            || rationale.is_some()
            || record_dir.is_some()
        {
            return Err("execution-authority-option-without-execute".into());
        }
        (serde_json::to_value(plan_uv_cache_reclaim(&uv_path, now)?), false)
    };
    let output = output.map_err(|error| error.to_string())?;
    println!(
        "{}",
        serde_json::to_string_pretty(&output).map_err(|error| error.to_string())?
    );
    if execution_failed {
        return Err("uv-cache-reclaim-command-failed".into());
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("disksage-uv-cache-reclaim: {error}");
        std::process::exit(2);
    }
}
