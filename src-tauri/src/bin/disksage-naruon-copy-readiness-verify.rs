// Offline, path-redacted verification of one Naruon cloud-copy readiness envelope.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use disksage_lib::naruon_cloud_copy_readiness::{
    self, CloudCopyReadinessState, NaruonCloudCopyReadinessEnvelope,
};

const USAGE: &str = "usage: disksage-naruon-copy-readiness-verify ABSOLUTE_READINESS.json";

#[derive(Debug, serde::Serialize)]
struct VerificationSummary {
    ok: bool,
    schema_kind: String,
    schema_version: u32,
    provider: disksage_lib::cloud::CloudProvider,
    readiness_state: CloudCopyReadinessState,
    candidate_count: u64,
    candidate_bytes: u64,
    readiness_fingerprint_sha256: String,
    local_paths_included: bool,
    relative_names_included: bool,
    raw_metadata_values_included: bool,
    cloud_write_executed: bool,
    source_eviction_authorized: bool,
    pre_copy_evidence_met: Option<bool>,
    icloud_native_status_observed: Option<bool>,
    icloud_native_sync_state: Option<String>,
    icloud_native_status_timed_out: Option<bool>,
}

#[derive(Debug, PartialEq, Eq)]
enum TerminalRequest {
    Help,
    Verify(PathBuf),
}

fn parse_args(args: &[OsString]) -> Result<TerminalRequest, String> {
    if args.len() == 1 && (args[0] == OsStr::new("-h") || args[0] == OsStr::new("--help")) {
        return Ok(TerminalRequest::Help);
    }
    if args.len() != 1 {
        return Err("naruon-copy-readiness-verifier-usage-invalid".into());
    }
    let path = PathBuf::from(&args[0]);
    if !path.is_absolute() {
        return Err("naruon-copy-readiness-input-path-not-absolute".into());
    }
    Ok(TerminalRequest::Verify(path))
}

fn verification_summary(envelope: NaruonCloudCopyReadinessEnvelope) -> VerificationSummary {
    let native_status = envelope
        .icloud_new_copy_admission
        .as_ref()
        .and_then(|summary| summary.native_status.as_ref());
    VerificationSummary {
        ok: true,
        schema_kind: envelope.schema_kind,
        schema_version: envelope.schema_version,
        provider: envelope.provider,
        readiness_state: envelope.readiness_state,
        candidate_count: envelope.candidate_count,
        candidate_bytes: envelope.candidate_bytes,
        readiness_fingerprint_sha256: envelope.readiness_fingerprint_sha256,
        local_paths_included: envelope.local_paths_included,
        relative_names_included: envelope.relative_names_included,
        raw_metadata_values_included: envelope.raw_metadata_values_included,
        cloud_write_executed: envelope.cloud_write_executed,
        source_eviction_authorized: envelope.source_eviction_authorized,
        pre_copy_evidence_met: envelope.pre_copy_evidence_met,
        icloud_native_status_observed: native_status.map(|status| status.status_observed),
        icloud_native_sync_state: native_status.and_then(|status| status.sync_state.clone()),
        icloud_native_status_timed_out: native_status.map(|status| status.timed_out),
    }
}

fn verify(path: &Path) -> Result<VerificationSummary, String> {
    naruon_cloud_copy_readiness::read_and_validate_naruon_cloud_copy_readiness(path)
        .map(verification_summary)
}

fn print_json(value: &impl serde::Serialize) {
    let encoded = serde_json::to_string(value).unwrap_or_else(|_| {
        "{\"ok\":false,\"error_code\":\"naruon-copy-readiness-verifier-json-failed\"}".into()
    });
    println!("{encoded}");
}

fn main() {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let path = match parse_args(&args) {
        Ok(TerminalRequest::Help) => {
            println!("{USAGE}");
            return;
        }
        Ok(TerminalRequest::Verify(path)) => path,
        Err(error_code) => {
            print_json(&serde_json::json!({ "ok": false, "error_code": error_code }));
            eprintln!("{USAGE}");
            std::process::exit(64);
        }
    };
    match verify(&path) {
        Ok(summary) => print_json(&summary),
        Err(error_code) => {
            print_json(&serde_json::json!({ "ok": false, "error_code": error_code }));
            std::process::exit(65);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn absolute_fixture() -> PathBuf {
        if cfg!(windows) {
            PathBuf::from(r"C:\readiness.json")
        } else {
            PathBuf::from("/readiness.json")
        }
    }

    #[test]
    fn parser_requires_exactly_one_absolute_path_or_sole_help() {
        assert!(parse_args(&[]).is_err());
        assert!(parse_args(&["relative.json".into()]).is_err());
        assert!(parse_args(&["one.json".into(), "two.json".into()]).is_err());
        assert_eq!(parse_args(&["--help".into()]).unwrap(), TerminalRequest::Help);
        assert_eq!(parse_args(&["-h".into()]).unwrap(), TerminalRequest::Help);
        assert!(parse_args(&["--help".into(), "relative.json".into()]).is_err());

        let absolute = absolute_fixture();
        assert_eq!(
            parse_args(&[absolute.as_os_str().to_owned()]).unwrap(),
            TerminalRequest::Verify(absolute)
        );
    }
}
