//! Path-free audit of local cloud-provider client runtime prerequisites.

use std::ffi::OsString;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use disksage_lib::cloud::{self, CloudProvider};
use disksage_lib::provider_client_runtime::{
    self, ProviderClientRuntimeSnapshot, ProviderClientRuntimeState,
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    output: Option<PathBuf>,
}

#[derive(Debug, serde::Serialize)]
struct ProviderClientRuntimeAudit {
    schema_version: u32,
    schema_kind: &'static str,
    generated_at_ms: u64,
    evidence_scope: &'static str,
    provider_count: usize,
    runtime_prerequisite_met_count: usize,
    runtime_prerequisite_blocked_count: usize,
    evidence_unavailable_count: usize,
    snapshots: Vec<ProviderClientRuntimeSnapshot>,
    local_paths_included: bool,
    account_identifiers_included: bool,
    raw_process_names_included: bool,
    remote_capacity_verified: bool,
    remote_sync_attested: bool,
    cloud_write_executed: bool,
    notices: Vec<&'static str>,
}

fn usage() -> &'static str {
    "usage: disksage-provider-client-runtime [--output ABSOLUTE_NEW_FILE.json]"
}

fn validate_output_parent(path: &Path) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| "provider-client-runtime-output-parent-missing".to_string())?;
    let metadata = std::fs::symlink_metadata(parent)
        .map_err(|_| "provider-client-runtime-output-parent-unavailable".to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("provider-client-runtime-output-parent-unsafe".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o022 != 0 {
            return Err("provider-client-runtime-output-parent-writable-by-others".into());
        }
    }
    Ok(())
}

fn parse_args(args: &[OsString]) -> Result<Args, String> {
    let mut output = None;
    let mut index = 0usize;
    while index < args.len() {
        let flag = args[index]
            .to_str()
            .ok_or_else(|| "provider-client-runtime-argument-not-utf8".to_string())?;
        match flag {
            "--output" => {
                index += 1;
                let value = args
                    .get(index)
                    .cloned()
                    .ok_or_else(|| "--output requires an absolute new file".to_string())?;
                let path = PathBuf::from(value);
                if !path.is_absolute() {
                    return Err("--output must be absolute".into());
                }
                validate_output_parent(&path)?;
                if output.replace(path).is_some() {
                    return Err("--output may be supplied once".into());
                }
            }
            "--help" | "-h" => return Err(usage().into()),
            _ => return Err("provider-client-runtime-unknown-argument".into()),
        }
        index += 1;
    }
    Ok(Args { output })
}

fn audit(generated_at_ms: u64) -> ProviderClientRuntimeAudit {
    let snapshots = [
        CloudProvider::Icloud,
        CloudProvider::Onedrive,
        CloudProvider::GoogleDrive,
    ]
    .into_iter()
    .map(|provider| {
        provider_client_runtime::collect_provider_client_runtime(provider, generated_at_ms)
    })
    .collect::<Vec<_>>();
    let runtime_prerequisite_met_count = snapshots
        .iter()
        .filter(|snapshot| snapshot.copy_prerequisite_met)
        .count();
    let evidence_unavailable_count = snapshots
        .iter()
        .filter(|snapshot| snapshot.state == ProviderClientRuntimeState::EvidenceUnavailable)
        .count();
    ProviderClientRuntimeAudit {
        schema_version: 1,
        schema_kind: "disksage.provider-client-runtime-audit",
        generated_at_ms,
        evidence_scope: "provider-level-local-runtime-prerequisite-only",
        provider_count: snapshots.len(),
        runtime_prerequisite_met_count,
        runtime_prerequisite_blocked_count: snapshots
            .len()
            .saturating_sub(runtime_prerequisite_met_count),
        evidence_unavailable_count,
        snapshots,
        local_paths_included: false,
        account_identifiers_included: false,
        raw_process_names_included: false,
        remote_capacity_verified: false,
        remote_sync_attested: false,
        cloud_write_executed: false,
        notices: vec![
            "process-presence-is-not-account-authentication",
            "runtime-prerequisite-is-not-remote-capacity",
            "runtime-prerequisite-is-not-sync-attestation",
            "copy-still-requires-fresh-capacity-and-review-gates",
        ],
    }
}

fn write_create_new(path: &Path, encoded: &[u8]) -> Result<(), String> {
    #[cfg(unix)]
    let mut file = {
        use std::os::unix::fs::OpenOptionsExt;
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .map_err(|_| "provider-client-runtime-output-create-failed".to_string())?
    };
    #[cfg(not(unix))]
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| "provider-client-runtime-output-create-failed".to_string())?;
    file.write_all(encoded)
        .and_then(|_| file.sync_all())
        .map_err(|_| "provider-client-runtime-output-write-failed".to_string())
}

fn run(args: &[OsString]) -> Result<(), String> {
    if matches!(args, [flag] if matches!(flag.to_str(), Some("--help") | Some("-h"))) {
        println!("{}", usage());
        return Ok(());
    }

    let args = parse_args(args)?;
    let report = audit(cloud::system_now_ms());
    let encoded = serde_json::to_vec_pretty(&report)
        .map_err(|_| "provider-client-runtime-output-encode-failed".to_string())?;
    if let Some(path) = args.output.as_deref() {
        write_create_new(path, &encoded)?;
    }
    println!(
        "{}",
        std::str::from_utf8(&encoded)
            .map_err(|_| "provider-client-runtime-output-encode-failed".to_string())?
    );
    Ok(())
}

fn command_args() -> Vec<OsString> {
    std::env::args_os().skip(1).collect()
}

fn main() {
    if let Err(error) = run(&command_args()) {
        eprintln!("{error}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_argument_must_be_absolute_and_unique() {
        assert!(parse_args(&[]).unwrap().output.is_none());
        assert!(parse_args(&["--output".into(), "relative.json".into()]).is_err());
        assert!(parse_args(&[
            "--output".into(),
            "/tmp/one.json".into(),
            "--output".into(),
            "/tmp/two.json".into(),
        ])
        .is_err());
    }

    #[test]
    fn audit_is_path_free_and_does_not_claim_remote_state() {
        let report = audit(42);
        let encoded = serde_json::to_string(&report).unwrap();

        assert_eq!(report.provider_count, 3);
        assert_eq!(
            report.runtime_prerequisite_met_count + report.runtime_prerequisite_blocked_count,
            3
        );
        assert!(!report.local_paths_included);
        assert!(!report.account_identifiers_included);
        assert!(!report.raw_process_names_included);
        assert!(!report.remote_capacity_verified);
        assert!(!report.remote_sync_attested);
        assert!(!report.cloud_write_executed);
        assert!(!encoded.contains("/Users/"));
        assert!(!encoded.contains('@'));
    }

    #[test]
    #[cfg(unix)]
    fn output_is_create_new_mode_0600() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("audit.json");
        write_create_new(&path, b"{}").unwrap();

        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(write_create_new(&path, b"changed").is_err());
        assert_eq!(std::fs::read(path).unwrap(), b"{}");
    }
}
