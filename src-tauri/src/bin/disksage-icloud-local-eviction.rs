//! Headless, evidence-bound iCloud local-copy eviction.
//!
//! The default action is a read-only plan. Execution requires the exact previously reviewed plan
//! fingerprint, an attributed human approval, a second matching confirmation, and a local immutable
//! record directory.

#[cfg(not(coverage))]
use disksage_lib::cloud::{self, CloudRoot};
#[cfg(not(coverage))]
use disksage_lib::cloud_local_eviction::{
    approve_icloud_local_eviction, execute_icloud_local_eviction, plan_icloud_local_eviction,
    write_immutable_record, IcloudLocalEvictionApproval, IcloudLocalEvictionPlan,
    IcloudLocalEvictionResult,
};
#[cfg(not(coverage))]
use std::path::{Path, PathBuf};

#[cfg(not(coverage))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    cloud_root: PathBuf,
    path: PathBuf,
    execute: bool,
    approved_plan_fingerprint: Option<String>,
    confirm_plan_fingerprint: Option<String>,
    approved_by: Option<String>,
    rationale: Option<String>,
    record_dir: Option<PathBuf>,
}

#[cfg(not(coverage))]
fn usage() -> &'static str {
    "usage: disksage-icloud-local-eviction --cloud-root ABSOLUTE_PATH --path ABSOLUTE_FILE [--execute --approved-plan-fingerprint HEX64 --confirm-plan-fingerprint HEX64 --approved-by human:IDENTITY --rationale TEXT --record-dir ABSOLUTE_LOCAL_DIRECTORY]"
}

#[cfg(not(coverage))]
fn value(args: &[String], index: &mut usize, flag: &str) -> Result<String, String> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| format!("{flag} 값이 필요함"))
}

#[cfg(not(coverage))]
fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut cloud_root = None;
    let mut path = None;
    let mut execute = false;
    let mut approved_plan_fingerprint = None;
    let mut confirm_plan_fingerprint = None;
    let mut approved_by = None;
    let mut rationale = None;
    let mut record_dir = None;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--cloud-root" => {
                cloud_root = Some(PathBuf::from(value(args, &mut index, "--cloud-root")?))
            }
            "--path" => path = Some(PathBuf::from(value(args, &mut index, "--path")?)),
            "--execute" => execute = true,
            "--approved-plan-fingerprint" => {
                approved_plan_fingerprint =
                    Some(value(args, &mut index, "--approved-plan-fingerprint")?)
            }
            "--confirm-plan-fingerprint" => {
                confirm_plan_fingerprint =
                    Some(value(args, &mut index, "--confirm-plan-fingerprint")?)
            }
            "--approved-by" => approved_by = Some(value(args, &mut index, "--approved-by")?),
            "--rationale" => rationale = Some(value(args, &mut index, "--rationale")?),
            "--record-dir" => {
                record_dir = Some(PathBuf::from(value(args, &mut index, "--record-dir")?))
            }
            "--help" | "-h" => return Err(usage().into()),
            unknown => return Err(format!("알 수 없는 인자: {unknown}")),
        }
        index += 1;
    }
    let cloud_root = cloud_root.ok_or_else(|| "--cloud-root 값이 필요함".to_string())?;
    let path = path.ok_or_else(|| "--path 값이 필요함".to_string())?;
    if !cloud_root.is_absolute() || !path.is_absolute() {
        return Err("--cloud-root와 --path는 절대 경로여야 함".into());
    }
    let execution_fields_present = [
        approved_plan_fingerprint.is_some(),
        confirm_plan_fingerprint.is_some(),
        approved_by.is_some(),
        rationale.is_some(),
        record_dir.is_some(),
    ];
    if execute && execution_fields_present.iter().any(|present| !present) {
        return Err("--execute에는 승인 fingerprint, human attribution, rationale, record-dir가 모두 필요함".into());
    }
    if !execute && execution_fields_present.iter().any(|present| *present) {
        return Err("실행 전용 인자는 --execute와 함께 사용해야 함".into());
    }
    if record_dir
        .as_ref()
        .is_some_and(|directory| !directory.is_absolute())
    {
        return Err("--record-dir는 절대 경로여야 함".into());
    }
    Ok(Args {
        cloud_root,
        path,
        execute,
        approved_plan_fingerprint,
        confirm_plan_fingerprint,
        approved_by,
        rationale,
        record_dir,
    })
}

#[cfg(not(coverage))]
fn home_dir() -> Result<PathBuf, String> {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .map_err(|_| "HOME/USERPROFILE을 찾을 수 없음".into())
}

#[cfg(not(coverage))]
fn select_root<'a>(roots: &'a [CloudRoot], requested: &Path) -> Result<&'a CloudRoot, String> {
    let matches: Vec<_> = roots
        .iter()
        .filter(|root| cloud::cloud_root_path_matches(Path::new(&root.path), requested))
        .collect();
    match matches.as_slice() {
        [only] => Ok(*only),
        [] => Err("요청한 경로가 현재 탐지된 클라우드 루트와 일치하지 않음".into()),
        _ => Err("요청한 경로와 일치하는 클라우드 루트가 여러 개임".into()),
    }
}

#[cfg(not(coverage))]
#[derive(Debug, serde::Serialize)]
struct PlanOutput {
    action: &'static str,
    mutation_executed: bool,
    plan: IcloudLocalEvictionPlan,
}

#[cfg(not(coverage))]
#[derive(Debug, serde::Serialize)]
struct ExecuteOutput {
    action: &'static str,
    mutation_executed: bool,
    plan: IcloudLocalEvictionPlan,
    approval: IcloudLocalEvictionApproval,
    approval_record: String,
    result: IcloudLocalEvictionResult,
    result_record: String,
}

#[cfg(not(coverage))]
fn print_json<T: serde::Serialize>(value: &T) -> Result<(), String> {
    println!(
        "{}",
        serde_json::to_string_pretty(value).map_err(|error| error.to_string())?
    );
    Ok(())
}

#[cfg(not(coverage))]
fn run() -> Result<(), String> {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let args = parse_args(&raw)?;
    let roots = cloud::discover_cloud_roots(&home_dir()?);
    let root = select_root(&roots, &args.cloud_root)?.clone();
    let now_ms = cloud::system_now_ms();
    let plan = plan_icloud_local_eviction(&root, &args.path, now_ms)?;
    if !args.execute {
        return print_json(&PlanOutput {
            action: "plan-icloud-local-eviction",
            mutation_executed: false,
            plan,
        });
    }

    let approved_fingerprint = args
        .approved_plan_fingerprint
        .as_deref()
        .ok_or_else(|| "approved-plan-fingerprint-missing".to_string())?;
    let confirmation = args
        .confirm_plan_fingerprint
        .as_deref()
        .ok_or_else(|| "confirm-plan-fingerprint-missing".to_string())?;
    if approved_fingerprint != confirmation {
        return Err("icloud-local-eviction-double-confirmation-mismatch".into());
    }
    let approved_by = args
        .approved_by
        .as_deref()
        .ok_or_else(|| "approved-by-missing".to_string())?;
    let rationale = args
        .rationale
        .as_deref()
        .ok_or_else(|| "rationale-missing".to_string())?;
    let record_dir = args
        .record_dir
        .as_deref()
        .ok_or_else(|| "record-dir-missing".to_string())?;
    if record_dir.starts_with(Path::new(&root.path)) || args.path.starts_with(record_dir) {
        return Err("icloud-local-eviction-record-dir-overlaps-cloud-data".into());
    }

    let approval =
        approve_icloud_local_eviction(&plan, approved_fingerprint, now_ms, approved_by, rationale)?;
    let approval_record = write_immutable_record(
        record_dir,
        &format!("{}.approval.json", approval.approval_id),
        &approval,
    )?;
    let result = execute_icloud_local_eviction(
        &root,
        &plan,
        &approval,
        confirmation,
        cloud::system_now_ms(),
    )?;
    let result_record = write_immutable_record(
        record_dir,
        &format!("{}.result.json", result.result_id),
        &result,
    )?;
    print_json(&ExecuteOutput {
        action: "evict-icloud-local-copy",
        mutation_executed: true,
        plan,
        approval,
        approval_record: approval_record.to_string_lossy().into_owned(),
        result,
        result_record: result_record.to_string_lossy().into_owned(),
    })
}

#[cfg(not(coverage))]
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(2);
    }
}

#[cfg(coverage)]
fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(windows))]
    const CLOUD_ROOT: &str = "/Cloud";
    #[cfg(not(windows))]
    const CLOUD_FILE: &str = "/Cloud/file.bin";
    #[cfg(not(windows))]
    const RECORD_DIR: &str = "/records";
    #[cfg(windows)]
    const CLOUD_ROOT: &str = "C:\\Cloud";
    #[cfg(windows)]
    const CLOUD_FILE: &str = "C:\\Cloud\\file.bin";
    #[cfg(windows)]
    const RECORD_DIR: &str = "C:\\records";

    #[test]
    fn parser_defaults_to_read_only_plan() {
        let args = parse_args(&[
            "--cloud-root".into(),
            CLOUD_ROOT.into(),
            "--path".into(),
            CLOUD_FILE.into(),
        ])
        .unwrap();
        assert!(!args.execute);
        assert!(args.record_dir.is_none());
    }

    #[test]
    fn parser_requires_complete_execution_confirmation() {
        let base = vec![
            "--cloud-root".into(),
            CLOUD_ROOT.into(),
            "--path".into(),
            CLOUD_FILE.into(),
            "--execute".into(),
        ];
        assert!(parse_args(&base).is_err());
        let mut complete = base;
        complete.extend([
            "--approved-plan-fingerprint".into(),
            "a".repeat(64),
            "--confirm-plan-fingerprint".into(),
            "a".repeat(64),
            "--approved-by".into(),
            "human:test".into(),
            "--rationale".into(),
            "retain cloud object".into(),
            "--record-dir".into(),
            RECORD_DIR.into(),
        ]);
        assert!(parse_args(&complete).unwrap().execute);
    }

    #[test]
    fn parser_rejects_execution_only_fields_on_plan() {
        assert!(parse_args(&[
            "--cloud-root".into(),
            CLOUD_ROOT.into(),
            "--path".into(),
            CLOUD_FILE.into(),
            "--approved-by".into(),
            "human:test".into(),
        ])
        .is_err());
    }
}
