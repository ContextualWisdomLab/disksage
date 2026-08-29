use disksage_lib::generated_cache_reclaim::{
    approve, audit, execute_and_record, remove_regenerable_root,
};
use std::ffi::OsString;
use std::path::PathBuf;

const USAGE: &str = "Usage: disksage-generated-cache-reclaim --root ABSOLUTE_PATH --home ABSOLUTE_PATH [--execute --approved-plan-fingerprint HEX --confirm PHRASE --approved-by ID --rationale TEXT --record PATH]\nWithout --execute, prints a read-only plan. Temporary Git workspaces are audit-only and must use DiskSage's specialized Git or shared-temp executor.";

#[derive(Debug, PartialEq, Eq)]
struct Options {
    root: PathBuf,
    home: PathBuf,
    execute: bool,
    fingerprint: Option<String>,
    confirmation: Option<String>,
    approved_by: Option<String>,
    rationale: Option<String>,
    record: Option<PathBuf>,
}

fn next_value(args: &[OsString], index: &mut usize, name: &str) -> Result<OsString, String> {
    let value = args
        .get(*index)
        .ok_or_else(|| format!("{name}-value-required"))?
        .clone();
    *index += 1;
    Ok(value)
}

fn utf8(value: OsString, code: &str) -> Result<String, String> {
    value.into_string().map_err(|_| code.into())
}

fn parse(args: &[OsString]) -> Result<Option<Options>, String> {
    if args.len() == 1 && matches!(args[0].to_str(), Some("-h" | "--help")) {
        return Ok(None);
    }
    let mut root = None;
    let mut home = None;
    let mut execute = false;
    let mut fingerprint = None;
    let mut confirmation = None;
    let mut approved_by = None;
    let mut rationale = None;
    let mut record = None;
    let mut index = 0;
    while index < args.len() {
        let option = args[index].to_str().ok_or("invalid-utf8-option")?;
        index += 1;
        match option {
            "--root" if root.is_none() => {
                root = Some(PathBuf::from(next_value(args, &mut index, "root")?))
            }
            "--home" if home.is_none() => {
                home = Some(PathBuf::from(next_value(args, &mut index, "home")?))
            }
            "--execute" if !execute => execute = true,
            "--approved-plan-fingerprint" if fingerprint.is_none() => {
                fingerprint = Some(utf8(
                    next_value(args, &mut index, "approved-plan-fingerprint")?,
                    "invalid-fingerprint",
                )?)
            }
            "--confirm" if confirmation.is_none() => {
                confirmation = Some(utf8(
                    next_value(args, &mut index, "confirm")?,
                    "invalid-confirmation",
                )?)
            }
            "--approved-by" if approved_by.is_none() => {
                approved_by = Some(utf8(
                    next_value(args, &mut index, "approved-by")?,
                    "invalid-approved-by",
                )?)
            }
            "--rationale" if rationale.is_none() => {
                rationale = Some(utf8(
                    next_value(args, &mut index, "rationale")?,
                    "invalid-rationale",
                )?)
            }
            "--record" if record.is_none() => {
                record = Some(PathBuf::from(next_value(args, &mut index, "record")?))
            }
            "-h" | "--help" => return Err("help-must-be-used-alone".into()),
            _ => return Err(format!("unknown-or-duplicate-option: {option}")),
        }
    }
    let options = Options {
        root: root.ok_or("root-required")?,
        home: home.ok_or("home-required")?,
        execute,
        fingerprint,
        confirmation,
        approved_by,
        rationale,
        record,
    };
    if !options.root.is_absolute() || !options.home.is_absolute() {
        return Err("absolute-root-and-home-required".into());
    }
    if !options.execute
        && (options.fingerprint.is_some()
            || options.confirmation.is_some()
            || options.approved_by.is_some()
            || options.rationale.is_some()
            || options.record.is_some())
    {
        return Err("execution-authority-option-without-execute".into());
    }
    Ok(Some(options))
}

fn run() -> Result<(), String> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let Some(options) = parse(&args)? else {
        println!("{USAGE}");
        return Ok(());
    };
    let now = disksage_lib::cloud::system_now_ms();
    let plan = audit(&options.root, &options.home, now)?;
    if !options.execute {
        println!(
            "{}",
            serde_json::to_string_pretty(&plan).map_err(|error| error.to_string())?
        );
        return Ok(());
    }
    if options.fingerprint.as_deref() != Some(plan.plan_fingerprint.as_str()) {
        return Err("approved-plan-fingerprint-mismatch".into());
    }
    let approval = approve(
        &plan,
        options
            .confirmation
            .as_deref()
            .ok_or("confirmation-required")?,
        options
            .approved_by
            .as_deref()
            .ok_or("approved-by-required")?,
        options.rationale.as_deref().ok_or("rationale-required")?,
        now,
    )?;
    let fresh = audit(
        &options.root,
        &options.home,
        disksage_lib::cloud::system_now_ms(),
    )?;
    let receipt = execute_and_record(
        &plan,
        &approval,
        &fresh,
        disksage_lib::cloud::system_now_ms(),
        options.record.as_deref().ok_or("record-required")?,
        |path| remove_regenerable_root(path, &options.home),
    )?;
    println!(
        "{}",
        serde_json::to_string_pretty(&receipt).map_err(|error| error.to_string())?
    );
    if !receipt.removed {
        return Err("generated-cache-reclaim-failed".into());
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("disksage-generated-cache-reclaim: {error}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn help_is_standalone_and_plan_is_default() {
        assert_eq!(parse(&strings(&["--help"])).unwrap(), None);
        let options = parse(&strings(&[
            "--root",
            "/Users/test/.cache/torch",
            "--home",
            "/Users/test",
        ]))
        .unwrap()
        .unwrap();
        assert!(!options.execute);
        assert!(options.record.is_none());
    }

    #[test]
    fn authority_requires_execute_and_duplicate_options_fail() {
        assert!(parse(&strings(&[
            "--root",
            "/Users/test/.cache/torch",
            "--home",
            "/Users/test",
            "--confirm",
            "x",
        ]))
        .is_err());
        assert!(parse(&strings(&[
            "--root",
            "/a",
            "--root",
            "/b",
            "--home",
            "/Users/test",
        ]))
        .is_err());
        assert!(parse(&strings(&["--root", "relative", "--home", "/Users/test"])).is_err());
    }
}
