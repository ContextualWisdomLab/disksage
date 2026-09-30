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
    "usage: disksage-provider-client-runtime [--output ABSOLUTE_NEW_FILE.json]\n\
다음 단계: 공급자 앱 상태와 제시된 조치를 확인하세요. 이 명령은 공급자 앱을 재시작하지 않습니다."
}

#[cfg(target_os = "macos")]
fn resolve_platform_output_parent_alias(parent: &Path) -> Result<PathBuf, String> {
    for (alias, expected_target) in [
        (Path::new("/var"), Path::new("/private/var")),
        (Path::new("/tmp"), Path::new("/private/tmp")),
        (Path::new("/etc"), Path::new("/private/etc")),
    ] {
        if !parent.starts_with(alias) {
            continue;
        }
        let metadata = std::fs::symlink_metadata(alias)
            .map_err(|_| "provider-client-runtime-output-parent-unavailable".to_string())?;
        if !metadata.file_type().is_symlink() {
            return Ok(parent.to_path_buf());
        }
        let resolved = std::fs::canonicalize(alias)
            .map_err(|_| "provider-client-runtime-output-parent-unavailable".to_string())?;
        if resolved != expected_target {
            return Err("provider-client-runtime-output-parent-unsafe".into());
        }
        let suffix = parent
            .strip_prefix(alias)
            .map_err(|_| "provider-client-runtime-output-parent-unsafe".to_string())?;
        return Ok(expected_target.join(suffix));
    }
    Ok(parent.to_path_buf())
}

#[cfg(not(target_os = "macos"))]
fn resolve_platform_output_parent_alias(parent: &Path) -> Result<PathBuf, String> {
    Ok(parent.to_path_buf())
}

fn resolve_output_path(path: &Path) -> Result<PathBuf, String> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| "provider-client-runtime-output-parent-missing".to_string())?;
    let file_name = path
        .file_name()
        .ok_or_else(|| "provider-client-runtime-output-parent-missing".to_string())?;
    let parent = resolve_platform_output_parent_alias(parent)?;
    let metadata = std::fs::symlink_metadata(&parent)
        .map_err(|_| "provider-client-runtime-output-parent-unavailable".to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("provider-client-runtime-output-parent-unsafe".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for ancestor in parent
            .ancestors()
            .filter(|ancestor| !ancestor.as_os_str().is_empty())
        {
            let metadata = std::fs::symlink_metadata(ancestor)
                .map_err(|_| "provider-client-runtime-output-parent-unavailable".to_string())?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err("provider-client-runtime-output-parent-unsafe".into());
            }
            let mode = metadata.permissions().mode();
            let shared_writable = mode & 0o022 != 0;
            let sticky = mode & 0o1000 != 0;
            if shared_writable && (ancestor == parent || !sticky) {
                return Err("provider-client-runtime-output-parent-writable-by-others".into());
            }
        }
    }
    Ok(parent.join(file_name))
}

fn validate_output_parent(path: &Path) -> Result<(), String> {
    resolve_output_path(path).map(|_| ())
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
                if output.is_some() {
                    return Err("--output may be supplied once".into());
                }
                index += 1;
                let value = args
                    .get(index)
                    .cloned()
                    .ok_or_else(|| "--output requires an absolute new file".to_string())?;
                let path = PathBuf::from(value);
                if !path.is_absolute() {
                    return Err("--output must be absolute".into());
                }
                output = Some(resolve_output_path(&path)?);
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
    let path = resolve_output_path(path)?;
    #[cfg(unix)]
    let mut file = {
        use std::os::unix::fs::OpenOptionsExt;
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .map_err(|_| "provider-client-runtime-output-create-failed".to_string())?
    };
    #[cfg(not(unix))]
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
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
        assert_eq!(
            parse_args(&["--output".into(), "relative.json".into()]).unwrap_err(),
            "--output must be absolute"
        );

        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("one.json").into_os_string();
        let second = directory.path().join("two.json").into_os_string();
        assert_eq!(
            parse_args(&["--output".into(), first, "--output".into(), second,]).unwrap_err(),
            "--output may be supplied once"
        );
    }

    #[test]
    fn duplicate_output_is_rejected_before_second_parent_probe() {
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("one.json").into_os_string();
        let second = directory
            .path()
            .join("missing-parent")
            .join("two.json")
            .into_os_string();

        assert_eq!(
            parse_args(&["--output".into(), first, "--output".into(), second,]).unwrap_err(),
            "--output may be supplied once"
        );
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
    fn arbitrary_symlinked_output_parent_is_rejected() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().unwrap();
        let actual_parent = directory.path().join("actual-parent");
        std::fs::create_dir(&actual_parent).unwrap();
        let alias = directory.path().join("alias-parent");
        symlink(&actual_parent, &alias).unwrap();
        let requested = alias.join("audit.json");

        assert_eq!(
            parse_args(&["--output".into(), requested.into_os_string()]).unwrap_err(),
            "provider-client-runtime-output-parent-unsafe"
        );
        assert!(!actual_parent.join("audit.json").exists());
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

    #[test]
    #[cfg(unix)]
    fn output_parent_authority_is_rechecked_at_publication_time() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let parent = directory.path().join("audit-parent");
        std::fs::create_dir(&parent).unwrap();
        let path = parent.join("audit.json");
        validate_output_parent(&path).unwrap();

        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o720)).unwrap();
        let result = write_create_new(&path, b"{}");
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700)).unwrap();

        assert_eq!(
            result.unwrap_err(),
            "provider-client-runtime-output-parent-writable-by-others"
        );
        assert!(
            !path.exists(),
            "authority drift after argument admission must not receive an audit artifact"
        );
    }
}
