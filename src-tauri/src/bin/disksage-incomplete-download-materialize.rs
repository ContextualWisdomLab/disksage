use disksage_lib::cloud::{
    discover_cloud_roots_report, CloudAccountScope, CloudProvider, CloudRoot,
};
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
use disksage_lib::provider_capacity::{
    collect_icloud_native_capacity, collect_live_root_capacity, CloudCapacitySnapshot,
};
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
    live_provider_capacity: bool,
    oauth_connections: Option<PathBuf>,
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
         (--live-icloud-capacity | \
          --live-provider-capacity [--oauth-connections ABSOLUTE.json] | \
          --capacity-snapshot ABSOLUTE.json) \
         [--max-entries 1..={DEFAULT_MAX_ENTRIES}] \
         [--stale-after-days 1..={MAX_STALE_AFTER_DAYS}]"
    )
}

fn parse_args(raw: &[String]) -> Result<Args, String> {
    let mut source_root = None;
    let mut destination_plan = None;
    let mut confirmed_plan_fingerprint = None;
    let mut receipt_dir = None;
    let mut approved_by = None;
    let mut rationale = None;
    let mut max_entries = DEFAULT_MAX_ENTRIES;
    let mut stale_after_days = DEFAULT_STALE_AFTER_DAYS;
    let mut live_icloud_capacity = false;
    let mut live_provider_capacity = false;
    let mut oauth_connections = None;
    let mut capacity_snapshot = None;
    let mut execute = false;
    let mut index = 0usize;
    while index < raw.len() {
        let value = |index: &mut usize, flag: &str| -> Result<String, String> {
            *index += 1;
            raw.get(*index)
                .cloned()
                .ok_or_else(|| format!("{flag} 값이 필요함"))
        };
        match raw[index].as_str() {
            "--source-root" => {
                if source_root.is_some() {
                    return Err("--source-root는 한 번만 지정할 수 있음".into());
                }
                source_root = Some(PathBuf::from(value(&mut index, "--source-root")?));
            }
            "--destination-plan" => {
                if destination_plan.is_some() {
                    return Err("--destination-plan은 한 번만 지정할 수 있음".into());
                }
                destination_plan = Some(PathBuf::from(value(&mut index, "--destination-plan")?));
            }
            "--confirm-plan-fingerprint" => {
                if confirmed_plan_fingerprint.is_some() {
                    return Err("--confirm-plan-fingerprint는 한 번만 지정할 수 있음".into());
                }
                confirmed_plan_fingerprint = Some(value(&mut index, "--confirm-plan-fingerprint")?);
            }
            "--receipt-dir" => {
                if receipt_dir.is_some() {
                    return Err("--receipt-dir은 한 번만 지정할 수 있음".into());
                }
                receipt_dir = Some(PathBuf::from(value(&mut index, "--receipt-dir")?));
            }
            "--approved-by" => {
                if approved_by.is_some() {
                    return Err("--approved-by는 한 번만 지정할 수 있음".into());
                }
                approved_by = Some(value(&mut index, "--approved-by")?);
            }
            "--rationale" => {
                if rationale.is_some() {
                    return Err("--rationale은 한 번만 지정할 수 있음".into());
                }
                rationale = Some(value(&mut index, "--rationale")?);
            }
            "--max-entries" => {
                let parsed = value(&mut index, "--max-entries")?
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
                let parsed = value(&mut index, "--stale-after-days")?
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
            "--live-provider-capacity" => {
                if live_provider_capacity {
                    return Err("--live-provider-capacity는 한 번만 지정할 수 있음".into());
                }
                live_provider_capacity = true;
            }
            "--oauth-connections" => {
                if oauth_connections.is_some() {
                    return Err("--oauth-connections는 한 번만 지정할 수 있음".into());
                }
                oauth_connections = Some(PathBuf::from(value(&mut index, "--oauth-connections")?));
            }
            "--capacity-snapshot" => {
                if capacity_snapshot.is_some() {
                    return Err("--capacity-snapshot은 한 번만 지정할 수 있음".into());
                }
                capacity_snapshot = Some(PathBuf::from(value(&mut index, "--capacity-snapshot")?));
            }
            "--execute" => {
                if execute {
                    return Err("--execute는 한 번만 지정할 수 있음".into());
                }
                execute = true;
            }
            "--help" | "-h" => return Err(usage()),
            flag => return Err(format!("알 수 없는 인자: {flag}")),
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
    let capacity_modes = usize::from(live_icloud_capacity)
        + usize::from(live_provider_capacity)
        + usize::from(capacity_snapshot.is_some());
    if capacity_modes != 1 {
        return Err("live iCloud, live provider, capacity snapshot 중 정확히 하나가 필요함".into());
    }
    if oauth_connections.is_some() && !live_provider_capacity {
        return Err("--oauth-connections에는 --live-provider-capacity가 필요함".into());
    }
    for (flag, path) in [
        ("--capacity-snapshot", capacity_snapshot.as_ref()),
        ("--oauth-connections", oauth_connections.as_ref()),
    ] {
        let Some(path) = path else {
            continue;
        };
        if !absolute_without_parent(path) {
            return Err(format!("{flag}은 상위 탐색이 없는 절대 경로여야 함"));
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
        live_provider_capacity,
        oauth_connections,
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
) -> Result<CloudRoot, String> {
    let canonical_plan_root = std::fs::canonicalize(&plan.cloud_root)
        .map_err(|_| "materialization-execution-cloud-root-unavailable".to_string())?;
    let mut matches = discover_cloud_roots_report(home)
        .roots
        .into_iter()
        .filter(|root| {
            root.id == plan.cloud_root_id
                && root.provider == plan.provider
                && (root.account_scope == CloudAccountScope::Unknown
                    || root.account_scope == plan.account_scope)
                && std::fs::canonicalize(&root.path).is_ok_and(|path| path == canonical_plan_root)
        })
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err("materialization-execution-cloud-root-not-uniquely-discovered".into());
    }
    Ok(matches.remove(0))
}

#[cfg(not(coverage))]
fn run() -> Result<(), String> {
    let args = parse_args(&std::env::args().skip(1).collect::<Vec<_>>())?;
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
    let cloud_root = verify_discovered_cloud_root(&home, &plan)?;

    let capacity_observed_at_ms = system_now_ms();
    let capacity =
        if args.live_icloud_capacity {
            if plan.provider != CloudProvider::Icloud {
                return Err("live-icloud-capacity-requires-icloud-plan".into());
            }
            collect_icloud_native_capacity(capacity_observed_at_ms)?
        } else if args.live_provider_capacity {
            collect_live_root_capacity(
                &cloud_root,
                args.oauth_connections.as_deref(),
                capacity_observed_at_ms,
            )?
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

#[cfg(not(coverage))]
fn main() {
    if let Err(error) = run() {
        eprintln!("DiskSage incomplete download materialization execution: {error}");
        std::process::exit(2);
    }
}

#[cfg(coverage)]
fn main() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn required() -> Vec<String> {
        vec![
            "--source-root".into(),
            "/source".into(),
            "--destination-plan".into(),
            "/private/plan.json".into(),
            "--confirm-plan-fingerprint".into(),
            "a".repeat(64),
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
        assert!(!parsed.live_provider_capacity);
        assert_eq!(parsed.confirmed_plan_fingerprint, "a".repeat(64));
    }

    #[test]
    fn parses_live_multicloud_execution_capacity_with_oauth_document() {
        let mut raw = required();
        raw.extend([
            "--live-provider-capacity".into(),
            "--oauth-connections".into(),
            "/private/oauth-connections.json".into(),
        ]);
        let parsed = parse_args(&raw).unwrap();
        assert!(!parsed.live_icloud_capacity);
        assert!(parsed.live_provider_capacity);
        assert_eq!(
            parsed.oauth_connections,
            Some(PathBuf::from("/private/oauth-connections.json"))
        );
        assert!(parsed.capacity_snapshot.is_none());
    }

    #[test]
    fn rejects_missing_execute_bad_attribution_and_ambiguous_capacity() {
        let mut missing_execute = required();
        missing_execute.retain(|value| value != "--execute");
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
            .position(|value| value == "human:test")
            .unwrap();
        bad_attribution[position] = "agent:test".into();
        bad_attribution.push("--live-icloud-capacity".into());
        assert!(parse_args(&bad_attribution).is_err());

        let mut oauth_without_live_provider = required();
        oauth_without_live_provider.extend([
            "--live-icloud-capacity".into(),
            "--oauth-connections".into(),
            "/private/oauth-connections.json".into(),
        ]);
        assert!(parse_args(&oauth_without_live_provider).is_err());

        let mut relative_oauth = required();
        relative_oauth.extend([
            "--live-provider-capacity".into(),
            "--oauth-connections".into(),
            "relative.json".into(),
        ]);
        assert!(parse_args(&relative_oauth).is_err());
    }
}
