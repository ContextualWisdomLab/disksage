use disksage_lib::colima_disk_reclaim::{
    execute_unavailable, plan_with_runner, ColimaDiskReclaimApproval, NativeColimaRunner,
};
use std::path::{Path, PathBuf};

#[derive(Debug, Default, PartialEq, Eq)]
struct CliArgs {
    profile: String,
    execute: bool,
    confirm_plan_fingerprint: Option<String>,
    confirm_exact_approval_phrase: Option<String>,
    approved_by: Option<String>,
    rationale: Option<String>,
}

fn parse_args(args: &[String]) -> Result<CliArgs, String> {
    let mut parsed = CliArgs {
        profile: "default".into(),
        ..CliArgs::default()
    };
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--profile"
            | "--confirm-plan-fingerprint"
            | "--confirm-exact-approval-phrase"
            | "--approved-by"
            | "--rationale" => {
                let flag = args[index].as_str();
                index += 1;
                let value = args
                    .get(index)
                    .cloned()
                    .ok_or_else(|| format!("{flag} 값을 직접 입력하세요."))?;
                match flag {
                    "--profile" => parsed.profile = value,
                    "--confirm-plan-fingerprint" => parsed.confirm_plan_fingerprint = Some(value),
                    "--confirm-exact-approval-phrase" => {
                        parsed.confirm_exact_approval_phrase = Some(value)
                    }
                    "--approved-by" => parsed.approved_by = Some(value),
                    _ => parsed.rationale = Some(value),
                }
            }
            "--execute" => parsed.execute = true,
            _ => return Err("지원하지 않는 옵션입니다. --profile로 다시 실행하세요.".into()),
        }
        index += 1;
    }
    Ok(parsed)
}

fn approval_from_input(
    args: &CliArgs,
    plan_fingerprint: String,
    approved_at_ms: u64,
) -> Result<ColimaDiskReclaimApproval, String> {
    Ok(ColimaDiskReclaimApproval {
        plan_fingerprint,
        exact_approval_phrase: args.confirm_exact_approval_phrase.clone().ok_or_else(|| {
            "검사 결과에 표시된 승인 문구를 직접 입력한 뒤 --confirm-exact-approval-phrase로 다시 승인하세요."
                .to_string()
        })?,
        approved_at_ms,
        approved_by: args
            .approved_by
            .clone()
            .ok_or_else(|| "유효한 로컬 사용자 이름을 확인한 뒤 --approved-by human:이름으로 다시 승인하세요.".to_string())?,
        rationale: args
            .rationale
            .clone()
            .ok_or_else(|| "검토한 근거를 --rationale에 입력하세요.".to_string())?,
    })
}

fn fixed_colima_bin() -> Result<PathBuf, String> {
    ["/opt/homebrew/bin/colima", "/usr/local/bin/colima"]
        .into_iter()
        .map(PathBuf::from)
        .filter_map(|path| std::fs::canonicalize(path).ok())
        .find(|path| std::fs::metadata(path).is_ok_and(|meta| meta.is_file()))
        .ok_or_else(|| "Colima를 찾을 수 없습니다. Colima 설치 상태를 확인하세요.".into())
}

fn colima_home() -> Result<PathBuf, String> {
    if let Some(value) = std::env::var_os("COLIMA_HOME") {
        let path = PathBuf::from(value);
        return path
            .is_absolute()
            .then_some(path)
            .ok_or_else(|| "COLIMA_HOME은 절대 경로여야 합니다.".into());
    }
    let home =
        std::env::var_os("HOME").ok_or_else(|| "홈 폴더를 확인할 수 없습니다.".to_string())?;
    let legacy = Path::new(&home).join(".colima");
    if legacy.is_dir() {
        return Ok(legacy);
    }
    if let Some(value) = std::env::var_os("XDG_CONFIG_HOME") {
        let xdg = PathBuf::from(value);
        if !xdg.is_absolute() {
            return Err("XDG_CONFIG_HOME은 절대 경로여야 합니다.".into());
        }
        return Ok(xdg.join("colima"));
    }
    let xdg_default = Path::new(&home).join(".config/colima");
    if xdg_default.is_dir() {
        return Ok(xdg_default);
    }
    Ok(legacy)
}

fn main() {
    let args = match parse_args(&std::env::args().skip(1).collect::<Vec<_>>()) {
        Ok(args) => args,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_millis() as u64)
        .unwrap_or(0);
    let result = fixed_colima_bin()
        .and_then(|bin| {
            colima_home().and_then(|home| {
                plan_with_runner(&NativeColimaRunner, &bin, &home, &args.profile, now_ms)
            })
        })
        .and_then(|plan| {
            if !args.execute {
                return serde_json::to_value(plan)
                    .map_err(|_| "검사 결과를 표시할 수 없습니다.".into());
            }
            let fingerprint = args.confirm_plan_fingerprint.clone().ok_or_else(|| {
                "검사 결과의 fingerprint를 확인한 뒤 --confirm-plan-fingerprint를 입력하세요."
                    .to_string()
            })?;
            let approval = approval_from_input(&args, fingerprint, now_ms)?;
            execute_unavailable(&plan, &approval, now_ms).and_then(|receipt| {
                serde_json::to_value(receipt).map_err(|_| "실행 결과를 표시할 수 없습니다.".into())
            })
        });
    match result.and_then(|value| {
        serde_json::to_string_pretty(&value).map_err(|_| "검사 결과를 표시할 수 없습니다.".into())
    }) {
        Ok(json) => println!("{json}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_approval_requires_operator_typed_exact_phrase() {
        let fingerprint = "a".repeat(64);
        let args = parse_args(&[
            "--execute".into(),
            "--confirm-plan-fingerprint".into(),
            fingerprint.clone(),
            "--approved-by".into(),
            "human:local-test-user".into(),
            "--rationale".into(),
            "reviewed stopped profile evidence".into(),
        ])
        .unwrap();
        assert!(approval_from_input(&args, fingerprint.clone(), 100)
            .unwrap_err()
            .contains("승인 문구를 직접 입력"));

        let typed_phrase = format!("DiskSage Colima 디스크 회수 승인 {fingerprint}");
        let typed_args = parse_args(&[
            "--execute".into(),
            "--confirm-plan-fingerprint".into(),
            fingerprint.clone(),
            "--confirm-exact-approval-phrase".into(),
            typed_phrase.clone(),
            "--approved-by".into(),
            "human:local-test-user".into(),
            "--rationale".into(),
            "reviewed stopped profile evidence".into(),
        ])
        .unwrap();
        let approval = approval_from_input(&typed_args, fingerprint, 100).unwrap();
        assert_eq!(approval.exact_approval_phrase, typed_phrase);
    }
}
