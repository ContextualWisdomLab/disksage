use disksage_lib::temp_reclaim::{plan_temp_reclaim, TempReclaimOptions};
use std::path::PathBuf;

fn usage() -> &'static str {
    "usage: disksage-temp-reclaim"
}

fn parse_args(args: &[String]) -> Result<(), String> {
    match args {
        [] => Ok(()),
        [flag] if flag == "--help" || flag == "-h" => Err(usage().into()),
        _ => Err(usage().into()),
    }
}

fn default_temp_root(platform: &str, environment_temp: PathBuf) -> PathBuf {
    match platform {
        "windows" => environment_temp,
        "macos" => PathBuf::from("/private/tmp"),
        _ => PathBuf::from("/tmp"),
    }
}

fn platform_temp_root() -> PathBuf {
    default_temp_root(std::env::consts::OS, std::env::temp_dir())
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    parse_args(&args)?;
    let options = TempReclaimOptions::default();
    let now = now_ms();
    let root = platform_temp_root();
    let output = serde_json::to_value(plan_temp_reclaim(&root, options, now)?)
        .map_err(|_| "temp-reclaim-json-failed".to_string())?;
    println!(
        "{}",
        serde_json::to_string_pretty(&output).map_err(|_| "temp-reclaim-json-failed".to_string())?
    );
    Ok(())
}

fn main() {
    let raw = std::env::args().skip(1).collect::<Vec<_>>();
    if raw.len() == 1 && matches!(raw[0].as_str(), "--help" | "-h") {
        println!("{}", usage());
        return;
    }
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_exposes_only_the_read_only_plan() {
        assert!(parse_args(&[]).is_ok());
        assert!(parse_args(&["--execute".into(), "fingerprint".into()]).is_err());
        assert!(parse_args(&[
            "--execute".into(),
            "fingerprint".into(),
            "phrase".into(),
            "rationale".into(),
        ])
        .is_err());
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
