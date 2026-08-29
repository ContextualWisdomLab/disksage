//! Headless planner/executor for exact unreferenced OpenCode tool-output artifacts.

use disksage_lib::opencode_artifact_reclaim;
use std::path::PathBuf;

const USAGE: &str = "usage: disksage-opencode-artifact-reclaim [--home ABSOLUTE_PATH] [--execute|--purge-quarantined --plan-fingerprint HEX64 --confirm EXACT_PHRASE --approved-by HUMAN_ID --rationale TEXT --journal-path ABSOLUTE_PATH --record-directory ABSOLUTE_PATH]";

#[derive(Debug)]
struct Args {
    home: PathBuf,
    execute: bool,
    purge_quarantined: bool,
    fingerprint: Option<String>,
    confirmation: Option<String>,
    approved_by: Option<String>,
    rationale: Option<String>,
    journal: Option<PathBuf>,
    records: Option<PathBuf>,
}

fn parse(raw: &[String]) -> Result<Args, String> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "HOME unavailable".to_string())?;
    let mut args = Args {
        home,
        execute: false,
        purge_quarantined: false,
        fingerprint: None,
        confirmation: None,
        approved_by: None,
        rationale: None,
        journal: None,
        records: None,
    };
    let mut index = 0;
    while index < raw.len() {
        let flag = &raw[index];
        if flag == "--execute" {
            args.execute = true;
            index += 1;
            continue;
        }
        if flag == "--purge-quarantined" {
            args.purge_quarantined = true;
            index += 1;
            continue;
        }
        index += 1;
        let value = raw
            .get(index)
            .ok_or_else(|| format!("{flag} requires a value"))?;
        match flag.as_str() {
            "--home" => args.home = PathBuf::from(value),
            "--plan-fingerprint" => args.fingerprint = Some(value.clone()),
            "--confirm" => args.confirmation = Some(value.clone()),
            "--approved-by" => args.approved_by = Some(value.clone()),
            "--rationale" => args.rationale = Some(value.clone()),
            "--journal-path" => args.journal = Some(PathBuf::from(value)),
            "--record-directory" => args.records = Some(PathBuf::from(value)),
            _ => return Err(format!("unknown option: {flag}")),
        }
        index += 1;
    }
    if !args.home.is_absolute() {
        return Err("--home must be absolute".into());
    }
    let execution_input_missing = args.fingerprint.is_none()
        || args.confirmation.is_none()
        || args.approved_by.is_none()
        || args.rationale.is_none()
        || args.journal.is_none()
        || args.records.is_none();
    if args.execute && args.purge_quarantined {
        return Err("choose exactly one mutation operation".into());
    }
    if (args.execute || args.purge_quarantined) && execution_input_missing {
        return Err("execution requires fingerprint, confirmation, attribution, rationale, journal, and record directory".into());
    }
    Ok(args)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

fn run() -> Result<(), String> {
    let args = parse(&std::env::args().skip(1).collect::<Vec<_>>())?;
    let timestamp = now_ms();
    if !args.execute && !args.purge_quarantined {
        let plan = opencode_artifact_reclaim::plan(&args.home, timestamp)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&plan)
                .map_err(|_| "plan serialization failed".to_string())?
        );
        return Ok(());
    }
    let journal = args
        .journal
        .as_deref()
        .ok_or_else(|| "execution journal missing".to_string())?;
    let records = args
        .records
        .as_deref()
        .ok_or_else(|| "execution record directory missing".to_string())?;
    let operation = if args.purge_quarantined {
        opencode_artifact_reclaim::purge_quarantined
    } else {
        opencode_artifact_reclaim::execute
    };
    let receipt = operation(&args.home, args.fingerprint.as_deref().unwrap_or_default(), args.confirmation.as_deref().unwrap_or_default(), args.approved_by.as_deref().unwrap_or_default(), args.rationale.as_deref().unwrap_or_default(), journal, records, timestamp)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&receipt)
            .map_err(|_| "receipt serialization failed".to_string())?
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("disksage-opencode-artifact-reclaim: {error}\n{USAGE}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_contract_is_all_or_nothing() {
        let error = parse(&["--execute".into()]).unwrap_err();
        assert!(error.contains("execution requires"));
    }

    #[test]
    fn explicit_home_must_match_process_home() {
        let process_home = std::env::var_os("HOME").expect("test process HOME");
        let alternate_home = tempfile::tempdir().unwrap();
        assert_ne!(alternate_home.path(), PathBuf::from(process_home));
        let error = parse(&[
            "--home".into(),
            alternate_home.path().to_string_lossy().into_owned(),
        ])
        .unwrap_err();
        assert_eq!(error, "--home must match process HOME");
    }
}
