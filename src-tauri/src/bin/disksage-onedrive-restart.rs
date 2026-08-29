//! Explicit, evidence-bound OneDrive quit/reopen/rescan recovery.

#[cfg(not(coverage))]
use std::io::{Read, Write};
#[cfg(not(coverage))]
use std::path::{Path, PathBuf};

#[cfg(not(coverage))]
const MAX_PLAN_BYTES: u64 = 128 * 1024;

#[cfg(not(coverage))]
#[derive(serde::Deserialize)]
struct PlanEnvelope {
    restart_plan: Option<disksage_lib::onedrive_internal_pressure::OneDriveRestartPlan>,
}

#[cfg(not(coverage))]
struct Args {
    plan: PathBuf,
    confirmation: String,
    reviewed_by: String,
    rationale: String,
    record: PathBuf,
}

#[cfg(not(coverage))]
fn value(args: &[String], index: &mut usize, flag: &str) -> Result<String, String> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| format!("{flag} requires a value"))
}

#[cfg(not(coverage))]
fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut plan = None;
    let mut confirmation = None;
    let mut reviewed_by = None;
    let mut rationale = None;
    let mut record = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--plan" if plan.is_none() => {
                plan = Some(PathBuf::from(value(args, &mut index, "--plan")?))
            }
            "--confirm" if confirmation.is_none() => {
                confirmation = Some(value(args, &mut index, "--confirm")?)
            }
            "--reviewed-by" if reviewed_by.is_none() => {
                reviewed_by = Some(value(args, &mut index, "--reviewed-by")?)
            }
            "--rationale" if rationale.is_none() => {
                rationale = Some(value(args, &mut index, "--rationale")?)
            }
            "--record" if record.is_none() => {
                record = Some(PathBuf::from(value(args, &mut index, "--record")?))
            }
            "--plan" | "--confirm" | "--reviewed-by" | "--rationale" | "--record" => {
                return Err("onedrive-restart-duplicate-argument".into())
            }
            "--help" | "-h" => return Err("usage: disksage-onedrive-restart --plan ABSOLUTE_PRESSURE_JSON --confirm EXACT_PHRASE --reviewed-by PERSON --rationale REASON --record ABSOLUTE_NEW_JSON".into()),
            _ => return Err("onedrive-restart-unknown-argument".into()),
        }
        index += 1;
    }
    let parsed = Args {
        plan: plan.ok_or("onedrive-restart-plan-required")?,
        confirmation: confirmation.ok_or("onedrive-restart-confirmation-required")?,
        reviewed_by: reviewed_by.ok_or("onedrive-restart-reviewer-required")?,
        rationale: rationale.ok_or("onedrive-restart-rationale-required")?,
        record: record.ok_or("onedrive-restart-record-required")?,
    };
    if !parsed.plan.is_absolute() || !parsed.record.is_absolute() {
        return Err("onedrive-restart-control-path-must-be-absolute".into());
    }
    Ok(parsed)
}

#[cfg(not(coverage))]
fn read_plan(
    path: &Path,
) -> Result<disksage_lib::onedrive_internal_pressure::OneDriveRestartPlan, String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| "onedrive-restart-plan-unreadable".to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > MAX_PLAN_BYTES {
        return Err("onedrive-restart-plan-invalid".into());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|_| "onedrive-restart-plan-unreadable".to_string())?
        .take(MAX_PLAN_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "onedrive-restart-plan-unreadable".to_string())?;
    let envelope: PlanEnvelope =
        serde_json::from_slice(&bytes).map_err(|_| "onedrive-restart-plan-invalid".to_string())?;
    envelope
        .restart_plan
        .ok_or_else(|| "onedrive-restart-plan-not-ready".into())
}

#[cfg(not(coverage))]
fn write_receipt(
    path: &Path,
    receipt: &disksage_lib::onedrive_internal_pressure::OneDriveRestartReceipt,
) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(receipt)
        .map_err(|_| "onedrive-restart-receipt-encode-failed".to_string())?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|_| "onedrive-restart-receipt-create-failed".to_string())?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| "onedrive-restart-receipt-write-failed".to_string())
}

#[cfg(all(not(coverage), target_os = "macos"))]
fn run(args: &[String]) -> Result<(), String> {
    use disksage_lib::onedrive_internal_pressure as pressure;
    let args = parse_args(args)?;
    let plan = read_plan(&args.plan)?;
    let now = disksage_lib::cloud::system_now_ms();
    let approval = pressure::approve_restart(
        &plan,
        &args.confirmation,
        &args.reviewed_by,
        &args.rationale,
        now,
    )?;
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or("home-directory-unavailable")?;
    let fresh = pressure::collect(&home, disksage_lib::cloud::system_now_ms())?;
    let identity = disksage_lib::provider_recovery::fixed_onedrive_executable_identity()?;
    let receipt = pressure::execute_restart_with(
        &plan,
        &approval,
        fresh,
        identity,
        disksage_lib::cloud::system_now_ms(),
        || {
            let recovery = disksage_lib::provider_recovery::restart_fixed_onedrive(
                &plan.executable_identity,
                disksage_lib::cloud::system_now_ms(),
            );
            let after = pressure::collect(&home, disksage_lib::cloud::system_now_ms()).ok();
            (recovery, after)
        },
    )?;
    write_receipt(&args.record, &receipt)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&receipt)
            .map_err(|_| "onedrive-restart-receipt-encode-failed")?
    );
    Ok(())
}

#[cfg(any(coverage, not(target_os = "macos")))]
fn run(_args: &[String]) -> Result<(), String> {
    Err("onedrive-restart-platform-unsupported".into())
}

#[cfg(not(coverage))]
fn main() {
    if let Err(error) = run(&std::env::args().skip(1).collect::<Vec<_>>()) {
        eprintln!("{error}");
        std::process::exit(2);
    }
}

#[cfg(coverage)]
fn main() {}

#[cfg(all(test, not(coverage)))]
mod tests {
    use super::*;

    #[test]
    fn exact_approval_and_absolute_records_are_required() {
        assert!(parse_args(&[]).is_err());
        assert!(parse_args(&[
            "--plan".into(),
            "relative.json".into(),
            "--confirm".into(),
            "exact".into(),
            "--reviewed-by".into(),
            "human:local".into(),
            "--rationale".into(),
            "restart stalled sync".into(),
            "--record".into(),
            "/tmp/result.json".into(),
        ])
        .is_err());

        let duplicate = vec![
            "--plan".into(),
            "/tmp/plan-a.json".into(),
            "--plan".into(),
            "/tmp/plan-b.json".into(),
        ];
        assert_eq!(
            parse_args(&duplicate).err().as_deref(),
            Some("onedrive-restart-duplicate-argument")
        );
    }
}
