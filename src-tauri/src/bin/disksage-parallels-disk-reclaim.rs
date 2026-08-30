use disksage_lib::parallels_disk_reclaim::{
    approve, enforce_cli_platform, execute, plan, validate_cli_argument_tokens,
    ParallelsDiskReclaimPlan,
};
use std::path::PathBuf;

const USAGE: &str = "usage: disksage-parallels-disk-reclaim --vm-id ID --bundle ABSOLUTE.pvm --disk ABSOLUTE.hdd [--approved-plan ABSOLUTE.json --confirm EXACT_PHRASE --approved-by human:IDENTITY --rationale TEXT --record-dir ABSOLUTE_DIRECTORY]";

fn value(args: &[String], flag: &str) -> Result<String, String> {
    let positions: Vec<_> = args
        .iter()
        .enumerate()
        .filter(|(_, value)| value.as_str() == flag)
        .collect();
    if positions.len() != 1 {
        return Err(format!("{flag}를 한 번 지정하세요."));
    }
    args.get(positions[0].0 + 1)
        .filter(|value| !value.starts_with('-'))
        .cloned()
        .ok_or_else(|| format!("{flag} 값을 지정하세요."))
}

fn output_requires_failure_exit(output: &serde_json::Value) -> bool {
    let result = output.get("result");
    result
        .and_then(|value| value.get("execution_succeeded"))
        .and_then(serde_json::Value::as_bool)
        != Some(true)
        || result
            .and_then(|value| value.get("verification_complete"))
            .and_then(serde_json::Value::as_bool)
            != Some(true)
        || output
            .get("result_record_error")
            .is_some_and(|error| !error.is_null())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args == ["--help"] || args == ["-h"] {
        println!("{USAGE}");
        return;
    }
    let result = (|| {
        enforce_cli_platform()?;
        validate_cli_argument_tokens(&args)?;
        let vm_id = value(&args, "--vm-id")?;
        let bundle = PathBuf::from(value(&args, "--bundle")?);
        let disk = PathBuf::from(value(&args, "--disk")?);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "시스템 시간을 확인하세요.".to_string())?
            .as_millis() as u64;
        let execution_requested = args.iter().any(|argument| argument == "--approved-plan");
        if !execution_requested {
            if args.len() != 6 {
                return Err(USAGE.into());
            }
            let planned = plan(&vm_id, &bundle, &disk, now)?;
            return serde_json::to_value(planned).map_err(|error| error.to_string());
        }
        if args.len() != 16 {
            return Err(USAGE.into());
        }
        let approved_plan_path = PathBuf::from(value(&args, "--approved-plan")?);
        let record_dir = PathBuf::from(value(&args, "--record-dir")?);
        if !approved_plan_path.is_absolute() || !record_dir.is_absolute() {
            return Err("승인 계획과 기록 디렉터리는 절대 경로여야 합니다.".into());
        }
        let metadata = std::fs::symlink_metadata(&approved_plan_path)
            .map_err(|_| "승인 계획 파일을 읽을 수 없습니다.".to_string())?;
        if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 1024 * 1024
        {
            return Err("승인 계획 파일이 안전한 일반 파일이 아닙니다.".into());
        }
        let approved_plan: ParallelsDiskReclaimPlan = serde_json::from_slice(
            &std::fs::read(&approved_plan_path)
                .map_err(|_| "승인 계획 파일을 읽을 수 없습니다.".to_string())?,
        )
        .map_err(|_| "승인 계획 파일 형식이 올바르지 않습니다.".to_string())?;
        let requested_bundle = std::fs::canonicalize(&bundle)
            .map_err(|_| "요청한 VM 경로를 확인할 수 없습니다.".to_string())?;
        let requested_disk = std::fs::canonicalize(&disk)
            .map_err(|_| "요청한 디스크 경로를 확인할 수 없습니다.".to_string())?;
        if approved_plan.vm_id != vm_id
            || PathBuf::from(&approved_plan.bundle_path) != requested_bundle
            || PathBuf::from(&approved_plan.disk_path) != requested_disk
        {
            return Err("승인한 VM과 현재 요청이 일치하지 않습니다.".into());
        }
        let confirmation = value(&args, "--confirm")?;
        let approval = approve(
            &approved_plan,
            &confirmation,
            now,
            &value(&args, "--approved-by")?,
            &value(&args, "--rationale")?,
        )?;
        let approval_path = disksage_lib::cloud_local_eviction::write_immutable_record(
            &record_dir,
            &format!("{}.approval.json", approval.approval_id),
            &approval,
        )?;
        let result = execute(&approved_plan, &approval, &confirmation, now)?;
        let result_record = disksage_lib::cloud_local_eviction::write_immutable_record(
            &record_dir,
            &format!("{}.result.json", result.result_id),
            &result,
        );
        let (result_path, result_record_error) = match result_record {
            Ok(path) => (Some(path.to_string_lossy().into_owned()), None),
            Err(error) => (None, Some(error)),
        };
        Ok(serde_json::json!({
            "action": "compact-approved-parallels-disk",
            "plan": approved_plan,
            "approval": approval,
            "approval_path": approval_path,
            "result": result,
            "result_path": result_path,
            "result_record_error": result_record_error,
        }))
    })();
    match result {
        Ok(output) => {
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
            if output_requires_failure_exit(&output) {
                std::process::exit(2);
            }
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn successful_execution_with_failed_verification_exits_nonzero() {
        let output = serde_json::json!({
            "result": {"execution_succeeded": true, "verification_complete": false},
            "result_record_error": null,
        });
        assert!(output_requires_failure_exit(&output));
    }

    #[test]
    fn successful_execution_with_failed_result_persistence_exits_nonzero() {
        let output = serde_json::json!({
            "result": {"execution_succeeded": true, "verification_complete": true},
            "result_record_error": "result-record-create-failed",
        });
        assert!(output_requires_failure_exit(&output));
    }

    #[test]
    fn fully_verified_and_recorded_execution_keeps_success_exit() {
        let output = serde_json::json!({
            "result": {"execution_succeeded": true, "verification_complete": true},
            "result_record_error": null,
        });
        assert!(!output_requires_failure_exit(&output));
    }
}
