use disksage_lib::incomplete_download::{
    collect_incomplete_download_audit, DEFAULT_MAX_ENTRIES, DEFAULT_STALE_AFTER_DAYS,
    MAX_STALE_AFTER_DAYS,
};
use disksage_lib::incomplete_download_recovery::{
    summarize_incomplete_download_recovery, validate_incomplete_download_recovery,
    RecoveryValidationLimits,
};
use disksage_lib::private_evidence::write_private_json_create_new;
use std::ffi::{OsStr, OsString};
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    root: PathBuf,
    max_entries: usize,
    stale_after_days: u64,
    private_output: Option<PathBuf>,
}

fn absolute_without_parent(path: &Path) -> bool {
    path.is_absolute()
        && !path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
}

fn usage() -> String {
    format!(
        "usage: disksage-incomplete-download-recovery --root ABSOLUTE_PATH \
         [--max-entries 1..={DEFAULT_MAX_ENTRIES}] \
         [--stale-after-days 1..={MAX_STALE_AFTER_DAYS}] \
         [--private-output ABSOLUTE_NEW_FILE.json]\n\
         다음 단계: 생성된 복구 계획을 검토하세요. 이 명령은 파일을 이동하거나 삭제하지 않습니다."
    )
}

fn next_value(raw: &[OsString], index: &mut usize, flag: &str) -> Result<OsString, String> {
    *index += 1;
    raw.get(*index)
        .cloned()
        .ok_or_else(|| format!("{flag} 값이 필요함"))
}

fn next_text_value(raw: &[OsString], index: &mut usize, flag: &str) -> Result<String, String> {
    next_value(raw, index, flag)?
        .into_string()
        .map_err(|_| format!("{flag} 값은 UTF-8 텍스트여야 함"))
}

fn parse_args(raw: &[OsString]) -> Result<Args, String> {
    let mut root = None;
    let mut max_entries = DEFAULT_MAX_ENTRIES;
    let mut max_entries_seen = false;
    let mut stale_after_days = DEFAULT_STALE_AFTER_DAYS;
    let mut stale_after_days_seen = false;
    let mut private_output = None;
    let mut index = 0usize;
    while index < raw.len() {
        let option = raw[index]
            .to_str()
            .ok_or_else(|| "incomplete-download-recovery-unknown-argument".to_string())?;
        match option {
            "--root" => {
                if root.is_some() {
                    return Err("--root는 한 번만 지정할 수 있음".into());
                }
                root = Some(PathBuf::from(next_value(raw, &mut index, "--root")?));
            }
            "--max-entries" => {
                if max_entries_seen {
                    return Err("--max-entries는 한 번만 지정할 수 있음".into());
                }
                max_entries_seen = true;
                let parsed = next_text_value(raw, &mut index, "--max-entries")?
                    .parse::<usize>()
                    .map_err(|_| "--max-entries는 양의 정수여야 함".to_string())?;
                if parsed == 0 || parsed > DEFAULT_MAX_ENTRIES {
                    return Err(format!(
                        "--max-entries는 1..={DEFAULT_MAX_ENTRIES} 범위여야 함"
                    ));
                }
                max_entries = parsed;
            }
            "--stale-after-days" => {
                if stale_after_days_seen {
                    return Err("--stale-after-days는 한 번만 지정할 수 있음".into());
                }
                stale_after_days_seen = true;
                let parsed = next_text_value(raw, &mut index, "--stale-after-days")?
                    .parse::<u64>()
                    .map_err(|_| "--stale-after-days는 양의 정수여야 함".to_string())?;
                if !(1..=MAX_STALE_AFTER_DAYS).contains(&parsed) {
                    return Err(format!(
                        "--stale-after-days는 1..={MAX_STALE_AFTER_DAYS} 범위여야 함"
                    ));
                }
                stale_after_days = parsed;
            }
            "--private-output" => {
                if private_output.is_some() {
                    return Err("--private-output은 한 번만 지정할 수 있음".into());
                }
                private_output = Some(PathBuf::from(next_value(
                    raw,
                    &mut index,
                    "--private-output",
                )?));
            }
            _unknown => return Err("incomplete-download-recovery-unknown-argument".into()),
        }
        index += 1;
    }
    let root = root.ok_or_else(|| "--root가 필요함".to_string())?;
    if !absolute_without_parent(&root) {
        return Err("--root는 상위 탐색이 없는 절대 경로여야 함".into());
    }
    if let Some(path) = &private_output {
        if !absolute_without_parent(path) {
            return Err("--private-output은 상위 탐색이 없는 절대 경로여야 함".into());
        }
    }
    Ok(Args {
        root,
        max_entries,
        stale_after_days,
        private_output,
    })
}

fn system_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn run() -> Result<(), String> {
    let raw = std::env::args_os().skip(1).collect::<Vec<_>>();
    if raw.len() == 1
        && matches!(
            raw.first().map(OsString::as_os_str),
            Some(argument) if argument == OsStr::new("--help") || argument == OsStr::new("-h")
        )
    {
        println!("{}", usage());
        return Ok(());
    }
    let args = parse_args(&raw)?;
    let audit = collect_incomplete_download_audit(
        &args.root,
        system_now_ms(),
        args.max_entries,
        args.stale_after_days,
    )?;
    let report = validate_incomplete_download_recovery(
        &args.root,
        &audit,
        system_now_ms(),
        RecoveryValidationLimits::default(),
    )?;
    let mut summary = serde_json::to_value(summarize_incomplete_download_recovery(&report))
        .map_err(|error| error.to_string())?;
    if let Some(path) = &args.private_output {
        let receipt = write_private_json_create_new(&args.root, path, &report)?;
        summary
            .as_object_mut()
            .ok_or_else(|| "incomplete download recovery summary JSON object가 아님".to_string())?
            .insert(
                "private_output".into(),
                serde_json::to_value(receipt).map_err(|error| error.to_string())?,
            );
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&summary).map_err(|error| error.to_string())?
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("DiskSage incomplete download recovery validation: {error}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bounded_absolute_arguments() {
        let args = parse_args(&[
            "--root".into(),
            "/source".into(),
            "--max-entries".into(),
            "42".into(),
            "--stale-after-days".into(),
            "60".into(),
            "--private-output".into(),
            "/private/recovery.json".into(),
        ])
        .unwrap();
        assert_eq!(args.root, PathBuf::from("/source"));
        assert_eq!(args.max_entries, 42);
        assert_eq!(args.stale_after_days, 60);
        assert_eq!(
            args.private_output,
            Some(PathBuf::from("/private/recovery.json"))
        );
    }

    #[test]
    fn rejects_relative_duplicate_missing_and_unbounded_arguments() {
        for raw in [
            vec![],
            vec!["--root".into(), "relative".into()],
            vec!["--root".into(), "/a/../b".into()],
            vec![
                "--max-entries".into(),
                "0".into(),
                "--root".into(),
                "/a".into(),
            ],
            vec![
                "--stale-after-days".into(),
                "0".into(),
                "--root".into(),
                "/a".into(),
            ],
            vec!["--root".into(), "/a".into(), "--root".into(), "/b".into()],
            vec!["--wat".into()],
        ] {
            assert!(parse_args(&raw).is_err(), "{raw:?}");
        }
    }
}
