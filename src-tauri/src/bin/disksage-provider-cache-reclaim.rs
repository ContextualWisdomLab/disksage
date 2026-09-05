use disksage_lib::provider_cache::{
    execute_trash, plan_with_runtime, ProviderCacheCleanupRequest, ProviderCacheReclaimPlan,
};
use std::io::Read;
use std::path::{Path, PathBuf};

const MAX_MANIFEST_BYTES: u64 = 2 * 1024 * 1024;
const PERMANENT_PURGE_UNAVAILABLE: &str =
    "provider-cache-identity-bound-permanent-delete-unavailable";

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

fn require_trash_mode(args: &[String]) -> Result<(), String> {
    let trash = args.iter().any(|arg| arg == "--trash");
    let permanent_purge = args.iter().any(|arg| arg == "--permanent-purge");
    if permanent_purge {
        return Err(PERMANENT_PURGE_UNAVAILABLE.into());
    }
    if trash {
        Ok(())
    } else {
        Err("--trash is required".into())
    }
}

fn public_plan(
    home: &Path,
    applications: &Path,
    podman: &Path,
    observed_at_ms: u64,
) -> ProviderCacheReclaimPlan {
    plan_with_runtime(home, applications, podman, observed_at_ms)
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

fn read_manifest_requests(path: &Path) -> Result<Vec<ProviderCacheCleanupRequest>, String> {
    let file = std::fs::File::open(path).map_err(|_| "manifest read failed".to_string())?;
    let metadata = file
        .metadata()
        .map_err(|_| "manifest metadata failed".to_string())?;
    if metadata.len() > MAX_MANIFEST_BYTES {
        return Err("manifest too large".to_string());
    }

    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_MANIFEST_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "manifest read failed".to_string())?;
    if bytes.len() as u64 > MAX_MANIFEST_BYTES {
        return Err("manifest too large".to_string());
    }

    serde_json::from_slice(&bytes).map_err(|_| "manifest JSON invalid".to_string())
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
            "usage": "disksage-provider-cache-reclaim plan | execute --manifest FILE --approved-plan-fingerprint SHA256 --confirm-plan-fingerprint SHA256 --confirm PHRASE --rationale TEXT --trash",
            "permanent_purge": "unavailable until identity-bound deletion and recovery authority is implemented"
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
        "plan" if args.len() == 1 => serde_json::to_value(public_plan(
            &home,
            std::path::Path::new("/Applications"),
            &podman,
            now_ms(),
        ))
        .map_err(|error| error.to_string()),
        "execute" => {
            validate_execute_args(&args)?;
            require_trash_mode(&args)?;
            let manifest = PathBuf::from(value(&args, "--manifest")?);
            if !manifest.is_absolute() {
                return Err("--manifest must be absolute".into());
            }
            let requests = read_manifest_requests(&manifest)?;
            let data = home.join(".local/share/disksage");
            serde_json::to_value(execute_trash(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oversized_manifest_is_rejected_before_json_parsing() {
        let path = std::env::temp_dir().join(format!(
            "disksage-provider-cache-manifest-{}-{}.json",
            std::process::id(),
            now_ms()
        ));
        std::fs::write(&path, vec![b' '; (MAX_MANIFEST_BYTES + 1) as usize]).unwrap();

        let error = read_manifest_requests(&path).unwrap_err();
        let _ = std::fs::remove_file(&path);

        assert_eq!(error, "manifest too large");
    }

    #[test]
    fn permanent_purge_mode_fails_before_manifest_or_executor_work() {
        let args = vec!["execute".to_string(), "--permanent-purge".to_string()];
        assert_eq!(
            require_trash_mode(&args),
            Err(PERMANENT_PURGE_UNAVAILABLE.into())
        );
    }

    #[test]
    fn trash_mode_remains_available() {
        let args = vec!["execute".to_string(), "--trash".to_string()];
        assert_eq!(require_trash_mode(&args), Ok(()));
    }

    #[test]
    fn public_plan_does_not_advertise_irreversible_approval() {
        let temp = tempfile::tempdir().expect("tempdir");
        let plan = public_plan(
            temp.path(),
            temp.path(),
            Path::new("/missing/podman"),
            1,
        );
        assert!(plan.exact_approval_phrase.is_none());
    }
}
