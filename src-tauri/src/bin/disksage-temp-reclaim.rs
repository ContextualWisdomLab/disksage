use disksage_lib::temp_reclaim::{plan_temp_reclaim, remove_temp_candidates, TempReclaimOptions};
use std::path::Path;

#[cfg(target_os = "macos")]
const ROOT: &str = "/private/tmp";
#[cfg(not(target_os = "macos"))]
const ROOT: &str = "/tmp";

fn usage() -> &'static str {
    "usage: disksage-temp-reclaim [--execute FINGERPRINT PHRASE RATIONALE]"
}

fn parse_args(args: &[String]) -> Result<Option<(&str, &str, &str)>, String> {
    match args {
        [] => Ok(None),
        [flag] if flag == "--help" || flag == "-h" => Err(usage().into()),
        [flag, fingerprint, phrase, rationale] if flag == "--execute" => {
            Ok(Some((fingerprint, phrase, rationale)))
        }
        _ => Err(usage().into()),
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let options = TempReclaimOptions::default();
    let now = now_ms();
    let output = match parse_args(&args)? {
        None => serde_json::to_value(plan_temp_reclaim(Path::new(ROOT), options, now)?)
            .map_err(|_| "temp-reclaim-json-failed".to_string())?,
        Some((fingerprint, phrase, rationale)) => serde_json::to_value(remove_temp_candidates(
            Path::new(ROOT),
            options,
            fingerprint,
            phrase,
            rationale,
            now,
        )?)
        .map_err(|_| "temp-reclaim-json-failed".to_string())?,
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&output).map_err(|_| "temp-reclaim-json-failed".to_string())?
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_is_plan_only_unless_all_execution_authority_is_present() {
        assert_eq!(parse_args(&[]).unwrap(), None);
        assert!(parse_args(&["--execute".into(), "fingerprint".into()]).is_err());
        assert!(parse_args(&[
            "--execute".into(),
            "fingerprint".into(),
            "phrase".into(),
            "rationale".into(),
        ])
        .unwrap()
        .is_some());
    }
}
