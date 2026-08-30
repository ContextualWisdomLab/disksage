use disksage_lib::generated_cache_reclaim::{
    approve, audit, execute_and_record, stage_and_remove_regenerable_root,
    MAX_APPROVAL_AGE_MS,
};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

const USAGE: &str = "Usage: disksage-generated-cache-reclaim --root ABSOLUTE_PATH [--execute --approved-plan-fingerprint HEX --confirm PHRASE --approved-by ID --rationale TEXT]\nWithout --execute, prints a read-only plan. Receipts use DiskSage's private application-data directory. Temporary Git workspaces are audit-only and must use DiskSage's specialized Git or shared-temp executor.";

#[derive(Debug, PartialEq, Eq)]
struct Options {
    root: PathBuf,
    execute: bool,
    fingerprint: Option<String>,
    confirmation: Option<String>,
    approved_by: Option<String>,
    rationale: Option<String>,
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
    let (mut root, mut fingerprint, mut confirmation, mut approved_by, mut rationale) =
        (None, None, None, None, None);
    let mut execute = false;
    let mut index = 0;
    while index < args.len() {
        let option = args[index].to_str().ok_or("invalid-utf8-option")?;
        index += 1;
        match option {
            "--root" if root.is_none() => {
                root = Some(PathBuf::from(next_value(args, &mut index, "root")?))
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
            "-h" | "--help" => return Err("help-must-be-used-alone".into()),
            _ => return Err(format!("unknown-or-duplicate-option: {option}")),
        }
    }
    let options = Options {
        root: root.ok_or("root-required")?,
        execute,
        fingerprint,
        confirmation,
        approved_by,
        rationale,
    };
    if !options.root.is_absolute() {
        return Err("absolute-root-required".into());
    }
    if !options.execute
        && (options.fingerprint.is_some()
            || options.confirmation.is_some()
            || options.approved_by.is_some()
            || options.rationale.is_some())
    {
        return Err("execution-authority-option-without-execute".into());
    }
    Ok(Some(options))
}

fn fixed_home() -> Result<PathBuf, String> {
    let home = dirs::home_dir()
        .filter(|path| path.is_absolute())
        .ok_or("home-directory-unavailable")?;
    std::fs::canonicalize(home).map_err(|_| "home-directory-unavailable".into())
}

fn fixed_receipt_path(home: &Path, fingerprint: &str, now_ms: u64) -> Result<PathBuf, String> {
    #[cfg(unix)]
    use std::os::unix::fs::DirBuilderExt;
    let directory = home.join(
        "Library/Application Support/com.contextualwisdomlab.disksage/generated-cache-receipts",
    );
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    builder.mode(0o700);
    builder
        .create(&directory)
        .map_err(|_| "generated-cache-receipt-directory-create-failed")?;
    let canonical = std::fs::canonicalize(&directory)
        .map_err(|_| "generated-cache-receipt-directory-unavailable")?;
    if !canonical.starts_with(home)
        || std::fs::symlink_metadata(&directory).map_or(true, |metadata| {
            !metadata.is_dir() || metadata.file_type().is_symlink()
        })
    {
        return Err("generated-cache-receipt-directory-unsafe".into());
    }
    Ok(canonical.join(format!("{now_ms}-{}.jsonl", &fingerprint[..16])))
}

fn run() -> Result<(), String> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let Some(options) = parse(&args)? else {
        println!("{USAGE}");
        return Ok(());
    };
    let home = fixed_home()?;
    let now = disksage_lib::cloud::system_now_ms();
    let plan = audit(&options.root, &home, now)?;
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
    let fresh = audit(&options.root, &home, disksage_lib::cloud::system_now_ms())?;
    let attempted_at_ms = disksage_lib::cloud::system_now_ms();
    let receipt_path = fixed_receipt_path(&home, &plan.plan_fingerprint, attempted_at_ms)?;
    let receipt = execute_and_record(
        &plan,
        &approval,
        &fresh,
        attempted_at_ms,
        &receipt_path,
        |path| {
            stage_and_remove_regenerable_root(
                &plan,
                path,
                &home,
                attempted_at_ms,
                approval
                    .approved_at_ms
                    .saturating_add(MAX_APPROVAL_AGE_MS),
            )
        },
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
        assert!(
            !parse(&strings(&["--root", "/Users/test/.cache/torch"]))
                .unwrap()
                .unwrap()
                .execute
        );
    }
    #[test]
    fn authority_and_paths_are_not_caller_controlled() {
        assert!(parse(&strings(&[
            "--root",
            "/Users/test/.cache/torch",
            "--confirm",
            "x"
        ]))
        .is_err());
        assert!(parse(&strings(&["--root", "/a", "--root", "/b"])).is_err());
        assert!(parse(&strings(&["--root", "relative"])).is_err());
        assert!(parse(&strings(&["--root", "/a", "--home", "/tmp"])).is_err());
        assert!(parse(&strings(&["--root", "/a", "--record", "/tmp/x"])).is_err());
    }
}
