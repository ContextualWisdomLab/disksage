use disksage_lib::provider_cache_reclaim::{
    execute, plan_with_runtime, ProviderCacheCleanupMode, ProviderCacheCleanupRequest,
};
use std::path::PathBuf;

fn validate_execute_args(args: &[String]) -> Result<(), String> {
    let valued = [
        "--manifest",
        "--approved-plan-fingerprint",
        "--confirm-plan-fingerprint",
        "--confirm",
        "--rationale",
    ];
    let switches = ["--trash", "--permanent-purge"];
    let mut seen = std::collections::BTreeSet::new();
    let mut index = 1;
    while index < args.len() {
        let flag = args[index].as_str();
        if !seen.insert(flag.to_string()) {
            return Err(format!("duplicate argument: {flag}"));
        }
        if switches.contains(&flag) {
            index += 1;
        } else if valued.contains(&flag) {
            if args
                .get(index + 1)
                .is_none_or(|value| value.starts_with("--"))
            {
                return Err(format!("{flag} value is required"));
            }
            index += 2;
        } else {
            return Err(format!("unknown argument: {flag}"));
        }
    }
    Ok(())
}

fn value(args: &[String], flag: &str) -> Result<String, String> {
    let index = args
        .iter()
        .position(|arg| arg == flag)
        .ok_or_else(|| format!("{flag} is required"))?;
    args.get(index + 1)
        .filter(|value| !value.starts_with("--"))
        .cloned()
        .ok_or_else(|| format!("{flag} value is required"))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_millis() as u64)
        .unwrap_or(0)
}

fn run() -> Result<serde_json::Value, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let action = args.first().map(String::as_str).unwrap_or("help");
    if matches!(action, "help" | "--help" | "-h") {
        return Ok(serde_json::json!({
            "usage": "disksage-provider-cache-reclaim plan | execute --manifest FILE --approved-plan-fingerprint SHA256 --confirm-plan-fingerprint SHA256 --confirm PHRASE --rationale TEXT [--trash|--permanent-purge]"
        }));
    }
    let home = std::env::var("HOME")
        .map(PathBuf::from)
        .map_err(|_| "HOME is required".to_string())?;
    let podman = [
        "/opt/homebrew/bin/podman",
        "/usr/local/bin/podman",
        "/usr/bin/podman",
    ]
    .into_iter()
    .map(PathBuf::from)
    .find(|path| path.is_file())
    .unwrap_or_else(|| PathBuf::from("podman"));
    match action {
        "plan" if args.len() == 1 => serde_json::to_value(plan_with_runtime(
            &home,
            std::path::Path::new("/Applications"),
            &podman,
            now_ms(),
        ))
        .map_err(|error| error.to_string()),
        "execute" => {
            validate_execute_args(&args)?;
            let mode = match (
                args.iter().any(|arg| arg == "--trash"),
                args.iter().any(|arg| arg == "--permanent-purge"),
            ) {
                (true, false) => ProviderCacheCleanupMode::Trash,
                (false, true) => ProviderCacheCleanupMode::PermanentPurge,
                _ => return Err("exactly one of --trash or --permanent-purge is required".into()),
            };
            let manifest = PathBuf::from(value(&args, "--manifest")?);
            if !manifest.is_absolute() {
                return Err("--manifest must be absolute".into());
            }
            let requests: Vec<ProviderCacheCleanupRequest> = serde_json::from_slice(
                &std::fs::read(manifest).map_err(|_| "manifest read failed")?,
            )
            .map_err(|_| "manifest JSON invalid")?;
            let data = home.join(".local/share/disksage");
            serde_json::to_value(execute(
                &home,
                std::path::Path::new("/Applications"),
                &podman,
                &requests,
                &value(&args, "--approved-plan-fingerprint")?,
                &value(&args, "--confirm-plan-fingerprint")?,
                &value(&args, "--confirm")?,
                &value(&args, "--rationale")?,
                &data.join("journal.jsonl"),
                &data.join("receipts/provider-cache"),
                mode,
                now_ms(),
            )?)
            .map_err(|error| error.to_string())
        }
        _ => Err("invalid arguments; use --help".into()),
    }
}

fn main() {
    match run() {
        Ok(value) => println!("{}", serde_json::to_string_pretty(&value).unwrap()),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }
}
