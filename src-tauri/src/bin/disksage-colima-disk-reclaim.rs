use disksage_lib::colima_disk_reclaim::{
    execute_unavailable, plan_with_runner, ColimaDiskReclaimApproval, NativeColimaRunner,
};
use std::path::{Path, PathBuf};

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
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let mut profile = "default".to_string();
    let mut execute = false;
    let mut confirm = None;
    let mut approved_by = None;
    let mut rationale = None;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--profile" | "--confirm-plan-fingerprint" | "--approved-by" | "--rationale" => {
                let flag = args[index].clone();
                index += 1;
                let value = args.get(index).cloned().unwrap_or_default();
                match flag.as_str() {
                    "--profile" => profile = value,
                    "--confirm-plan-fingerprint" => confirm = Some(value),
                    "--approved-by" => approved_by = Some(value),
                    _ => rationale = Some(value),
                }
            }
            "--execute" => execute = true,
            _ => {
                eprintln!("지원하지 않는 옵션입니다. --profile로 다시 실행하세요.");
                std::process::exit(2);
            }
        }
        index += 1;
    }
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_millis() as u64)
        .unwrap_or(0);
    let result = fixed_colima_bin()
        .and_then(|bin| {
            colima_home().and_then(|home| {
                plan_with_runner(&NativeColimaRunner, &bin, &home, &profile, now_ms)
            })
        })
        .and_then(|plan| {
            if !execute {
                return serde_json::to_value(plan)
                    .map_err(|_| "검사 결과를 표시할 수 없습니다.".into());
            }
            let fingerprint = confirm.ok_or_else(|| {
                "검사 결과의 fingerprint를 확인한 뒤 --confirm-plan-fingerprint를 입력하세요."
                    .to_string()
            })?;
            let approval = ColimaDiskReclaimApproval {
                plan_fingerprint: fingerprint,
                exact_approval_phrase: plan.exact_approval_phrase.clone(),
                approved_at_ms: now_ms,
                approved_by: approved_by
                    .ok_or_else(|| "--approved-by human:이름을 입력하세요.".to_string())?,
                rationale: rationale
                    .ok_or_else(|| "검토한 근거를 --rationale에 입력하세요.".to_string())?,
            };
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
