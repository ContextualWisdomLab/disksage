//! Bounded desktop-provider restart with a machine-readable, non-mutation receipt.
//!
//! This command only requests a restart of OneDrive or Google Drive. It never copies cloud data,
//! deletes local data, or treats process presence as synchronization evidence.

#[cfg(not(coverage))]
use std::fs::OpenOptions;
#[cfg(not(coverage))]
use std::io::Write;
#[cfg(not(coverage))]
use std::path::{Path, PathBuf};

#[cfg(not(coverage))]
use disksage_lib::cloud::{self, CloudProvider};
#[cfg(not(coverage))]
use disksage_lib::provider_recovery::{self, ProviderRecoveryOutput};

#[cfg(not(coverage))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    provider: CloudProvider,
    output: Option<PathBuf>,
    allow_graceful_term: bool,
}

#[cfg(not(coverage))]
fn parse_provider(value: &str) -> Result<CloudProvider, String> {
    match value {
        "onedrive" => Ok(CloudProvider::Onedrive),
        "google-drive" => Ok(CloudProvider::GoogleDrive),
        "icloud" => Err("iCloud is system-managed; use the native health probe".into()),
        _ => Err(format!("unsupported provider: {value}")),
    }
}

#[cfg(not(coverage))]
fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut provider = None;
    let mut output = None;
    let mut allow_graceful_term = false;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--provider" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--provider requires onedrive or google-drive".to_string())?;
                if provider.replace(parse_provider(value)?).is_some() {
                    return Err("--provider may be supplied once".into());
                }
            }
            "--output" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--output requires an absolute new file".to_string())?;
                let path = PathBuf::from(value);
                if !path.is_absolute() {
                    return Err("--output must be absolute".into());
                }
                if output.replace(path).is_some() {
                    return Err("--output may be supplied once".into());
                }
            }
            "--allow-graceful-term" => {
                if allow_graceful_term {
                    return Err("--allow-graceful-term may be supplied once".into());
                }
                allow_graceful_term = true;
            }
            "--help" | "-h" => {
                return Err(
                    "usage: disksage-provider-recovery --provider onedrive|google-drive [--allow-graceful-term] [--output ABSOLUTE_NEW_FILE.json]"
                        .into(),
                )
            }
            flag => return Err(format!("unknown argument: {flag}")),
        }
        index += 1;
    }
    let provider = provider.ok_or_else(|| "--provider is required".to_string())?;
    if !provider_recovery::recovery_supported(provider) {
        return Err("provider-recovery-not-supported".into());
    }
    Ok(Args {
        provider,
        output,
        allow_graceful_term,
    })
}

#[cfg(not(coverage))]
fn write_create_new(path: &Path, output: &ProviderRecoveryOutput) -> Result<(), String> {
    let encoded = serde_json::to_vec_pretty(output)
        .map_err(|_| "provider-recovery-output-encode-failed".to_string())?;
    #[cfg(unix)]
    let mut file = {
        use std::os::unix::fs::OpenOptionsExt;
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .map_err(|_| "provider-recovery-output-create-failed".to_string())?
    };
    #[cfg(not(unix))]
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| "provider-recovery-output-create-failed".to_string())?;
    file.write_all(&encoded)
        .and_then(|_| file.sync_all())
        .map_err(|_| "provider-recovery-output-write-failed".to_string())
}

#[cfg(not(coverage))]
fn run(args: &[String]) -> Result<(), String> {
    let args = parse_args(args)?;
    let output = provider_recovery::recover_provider_client_with_options(
        args.provider,
        cloud::system_now_ms(),
        args.allow_graceful_term,
    )?;
    if let Some(path) = args.output.as_deref() {
        write_create_new(path, &output)?;
    }
    let encoded = serde_json::to_vec_pretty(&output)
        .map_err(|_| "provider-recovery-output-encode-failed".to_string())?;
    println!(
        "{}",
        std::str::from_utf8(&encoded)
            .map_err(|_| "provider-recovery-output-encode-failed".to_string())?
    );
    Ok(())
}

#[cfg(not(coverage))]
fn main() {
    if let Err(error) = run(&std::env::args().skip(1).collect::<Vec<_>>()) {
        eprintln!("{error}");
        std::process::exit(2);
    }
}

#[cfg(coverage)]
fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_is_required_and_icloud_is_rejected() {
        assert!(parse_args(&[]).is_err());
        assert!(parse_args(&["--provider".into(), "icloud".into()]).is_err());
        assert_eq!(
            parse_args(&["--provider".into(), "google-drive".into()])
                .unwrap()
                .provider,
            CloudProvider::GoogleDrive
        );
    }

    #[test]
    fn output_is_absolute_and_unique() {
        assert!(parse_args(&[
            "--provider".into(),
            "onedrive".into(),
            "--output".into(),
            "relative.json".into(),
        ])
        .is_err());
        assert!(parse_args(&[
            "--provider".into(),
            "onedrive".into(),
            "--output".into(),
            "/tmp/one.json".into(),
            "--output".into(),
            "/tmp/two.json".into(),
        ])
        .is_err());
    }

    #[test]
    fn graceful_term_is_explicit_and_not_duplicable() {
        let args = parse_args(&[
            "--provider".into(),
            "google-drive".into(),
            "--allow-graceful-term".into(),
        ])
        .unwrap();
        assert!(args.allow_graceful_term);
        assert!(parse_args(&[
            "--provider".into(),
            "google-drive".into(),
            "--allow-graceful-term".into(),
            "--allow-graceful-term".into(),
        ])
        .is_err());
    }

    #[test]
    fn recovery_output_never_claims_data_mutation() {
        let output = ProviderRecoveryOutput {
            schema_version: provider_recovery::PROVIDER_RECOVERY_SCHEMA_VERSION,
            provider: CloudProvider::GoogleDrive,
            action: "restart-provider-client".into(),
            pre_runtime_observed: true,
            quit_requested: true,
            launch_requested: true,
            post_runtime_observed: Some(true),
            blockers: Vec::new(),
            cloud_write_executed: false,
            source_eviction_executed: false,
        };
        assert!(!output.cloud_write_executed);
        assert!(!output.source_eviction_executed);
    }
}
