use disksage_lib::temp_reclaim::{plan_temp_reclaim, TempReclaimOptions};
use std::path::Path;

#[cfg(target_os = "macos")]
const ROOT: &str = "/private/tmp";
#[cfg(not(target_os = "macos"))]
const ROOT: &str = "/tmp";
const REMOVAL_UNAVAILABLE: &str = "temp-reclaim-removal-private-approval-unavailable";

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
    if parse_args(&args)?.is_some() {
        return Err(REMOVAL_UNAVAILABLE.into());
    }
    let options = TempReclaimOptions::default();
    let now = now_ms();
    let output = serde_json::to_value(plan_temp_reclaim(Path::new(ROOT), options, now)?)
        .map_err(|_| "temp-reclaim-json-failed".to_string())?;
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
    use std::path::PathBuf;

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

    #[test]
    fn windows_uses_the_operating_system_temp_root_instead_of_unix_tmp() {
        let windows_temp = PathBuf::from(r"C:\Users\tester\AppData\Local\Temp");
        assert_eq!(
            default_temp_root("windows", windows_temp.clone()),
            windows_temp
        );
        assert_ne!(
            default_temp_root("windows", PathBuf::from(r"D:\Temp")),
            PathBuf::from("/tmp")
        );
    }
}
