use disksage_lib::cloud::{discover_cloud_roots_report, CloudAccountScope, CloudProvider};
use disksage_lib::incomplete_download::{
    collect_incomplete_download_audit, DEFAULT_MAX_ENTRIES, DEFAULT_STALE_AFTER_DAYS,
    MAX_STALE_AFTER_DAYS,
};
use disksage_lib::incomplete_download_materialization::plan_incomplete_download_materialization;
use disksage_lib::incomplete_download_materialization_destination::{
    approve_incomplete_download_destination, IncompleteDownloadDestinationPlan,
};
use disksage_lib::incomplete_download_materialization_execution::{
    execute_incomplete_download_materialization,
    summarize_incomplete_download_materialization_receipt,
};
use disksage_lib::incomplete_download_recovery::{
    validate_incomplete_download_recovery, RecoveryValidationLimits,
};
use disksage_lib::provider_capacity::{collect_icloud_native_capacity, CloudCapacitySnapshot};
use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

const MAX_PRIVATE_PLAN_BYTES: u64 = 1024 * 1024;
const MAX_CAPACITY_SNAPSHOT_BYTES: u64 = 64 * 1024;
const MAX_REVIEW_TEXT_BYTES: usize = 2_048;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    source_root: PathBuf,
    destination_plan: PathBuf,
    confirmed_plan_fingerprint: String,
    receipt_dir: PathBuf,
    approved_by: String,
    rationale: String,
    max_entries: usize,
    stale_after_days: u64,
    live_icloud_capacity: bool,
    capacity_snapshot: Option<PathBuf>,
    execute: bool,
}

fn absolute_without_parent(path: &Path) -> bool {
    path.is_absolute()
        && !path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
}

fn valid_hex64(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn usage() -> String {
    format!(
        "usage: disksage-incomplete-download-materialize \
         --source-root ABSOLUTE_PATH \
         --destination-plan ABSOLUTE_PRIVATE_PLAN.json \
         --confirm-plan-fingerprint HEX64 \
         --receipt-dir ABSOLUTE_PRIVATE_DIRECTORY \
         --approved-by human:ID --rationale TEXT --execute \
         (--live-icloud-capacity | --capacity-snapshot ABSOLUTE.json) \
         [--max-entries 1..={DEFAULT_MAX_ENTRIES}] \
         [--stale-after-days 1..={MAX_STALE_AFTER_DAYS}]\n\
         다음 단계: 계획 지문과 용량 증거를 검토한 뒤 승인 정보와 --execute를 제공하세요."
    )
}

fn next_value(raw: &[OsString], index: &mut usize, flag: &str) -> Result<OsString, String> {
    *index += 1;
    raw.get(*index)
        .cloned()
        .ok_or_else(|| format!("{flag} 값이 필요함"))
}

fn next_text_value(raw: &[OsString], index: &mut usize, flag: &str) -> Result<String, String> {
    next_value(raw, index, flag)?
        .into_string()
        .map_err(|_| format!("{flag} 값은 UTF-8 텍스트여야 함"))
}

fn parse_args(raw: &[OsString]) -> Result<Args, String> {
    let mut source_root = None;
    let mut destination_plan = None;
    let mut confirmed_plan_fingerprint = None;
    let mut receipt_dir = None;
    let mut approved_by = None;
    let mut rationale = None;
    let mut max_entries = DEFAULT_MAX_ENTRIES;
    let mut max_entries_seen = false;
    let mut stale_after_days = DEFAULT_STALE_AFTER_DAYS;
    let mut stale_after_days_seen = false;
    let mut live_icloud_capacity = false;
    let mut capacity_snapshot = None;
    let mut execute = false;
    let mut index = 0usize;
    while index < raw.len() {
        let option = raw[index]
            .to_str()
            .ok_or_else(|| "incomplete-download-materialize-unknown-argument".to_string())?;
        match option {
            "--source-root" => {
                if source_root.is_some() {
                    return Err("--source-root는 한 번만 지정할 수 있음".into());
                }
                source_root = Some(PathBuf::from(next_value(raw, &mut index, "--source-root")?));
            }
            "--destination-plan" => {
                if destination_plan.is_some() {
                    return Err("--destination-plan은 한 번만 지정할 수 있음".into());
                }
                destination_plan = Some(PathBuf::from(next_value(
                    raw,
                    &mut index,
                    "--destination-plan",
                )?));
            }
            "--confirm-plan-fingerprint" => {
                if confirmed_plan_fingerprint.is_some() {
                    return Err("--confirm-plan-fingerprint는 한 번만 지정할 수 있음".into());
                }
                confirmed_plan_fingerprint = Some(next_text_value(
                    raw,
                    &mut index,
                    "--confirm-plan-fingerprint",
                )?);
            }
            "--receipt-dir" => {
                if receipt_dir.is_some() {
                    return Err("--receipt-dir은 한 번만 지정할 수 있음".into());
                }
                receipt_dir = Some(PathBuf::from(next_value(raw, &mut index, "--receipt-dir")?));
            }
            "--approved-by" => {
                if approved_by.is_some() {
                    return Err("--approved-by는 한 번만 지정할 수 있음".into());
                }
                approved_by = Some(next_text_value(raw, &mut index, "--approved-by")?);
            }
            "--rationale" => {
                if rationale.is_some() {
                    return Err("--rationale은 한 번만 지정할 수 있음".into());
                }
                rationale = Some(next_text_value(raw, &mut index, "--rationale")?);
            }
            "--max-entries" => {
                if max_entries_seen {
                    return Err("--max-entries는 한 번만 지정할 수 있음".into());
                }
                max_entries_seen = true;
                let parsed = next_text_value(raw, &mut index, "--max-entries")?
                    .parse::<usize>()
                    .map_err(|_| "--max-entries는 양의 정수여야 함".to_string())?;
                if parsed == 0 || parsed > DEFAULT_MAX_ENTRIES {
                    return Err(format!(
                        "--max-entries는 1..={DEFAULT_MAX_ENTRIES} 범위여야 함"
                    ));
                }
                max_entries = parsed;
            }
            "--stale-after-days" => {
                if stale_after_days_seen {
                    return Err("--stale-after-days는 한 번만 지정할 수 있음".into());
                }
                stale_after_days_seen = true;
                let parsed = next_text_value(raw, &mut index, "--stale-after-days")?
                    .parse::<u64>()
                    .map_err(|_| "--stale-after-days는 양의 정수여야 함".to_string())?;
                if !(1..=MAX_STALE_AFTER_DAYS).contains(&parsed) {
                    return Err(format!(
                        "--stale-after-days는 1..={MAX_STALE_AFTER_DAYS} 범위여야 함"
                    ));
                }
                stale_after_days = parsed;
            }
            "--live-icloud-capacity" => {
                if live_icloud_capacity {
                    return Err("--live-icloud-capacity는 한 번만 지정할 수 있음".into());
                }
                live_icloud_capacity = true;
            }
            "--capacity-snapshot" => {
                if capacity_snapshot.is_some() {
                    return Err("--capacity-snapshot은 한 번만 지정할 수 있음".into());
                }
                capacity_snapshot = Some(PathBuf::from(next_value(
                    raw,
                    &mut index,
                    "--capacity-snapshot",
                )?));
            }
            "--execute" => {
                if execute {
                    return Err("--execute는 한 번만 지정할 수 있음".into());
                }
                execute = true;
            }
            _unknown => return Err("incomplete-download-materialize-unknown-argument".into()),
        }
        index += 1;
    }

    let source_root = source_root.ok_or_else(|| "--source-root가 필요함".to_string())?;
    let destination_plan =
        destination_plan.ok_or_else(|| "--destination-plan이 필요함".to_string())?;
    let confirmed_plan_fingerprint = confirmed_plan_fingerprint
        .ok_or_else(|| "--confirm-plan-fingerprint가 필요함".to_string())?;
    let receipt_dir = receipt_dir.ok_or_else(|| "--receipt-dir이 필요함".to_string())?;
    let approved_by = approved_by.ok_or_else(|| "--approved-by가 필요함".to_string())?;
    let rationale = rationale.ok_or_else(|| "--rationale이 필요함".to_string())?;
    for (flag, path) in [
        ("--source-root", source_root.as_path()),
        ("--destination-plan", destination_plan.as_path()),
        ("--receipt-dir", receipt_dir.as_path()),
    ] {
        if !absolute_without_parent(path) {
            return Err(format!("{flag}은 상위 탐색이 없는 절대 경로여야 함"));
        }
    }
    if !valid_hex64(&confirmed_plan_fingerprint) {
        return Err("--confirm-plan-fingerprint는 소문자 HEX64여야 함".into());
    }
    if !approved_by.starts_with("human:")
        || approved_by.len() <= "human:".len()
        || approved_by.len() > MAX_REVIEW_TEXT_BYTES
    {
        return Err("--approved-by는 human: 접두사의 명시적 attribution이어야 함".into());
    }
    if rationale.trim().is_empty() || rationale.len() > MAX_REVIEW_TEXT_BYTES {
        return Err("--rationale은 비어 있지 않은 bounded text여야 함".into());
    }
    if !execute {
        return Err("--execute 명시가 필요함".into());
    }
    if live_icloud_capacity == capacity_snapshot.is_some() {
        return Err("--live-icloud-capacity와 --capacity-snapshot 중 정확히 하나가 필요함".into());
    }
    if let Some(path) = &capacity_snapshot {
        if !absolute_without_parent(path) {
            return Err("--capacity-snapshot은 상위 탐색이 없는 절대 경로여야 함".into());
        }
    }
    Ok(Args {
        source_root,
        destination_plan,
        confirmed_plan_fingerprint,
        receipt_dir,
        approved_by,
        rationale,
        max_entries,
        stale_after_days,
        live_icloud_capacity,
        capacity_snapshot,
        execute,
    })
}

fn system_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn read_bounded_json<T: serde::de::DeserializeOwned>(
    path: &Path,
    max_bytes: u64,
    error_prefix: &str,
) -> Result<T, String> {
    let metadata =
        std::fs::symlink_metadata(path).map_err(|_| format!("{error_prefix}-unavailable"))?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() == 0
        || metadata.len() > max_bytes
    {
        return Err(format!("{error_prefix}-unsafe"));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|_| format!("{error_prefix}-open-failed"))?
        .take(max_bytes + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| format!("{error_prefix}-read-failed"))?;
    if bytes.len() as u64 > max_bytes {
        return Err(format!("{error_prefix}-too-large"));
    }
    serde_json::from_slice(&bytes).map_err(|_| format!("{error_prefix}-json-invalid"))
}

fn read_capacity_snapshot(path: &Path) -> Result<CloudCapacitySnapshot, String> {
    let value: serde_json::Value = read_bounded_json(
        path,
        MAX_CAPACITY_SNAPSHOT_BYTES,
        "materialization-execution-capacity-snapshot",
    )?;
    if let Ok(snapshot) = serde_json::from_value(value.clone()) {
        return Ok(snapshot);
    }
    let nested = value
        .pointer("/capacity/snapshot")
        .cloned()
        .ok_or_else(|| "materialization-execution-capacity-snapshot-json-invalid".to_string())?;
    serde_json::from_value(nested)
        .map_err(|_| "materialization-execution-capacity-snapshot-json-invalid".into())
}

fn verify_discovered_cloud_root(
    home: &Path,
    plan: &IncompleteDownloadDestinationPlan,
) -> Result<(), String> {
    let canonical_plan_root = std::fs::canonicalize(&plan.cloud_root)
        .map_err(|_| "materialization-execution-cloud-root-unavailable".to_string())?;
    let matches = discover_cloud_roots_report(home)
        .roots
        .into_iter()
        .filter(|root| {
            root.id == plan.cloud_root_id
                && root.provider == plan.provider
                && (root.account_scope == CloudAccountScope::Unknown
                    || root.account_scope == plan.account_scope)
                && std::fs::canonicalize(&root.path).is_ok_and(|path| path == canonical_plan_root)
        })
        .count();
    if matches != 1 {
        return Err("materialization-execution-cloud-root-not-uniquely-discovered".into());
    }
    Ok(())
}

fn run() -> Result<(), String> {
    let raw = std::env::args_os().skip(1).collect::<Vec<_>>();
    if raw.len() == 1
        && matches!(
            raw.first().map(OsString::as_os_str),
            Some(argument) if argument == OsStr::new("--help") || argument == OsStr::new("-h")
        )
    {
        println!("{}", usage());
        return Ok(());
    }
    let args = parse_args(&raw)?;
    let plan: IncompleteDownloadDestinationPlan = read_bounded_json(
        &args.destination_plan,
        MAX_PRIVATE_PLAN_BYTES,
        "materialization-execution-destination-plan",
    )?;
    if plan.destination_plan_fingerprint != args.confirmed_plan_fingerprint {
        return Err("materialization-execution-confirmed-plan-mismatch".into());
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "home-directory-unavailable".to_string())?;
    verify_discovered_cloud_root(&home, &plan)?;

    let capacity_observed_at_ms = system_now_ms();
    let capacity =
        if args.live_icloud_capacity {
            if plan.provider != CloudProvider::Icloud {
                return Err("live-icloud-capacity-requires-icloud-plan".into());
            }
            collect_icloud_native_capacity(capacity_observed_at_ms)?
        } else {
            read_capacity_snapshot(args.capacity_snapshot.as_deref().ok_or_else(|| {
                "materialization-execution-capacity-snapshot-missing".to_string()
            })?)?
        };

    let audit = collect_incomplete_download_audit(
        &args.source_root,
        system_now_ms(),
        args.max_entries,
        args.stale_after_days,
    )?;
    let recovery = validate_incomplete_download_recovery(
        &args.source_root,
        &audit,
        system_now_ms(),
        RecoveryValidationLimits::default(),
    )?;
    let materialization = plan_incomplete_download_materialization(
        &args.source_root,
        &audit,
        &recovery,
        system_now_ms(),
    )?;
    let approval = approve_incomplete_download_destination(
        &plan,
        &args.confirmed_plan_fingerprint,
        system_now_ms(),
        &args.approved_by,
        &args.rationale,
    )?;
    let (receipt, _) = execute_incomplete_download_materialization(
        &args.source_root,
        &materialization,
        &plan,
        &approval,
        &args.confirmed_plan_fingerprint,
        capacity,
        &args.receipt_dir,
        system_now_ms(),
    )?;
    println!(
        "{}",
        serde_json::to_string_pretty(&summarize_incomplete_download_materialization_receipt(
            &receipt
        ))
        .map_err(|error| error.to_string())?
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("DiskSage incomplete download materialization execution: {error}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn required() -> Vec<OsString> {
        vec![
            "--source-root".into(),
            "/source".into(),
            "--destination-plan".into(),
            "/private/plan.json".into(),
            "--confirm-plan-fingerprint".into(),
            "a".repeat(64).into(),
            "--receipt-dir".into(),
            "/private/receipts".into(),
            "--approved-by".into(),
            "human:test".into(),
            "--rationale".into(),
            "approved exact plan".into(),
            "--execute".into(),
        ]
    }

    #[test]
    fn parses_explicit_bounded_execution_arguments() {
        let mut raw = required();
        raw.push("--live-icloud-capacity".into());
        let parsed = parse_args(&raw).unwrap();
        assert!(parsed.execute);
        assert!(parsed.live_icloud_capacity);
        assert_eq!(parsed.confirmed_plan_fingerprint, "a".repeat(64));
    }

    #[test]
    fn rejects_missing_execute_bad_attribution_and_ambiguous_capacity() {
        let mut missing_execute = required();
        missing_execute.retain(|value| value != OsStr::new("--execute"));
        missing_execute.push("--live-icloud-capacity".into());
        assert!(parse_args(&missing_execute).is_err());

        let mut ambiguous = required();
        ambiguous.extend([
            "--live-icloud-capacity".into(),
            "--capacity-snapshot".into(),
            "/private/capacity.json".into(),
        ]);
        assert!(parse_args(&ambiguous).is_err());

        let mut bad_attribution = required();
        let position = bad_attribution
            .iter()
            .position(|value| value == OsStr::new("human:test"))
            .unwrap();
        bad_attribution[position] = "agent:test".into();
        bad_attribution.push("--live-icloud-capacity".into());
        assert!(parse_args(&bad_attribution).is_err());
    }
}
