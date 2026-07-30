//! Path-free, read-only preflight for one macOS File Provider item.

#[cfg(not(coverage))]
use std::fs::OpenOptions;
#[cfg(not(coverage))]
use std::io::Write;
#[cfg(not(coverage))]
use std::path::{Component, Path, PathBuf};

#[cfg(not(coverage))]
use disksage_lib::cloud;
#[cfg(not(coverage))]
use disksage_lib::provider_sync::{audit_file_provider_status, FileProviderStatusAudit};

#[cfg(not(coverage))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    path: PathBuf,
    candidate_fingerprint: Option<String>,
    output: Option<PathBuf>,
}

#[cfg(not(coverage))]
#[derive(Debug, serde::Serialize)]
struct FileProviderStatusAuditReport {
    schema_version: u32,
    output_mode: &'static str,
    candidate_fingerprint: Option<String>,
    audit: FileProviderStatusAudit,
    local_path_included: bool,
    local_content_read: bool,
    provider_sync_attested: bool,
    remote_capacity_verified: bool,
    remote_content_verified: bool,
    cloud_write_executed: bool,
    source_eviction_authorized: bool,
    mutation_performed: bool,
    notices: Vec<&'static str>,
}

#[cfg(not(coverage))]
fn absolute_without_parent(path: &Path) -> bool {
    path.is_absolute()
        && !path
            .components()
            .any(|component| component == Component::ParentDir)
}

#[cfg(not(coverage))]
fn valid_hex64(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(not(coverage))]
fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut path = None;
    let mut candidate_fingerprint = None;
    let mut output = None;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--path" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--path requires an absolute existing file".to_string())?;
                if path.replace(PathBuf::from(value)).is_some() {
                    return Err("--path may be supplied once".into());
                }
            }
            "--candidate-fingerprint" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--candidate-fingerprint requires hex64".to_string())?;
                if !valid_hex64(value) {
                    return Err("--candidate-fingerprint must be hex64".into());
                }
                if candidate_fingerprint
                    .replace(value.to_ascii_lowercase())
                    .is_some()
                {
                    return Err("--candidate-fingerprint may be supplied once".into());
                }
            }
            "--output" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--output requires an absolute new file".to_string())?;
                if output.replace(PathBuf::from(value)).is_some() {
                    return Err("--output may be supplied once".into());
                }
            }
            "--help" | "-h" => {
                return Err(
                    "usage: disksage-file-provider-status-audit --path ABSOLUTE_EXISTING_FILE \
                     [--candidate-fingerprint HEX64] [--output ABSOLUTE_NEW_FILE.json]"
                        .into(),
                )
            }
            flag => return Err(format!("unknown argument: {flag}")),
        }
        index += 1;
    }

    let path = path.ok_or_else(|| "--path is required".to_string())?;
    if !absolute_without_parent(&path) {
        return Err("--path must be absolute without parent traversal".into());
    }
    if let Some(output) = output.as_deref() {
        if !absolute_without_parent(output) {
            return Err("--output must be absolute without parent traversal".into());
        }
    }
    Ok(Args {
        path,
        candidate_fingerprint,
        output,
    })
}

#[cfg(not(coverage))]
fn report(args: &Args, observed_at_ms: u64) -> FileProviderStatusAuditReport {
    let audit = audit_file_provider_status(&args.path, observed_at_ms);
    FileProviderStatusAuditReport {
        schema_version: 1,
        output_mode: "file-provider-status-audit",
        candidate_fingerprint: args.candidate_fingerprint.clone(),
        local_content_read: audit.local_content_read,
        mutation_performed: audit.mutation_performed,
        audit,
        local_path_included: false,
        provider_sync_attested: false,
        remote_capacity_verified: false,
        remote_content_verified: false,
        cloud_write_executed: false,
        source_eviction_authorized: false,
        notices: vec![
            "preflight-only-no-content-hash",
            "provider-native-status-is-not-remote-content-proof",
            "candidate-fingerprint-is-caller-supplied-binding",
            "receipt-and-provider-evidence-required-before-eviction",
            "no-upload-no-hydration-no-eviction-no-delete",
        ],
    }
}

#[cfg(not(coverage))]
fn write_create_new(path: &Path, encoded: &[u8]) -> Result<(), String> {
    #[cfg(unix)]
    let mut file = {
        use std::os::unix::fs::OpenOptionsExt;
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .map_err(|_| "file-provider-status-output-create-failed".to_string())?
    };
    #[cfg(not(unix))]
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| "file-provider-status-output-create-failed".to_string())?;
    file.write_all(encoded)
        .and_then(|_| file.sync_all())
        .map_err(|_| "file-provider-status-output-write-failed".to_string())
}

#[cfg(not(coverage))]
fn run(args: &[String]) -> Result<(), String> {
    let args = parse_args(args)?;
    let report = report(&args, cloud::system_now_ms());
    let encoded = serde_json::to_vec_pretty(&report)
        .map_err(|_| "file-provider-status-output-encode-failed".to_string())?;
    if let Some(path) = args.output.as_deref() {
        write_create_new(path, &encoded)?;
    }
    println!(
        "{}",
        std::str::from_utf8(&encoded)
            .map_err(|_| "file-provider-status-output-encode-failed".to_string())?
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
    fn args_require_safe_absolute_paths_and_validate_optional_binding() {
        assert!(parse_args(&[]).is_err());
        assert!(parse_args(&["--path".into(), "relative".into()]).is_err());
        assert!(parse_args(&["--path".into(), "/tmp/../secret".into()]).is_err());
        assert!(parse_args(&[
            "--path".into(),
            "/tmp/item".into(),
            "--candidate-fingerprint".into(),
            "not-hex".into(),
        ])
        .is_err());
        let parsed = parse_args(&[
            "--path".into(),
            "/tmp/item".into(),
            "--candidate-fingerprint".into(),
            "A".repeat(64),
            "--output".into(),
            "/tmp/audit.json".into(),
        ])
        .unwrap();
        assert_eq!(parsed.candidate_fingerprint, Some("a".repeat(64)));
    }

    #[test]
    fn report_is_path_free_and_never_claims_authority() {
        let args = Args {
            path: PathBuf::from("/definitely/missing/private-item"),
            candidate_fingerprint: Some("a".repeat(64)),
            output: None,
        };
        let report = report(&args, 42);
        let encoded = serde_json::to_string(&report).unwrap();

        assert!(!report.local_path_included);
        assert!(!report.local_content_read);
        assert!(!report.provider_sync_attested);
        assert!(!report.remote_capacity_verified);
        assert!(!report.remote_content_verified);
        assert!(!report.cloud_write_executed);
        assert!(!report.source_eviction_authorized);
        assert!(!report.mutation_performed);
        assert!(!encoded.contains("/definitely/"));
        assert!(!encoded.contains("private-item"));
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
