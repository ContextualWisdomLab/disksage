use disksage_lib::{cloud, transparent_compression};
use std::path::PathBuf;

fn usage() -> String {
    "usage: disksage-transparent-compress --root ABSOLUTE_PATH [--minimum-age-days N] [--max-files N] [--apply --plan-fingerprint HEX --confirmation-phrase PHRASE --rationale TEXT]".into()
}

fn value(args: &[String], index: &mut usize) -> Result<String, String> {
    *index += 1;
    args.get(*index).cloned().ok_or_else(usage)
}

fn run() -> Result<(), String> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let mut root = None;
    let mut minimum_age_days = 30;
    let mut max_files = 10_000;
    let mut apply = false;
    let mut fingerprint = None;
    let mut phrase = None;
    let mut rationale = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--root" => root = Some(PathBuf::from(value(&args, &mut index)?)),
            "--minimum-age-days" => {
                minimum_age_days = value(&args, &mut index)?.parse().map_err(|_| usage())?
            }
            "--max-files" => max_files = value(&args, &mut index)?.parse().map_err(|_| usage())?,
            "--apply" => apply = true,
            "--plan-fingerprint" => fingerprint = Some(value(&args, &mut index)?),
            "--confirmation-phrase" => phrase = Some(value(&args, &mut index)?),
            "--rationale" => rationale = Some(value(&args, &mut index)?),
            _ => return Err(usage()),
        }
        index += 1;
    }
    let root = root.ok_or_else(usage)?;
    let now_ms = cloud::system_now_ms();
    let plan = transparent_compression::plan(&root, minimum_age_days, max_files, now_ms)?;
    let output = if apply {
        serde_json::to_value(transparent_compression::execute(
            &plan,
            fingerprint.as_deref().ok_or_else(usage)?,
            phrase.as_deref().ok_or_else(usage)?,
            rationale.as_deref().ok_or_else(usage)?,
            now_ms,
        )?)
    } else {
        serde_json::to_value(plan)
    }
    .map_err(|error| error.to_string())?;
    println!(
        "{}",
        serde_json::to_string_pretty(&output).map_err(|error| error.to_string())?
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("disksage-transparent-compress: {error}");
        std::process::exit(2);
    }
}
