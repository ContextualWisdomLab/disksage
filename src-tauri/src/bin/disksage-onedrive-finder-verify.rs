//! Headless OneDrive Finder postcheck entry point.
//!
//! This binary is intentionally path-redacted at stdout. It loads the immutable private Finder
//! assistance receipt produced by DiskSage, re-discovers the selected OneDrive root, and delegates
//! identity/allocation verification to the library before emitting a bounded summary.

use disksage_lib::cloud::{self, CloudProvider, CloudRoot};
use disksage_lib::cloud_local_eviction_batch::{
    verify_onedrive_finder_assistance, OnedriveFinderAssistanceReceipt,
    OnedriveFinderAssistanceVerification, MAX_BATCH_ITEMS,
};
use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};

const MAX_RECEIPT_BYTES: u64 = 2 * 1024 * 1024;
const HELP_REQUESTED: &str = "onedrive-finder-verification-help-requested";

#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    cloud_root: PathBuf,
    receipt: PathBuf,
    record_dir: PathBuf,
}

fn usage() -> String {
    format!(
        "usage: {} --cloud-root ABSOLUTE_PATH --receipt ABSOLUTE_FINDER_ASSISTANCE_JSON \\
         --record-dir ABSOLUTE_LOCAL_DIRECTORY",
        env!("CARGO_BIN_NAME")
    )
}

fn next_value(args: &[OsString], index: &mut usize, flag: &str) -> Result<OsString, String> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| format!("{flag} 값이 필요함"))
}

fn parse_args_os(args: &[OsString]) -> Result<Args, String> {
    if args.len() == 1 && matches!(args[0].to_str(), Some("--help" | "-h")) {
        return Err(HELP_REQUESTED.into());
    }

    let mut cloud_root = None;
    let mut receipt = None;
    let mut record_dir = None;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].to_str() {
            Some("--cloud-root") if cloud_root.is_none() => {
                cloud_root = Some(PathBuf::from(next_value(args, &mut index, "--cloud-root")?));
            }
            Some("--receipt") if receipt.is_none() => {
                receipt = Some(PathBuf::from(next_value(args, &mut index, "--receipt")?));
            }
            Some("--record-dir") if record_dir.is_none() => {
                record_dir = Some(PathBuf::from(next_value(args, &mut index, "--record-dir")?));
            }
            Some("--help" | "-h") => return Err("알 수 없는 인자".into()),
            Some(_) => return Err("알 수 없는 인자".into()),
            None => return Err("onedrive-finder-verification-invalid-utf8-argument".into()),
        }
        index += 1;
    }

    let parsed = Args {
        cloud_root: cloud_root.ok_or_else(|| "--cloud-root 값이 필요함".to_string())?,
        receipt: receipt.ok_or_else(|| "--receipt 값이 필요함".to_string())?,
        record_dir: record_dir.ok_or_else(|| "--record-dir 값이 필요함".to_string())?,
    };
    if !parsed.cloud_root.is_absolute()
        || !parsed.receipt.is_absolute()
        || !parsed.record_dir.is_absolute()
    {
        return Err("cloud root, receipt, record-dir은 절대 경로여야 함".into());
    }
    Ok(parsed)
}

fn home_dir() -> Result<PathBuf, String> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|home| home.is_absolute())
        .ok_or_else(|| "HOME을 확인할 수 없음".to_string())
}

fn canonical_existing(path: &Path, error_code: &str) -> Result<PathBuf, String> {
    std::fs::canonicalize(path).map_err(|_| error_code.to_string())
}

fn select_onedrive_root<'a>(
    roots: &'a [CloudRoot],
    requested: &Path,
) -> Result<&'a CloudRoot, String> {
    let matches: Vec<_> = roots
        .iter()
        .filter(|root| cloud::cloud_root_path_matches(Path::new(&root.path), requested))
        .collect();
    match matches.as_slice() {
        [] => Err("요청한 경로가 현재 탐지된 OneDrive 루트와 일치하지 않음".into()),
        [only] if only.provider == CloudProvider::Onedrive => Ok(*only),
        [_] => Err("OneDrive 루트가 필요함".into()),
        _ => Err("요청한 경로와 일치하는 OneDrive 루트가 여러 개임".into()),
    }
}

fn validate_control_locations(
    cloud_root: &Path,
    receipt: &Path,
    record_dir: &Path,
) -> Result<(), String> {
    let cloud_root = canonical_existing(
        cloud_root,
        "onedrive-finder-verification-cloud-root-unavailable",
    )?;
    let record_metadata = std::fs::symlink_metadata(record_dir)
        .map_err(|_| "onedrive-finder-verification-record-dir-unavailable".to_string())?;
    if record_metadata.file_type().is_symlink() || !record_metadata.is_dir() {
        return Err("onedrive-finder-verification-record-dir-unsafe".into());
    }
    let receipt_metadata = std::fs::symlink_metadata(receipt)
        .map_err(|_| "onedrive-finder-verification-receipt-unavailable".to_string())?;
    if receipt_metadata.file_type().is_symlink() || !receipt_metadata.is_file() {
        return Err("onedrive-finder-verification-receipt-unsafe".into());
    }
    let record_dir = canonical_existing(
        record_dir,
        "onedrive-finder-verification-record-dir-unavailable",
    )?;
    let receipt = canonical_existing(receipt, "onedrive-finder-verification-receipt-unavailable")?;
    if record_dir.starts_with(&cloud_root) || receipt.starts_with(&cloud_root) {
        return Err("onedrive-finder-verification-control-data-overlaps-cloud".into());
    }
    if receipt.parent() != Some(record_dir.as_path()) {
        return Err("onedrive-finder-verification-receipt-outside-record-dir".into());
    }
    Ok(())
}

fn read_receipt(path: &Path) -> Result<OnedriveFinderAssistanceReceipt, String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| "onedrive-finder-verification-receipt-unavailable".to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("onedrive-finder-verification-receipt-unsafe".into());
    }
    if metadata.len() == 0 || metadata.len() > MAX_RECEIPT_BYTES {
        return Err("onedrive-finder-verification-receipt-size-invalid".into());
    }
    let file = std::fs::File::open(path)
        .map_err(|_| "onedrive-finder-verification-receipt-open-failed".to_string())?;
    let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or(0));
    file.take(MAX_RECEIPT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "onedrive-finder-verification-receipt-read-failed".to_string())?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_RECEIPT_BYTES {
        return Err("onedrive-finder-verification-receipt-size-invalid".into());
    }
    let receipt: OnedriveFinderAssistanceReceipt = serde_json::from_slice(&bytes)
        .map_err(|_| "onedrive-finder-verification-receipt-json-invalid".to_string())?;
    if usize::try_from(receipt.selected_count).unwrap_or(usize::MAX) > MAX_BATCH_ITEMS
        || receipt.items.len() > MAX_BATCH_ITEMS
    {
        return Err("onedrive-finder-verification-receipt-item-count-invalid".into());
    }
    Ok(receipt)
}

#[derive(Debug, serde::Serialize)]
struct VerificationOutput {
    action: &'static str,
    mutation_executed: bool,
    individual_paths_redacted: bool,
    receipt_id: String,
    verification_id: String,
    verified_at_ms: u64,
    retained_count: u32,
    verified_count: u32,
    total_allocated_bytes_before: u64,
    total_allocated_bytes_after: u64,
    observed_allocation_reduction_bytes: u64,
    verification_complete: bool,
    customer_next_action: String,
}

fn redact_verification(result: OnedriveFinderAssistanceVerification) -> VerificationOutput {
    VerificationOutput {
        action: "verify-onedrive-finder-free-up-space",
        mutation_executed: false,
        individual_paths_redacted: true,
        receipt_id: result.receipt_id,
        verification_id: result.verification_id,
        verified_at_ms: result.verified_at_ms,
        retained_count: result.retained_count,
        verified_count: result.verified_count,
        total_allocated_bytes_before: result.total_allocated_bytes_before,
        total_allocated_bytes_after: result.total_allocated_bytes_after,
        observed_allocation_reduction_bytes: result.observed_allocation_reduction_bytes,
        verification_complete: result.verification_complete,
        customer_next_action: result.customer_next_action,
    }
}

fn print_json<T: serde::Serialize>(value: &T) -> Result<(), String> {
    println!(
        "{}",
        serde_json::to_string_pretty(value)
            .map_err(|_| { "onedrive-finder-verification-output-serialize-failed".to_string() })?
    );
    Ok(())
}

fn run() -> Result<(), String> {
    let raw = std::env::args_os().skip(1).collect::<Vec<_>>();
    let args = parse_args_os(&raw)?;
    let roots = cloud::discover_cloud_roots(&home_dir()?);
    let root = select_onedrive_root(&roots, &args.cloud_root)?.clone();
    validate_control_locations(Path::new(&root.path), &args.receipt, &args.record_dir)?;
    let receipt = read_receipt(&args.receipt)?;
    let verification = verify_onedrive_finder_assistance(
        &root,
        &receipt,
        &args.record_dir,
        cloud::system_now_ms(),
    )?;
    print_json(&redact_verification(verification))
}

fn main() {
    if let Err(error) = run() {
        if error == HELP_REQUESTED {
            println!("{}", usage());
            return;
        }
        eprintln!("{error}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use disksage_lib::cloud::{CloudAccountScope, CloudProvider};
    use disksage_lib::cloud_local_eviction_batch::{
        OnedriveFinderAssistanceItem, ICLOUD_LOCAL_EVICTION_BATCH_VERSION,
    };

    #[cfg(not(windows))]
    const CLOUD_ROOT: &str = "/cloud";
    #[cfg(windows)]
    const CLOUD_ROOT: &str = r"C:\cloud";
    #[cfg(not(windows))]
    const RECEIPT: &str = "/records/receipt.finder-assistance.json";
    #[cfg(windows)]
    const RECEIPT: &str = r"C:\records\receipt.finder-assistance.json";
    #[cfg(not(windows))]
    const RECORD_DIR: &str = "/records";
    #[cfg(windows)]
    const RECORD_DIR: &str = r"C:\records";

    fn sample_receipt(path: &Path) -> OnedriveFinderAssistanceReceipt {
        OnedriveFinderAssistanceReceipt {
            version: ICLOUD_LOCAL_EVICTION_BATCH_VERSION,
            receipt_id: "a".repeat(64),
            batch_fingerprint: "b".repeat(64),
            approval_id: "c".repeat(64),
            approval_evidence_sha256: "f".repeat(64),
            requested_at_ms: 1,
            selected_count: 1,
            total_allocated_bytes_before: 4096,
            items: vec![OnedriveFinderAssistanceItem {
                path: path.to_string_lossy().into_owned(),
                plan_fingerprint: "d".repeat(64),
                item_identifier_fingerprint: "e".repeat(64),
                logical_bytes: 8192,
                allocated_bytes_before: 4096,
                filesystem_modified_ms: 1,
            }],
            finder_selection_requested: true,
            customer_next_action: "In Finder, choose OneDrive Free Up Space for the selected items, then verify in DiskSage.".into(),
        }
    }

    #[test]
    fn parser_accepts_explicit_verification_contract() {
        let args = [
            OsString::from("--cloud-root"),
            OsString::from(CLOUD_ROOT),
            OsString::from("--receipt"),
            OsString::from(RECEIPT),
            OsString::from("--record-dir"),
            OsString::from(RECORD_DIR),
        ];
        let parsed = parse_args_os(&args).unwrap();
        assert_eq!(parsed.cloud_root, PathBuf::from(CLOUD_ROOT));
        assert_eq!(parsed.receipt, PathBuf::from(RECEIPT));
        assert_eq!(parsed.record_dir, PathBuf::from(RECORD_DIR));
    }

    #[test]
    fn parser_rejects_partial_relative_and_duplicate_contracts() {
        assert!(
            parse_args_os(&[OsString::from("--cloud-root"), OsString::from(CLOUD_ROOT)]).is_err()
        );
        assert!(parse_args_os(&[
            OsString::from("--cloud-root"),
            OsString::from("relative"),
            OsString::from("--receipt"),
            OsString::from(RECEIPT),
            OsString::from("--record-dir"),
            OsString::from(RECORD_DIR),
        ])
        .is_err());
        assert!(parse_args_os(&[
            OsString::from("--cloud-root"),
            OsString::from(CLOUD_ROOT),
            OsString::from("--cloud-root"),
            OsString::from(CLOUD_ROOT),
            OsString::from("--receipt"),
            OsString::from(RECEIPT),
            OsString::from("--record-dir"),
            OsString::from(RECORD_DIR),
        ])
        .is_err());
    }

    #[test]
    fn selector_requires_one_matching_onedrive_root() {
        let roots = vec![CloudRoot {
            id: "onedrive:test".into(),
            provider: CloudProvider::Onedrive,
            account_scope: CloudAccountScope::Personal,
            label: "OneDrive".into(),
            path: CLOUD_ROOT.into(),
            readable: true,
            access_issue: None,
        }];
        assert_eq!(
            select_onedrive_root(&roots, Path::new(CLOUD_ROOT))
                .unwrap()
                .provider,
            CloudProvider::Onedrive
        );
        let mut wrong = roots;
        wrong[0].provider = CloudProvider::Icloud;
        assert!(select_onedrive_root(&wrong, Path::new(CLOUD_ROOT)).is_err());
    }

    #[test]
    fn receipt_reader_and_control_locations_stay_local_and_bounded() {
        let temp = tempfile::tempdir().unwrap();
        let cloud = temp.path().join("cloud");
        let records = temp.path().join("records");
        std::fs::create_dir_all(&cloud).unwrap();
        std::fs::create_dir_all(&records).unwrap();
        let receipt_path = records.join("receipt.finder-assistance.json");
        let receipt = sample_receipt(&cloud.join("item.bin"));
        std::fs::write(&receipt_path, serde_json::to_vec(&receipt).unwrap()).unwrap();

        validate_control_locations(&cloud, &receipt_path, &records).unwrap();
        assert_eq!(read_receipt(&receipt_path).unwrap(), receipt);

        let outside = temp.path().join("outside.finder-assistance.json");
        std::fs::write(&outside, serde_json::to_vec(&receipt).unwrap()).unwrap();
        assert_eq!(
            validate_control_locations(&cloud, &outside, &records).unwrap_err(),
            "onedrive-finder-verification-receipt-outside-record-dir"
        );

        let oversized = records.join("oversized.finder-assistance.json");
        std::fs::write(
            &oversized,
            vec![b'x'; usize::try_from(MAX_RECEIPT_BYTES).unwrap() + 1],
        )
        .unwrap();
        assert_eq!(
            read_receipt(&oversized).unwrap_err(),
            "onedrive-finder-verification-receipt-size-invalid"
        );
    }

    #[cfg(unix)]
    #[test]
    fn receipt_reader_rejects_symlink_rebinding() {
        let temp = tempfile::tempdir().unwrap();
        let cloud = temp.path().join("cloud");
        let records = temp.path().join("records");
        std::fs::create_dir_all(&cloud).unwrap();
        std::fs::create_dir_all(&records).unwrap();
        let real = records.join("real.finder-assistance.json");
        std::fs::write(
            &real,
            serde_json::to_vec(&sample_receipt(&cloud.join("item.bin"))).unwrap(),
        )
        .unwrap();
        let alias = records.join("alias.finder-assistance.json");
        std::os::unix::fs::symlink(&real, &alias).unwrap();
        assert_eq!(
            read_receipt(&alias).unwrap_err(),
            "onedrive-finder-verification-receipt-unsafe"
        );
    }

    #[test]
    fn verification_output_never_contains_item_paths() {
        let output = redact_verification(OnedriveFinderAssistanceVerification {
            version: ICLOUD_LOCAL_EVICTION_BATCH_VERSION,
            verification_id: "f".repeat(64),
            receipt_id: "a".repeat(64),
            verified_at_ms: 2,
            retained_count: 1,
            verified_count: 1,
            total_allocated_bytes_before: 4096,
            total_allocated_bytes_after: 0,
            observed_allocation_reduction_bytes: 4096,
            verification_complete: true,
            customer_next_action: "Local space release verified".into(),
        });
        let encoded = serde_json::to_string(&output).unwrap();
        let value = serde_json::from_str::<serde_json::Value>(&encoded).unwrap();
        let object = value.as_object().unwrap();
        assert!(object.keys().all(|key| key != "path" && key != "paths"));
        assert!(!encoded.contains(CLOUD_ROOT));
        assert_eq!(object["individual_paths_redacted"], true);
        assert!(encoded.contains("verification_complete"));
    }
}
