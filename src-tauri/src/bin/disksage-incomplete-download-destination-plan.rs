use disksage_lib::cloud::{discover_cloud_roots_report, CloudProvider, CloudRoot};
use disksage_lib::incomplete_download::{
    collect_incomplete_download_audit, DEFAULT_MAX_ENTRIES, DEFAULT_STALE_AFTER_DAYS,
    MAX_STALE_AFTER_DAYS,
};
use disksage_lib::incomplete_download_materialization::plan_incomplete_download_materialization;
use disksage_lib::incomplete_download_materialization_destination::{
    plan_incomplete_download_destination, summarize_incomplete_download_destination,
};
use disksage_lib::incomplete_download_recovery::{
    validate_incomplete_download_recovery, RecoveryValidationLimits,
};
use disksage_lib::private_evidence::write_private_json_create_new;
use disksage_lib::provider_capacity::{
    collect_icloud_native_capacity, CloudCapacitySnapshot, DEFAULT_CAPACITY_RESERVE_BYTES,
};
use std::ffi::OsString;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

const MAX_CAPACITY_SNAPSHOT_BYTES: u64 = 64 * 1024;
const MAX_CAPACITY_RESERVE_MIB: u64 = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    source_root: PathBuf,
    cloud_root: PathBuf,
    destination_subdirectory: PathBuf,
    max_entries: usize,
    stale_after_days: u64,
    reserve_mib: u64,
    live_icloud_capacity: bool,
    capacity_snapshot: Option<PathBuf>,
    private_output: Option<PathBuf>,
}

fn absolute_without_parent(path: &Path) -> bool {
    path.is_absolute()
        && !path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
}

fn safe_relative_directory(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn usage() -> String {
    format!(
        "usage: disksage-incomplete-download-destination-plan \
         --source-root ABSOLUTE_PATH --cloud-root ABSOLUTE_PATH \
         --destination-subdirectory RELATIVE_PATH \
         (--live-icloud-capacity | --capacity-snapshot ABSOLUTE.json) \
         [--max-entries 1..={DEFAULT_MAX_ENTRIES}] \
         [--stale-after-days 1..={MAX_STALE_AFTER_DAYS}] \
         [--capacity-reserve-mib 0..={MAX_CAPACITY_RESERVE_MIB}] \
         [--private-output ABSOLUTE_NEW_FILE.json]"
    )
}

fn native_value(raw: &[OsString], index: &mut usize, flag: &str) -> Result<OsString, String> {
    *index += 1;
    raw.get(*index)
        .cloned()
        .ok_or_else(|| format!("{flag} 값이 필요함"))
}

fn text_value(raw: &[OsString], index: &mut usize, flag: &str) -> Result<String, String> {
    native_value(raw, index, flag)?
        .into_string()
        .map_err(|_| "incomplete-download-destination-plan-invalid-utf8-argument".to_string())
}

fn parse_args_os(raw: &[OsString]) -> Result<Args, String> {
    let mut source_root = None;
    let mut cloud_root = None;
    let mut destination_subdirectory = None;
    let mut max_entries = DEFAULT_MAX_ENTRIES;
    let mut max_entries_seen = false;
    let mut stale_after_days = DEFAULT_STALE_AFTER_DAYS;
    let mut stale_after_days_seen = false;
    let mut reserve_mib = DEFAULT_CAPACITY_RESERVE_BYTES / (1024 * 1024);
    let mut reserve_mib_seen = false;
    let mut live_icloud_capacity = false;
    let mut capacity_snapshot = None;
    let mut private_output = None;
    let mut index = 0usize;
    while index < raw.len() {
        match raw[index].to_str() {
            Some("--source-root") => {
                if source_root.is_some() {
                    return Err("--source-root는 한 번만 지정할 수 있음".into());
                }
                source_root = Some(PathBuf::from(native_value(raw, &mut index, "--source-root")?));
            }
            Some("--cloud-root") => {
                if cloud_root.is_some() {
                    return Err("--cloud-root는 한 번만 지정할 수 있음".into());
                }
                cloud_root = Some(PathBuf::from(native_value(raw, &mut index, "--cloud-root")?));
            }
            Some("--destination-subdirectory") => {
                if destination_subdirectory.is_some() {
                    return Err("--destination-subdirectory는 한 번만 지정할 수 있음".into());
                }
                destination_subdirectory = Some(PathBuf::from(text_value(
                    raw,
                    &mut index,
                    "--destination-subdirectory",
                )?));
            }
            Some("--max-entries") => {
                if max_entries_seen {
                    return Err("--max-entries는 한 번만 지정할 수 있음".into());
                }
                max_entries_seen = true;
                let parsed = text_value(raw, &mut index, "--max-entries")?
                    .parse::<usize>()
                    .map_err(|_| "--max-entries는 양의 정수여야 함".to_string())?;
                if parsed == 0 || parsed > DEFAULT_MAX_ENTRIES {
                    return Err(format!(
                        "--max-entries는 1..={DEFAULT_MAX_ENTRIES} 범위여야 함"
                    ));
                }
                max_entries = parsed;
            }
            Some("--stale-after-days") => {
                if stale_after_days_seen {
                    return Err("--stale-after-days는 한 번만 지정할 수 있음".into());
                }
                stale_after_days_seen = true;
                let parsed = text_value(raw, &mut index, "--stale-after-days")?
                    .parse::<u64>()
                    .map_err(|_| "--stale-after-days는 양의 정수여야 함".to_string())?;
                if !(1..=MAX_STALE_AFTER_DAYS).contains(&parsed) {
                    return Err(format!(
                        "--stale-after-days는 1..={MAX_STALE_AFTER_DAYS} 범위여야 함"
                    ));
                }
                stale_after_days = parsed;
            }
            Some("--capacity-reserve-mib") => {
                if reserve_mib_seen {
                    return Err("--capacity-reserve-mib는 한 번만 지정할 수 있음".into());
                }
                reserve_mib_seen = true;
                let parsed = text_value(raw, &mut index, "--capacity-reserve-mib")?
                    .parse::<u64>()
                    .map_err(|_| "--capacity-reserve-mib는 정수여야 함".to_string())?;
                if parsed > MAX_CAPACITY_RESERVE_MIB {
                    return Err(format!(
                        "--capacity-reserve-mib는 0..={MAX_CAPACITY_RESERVE_MIB} 범위여야 함"
                    ));
                }
                reserve_mib = parsed;
            }
            Some("--live-icloud-capacity") => {
                if live_icloud_capacity {
                    return Err("--live-icloud-capacity는 한 번만 지정할 수 있음".into());
                }
                live_icloud_capacity = true;
            }
            Some("--capacity-snapshot") => {
                if capacity_snapshot.is_some() {
                    return Err("--capacity-snapshot은 한 번만 지정할 수 있음".into());
                }
                capacity_snapshot = Some(PathBuf::from(native_value(
                    raw,
                    &mut index,
                    "--capacity-snapshot",
                )?));
            }
            Some("--private-output") => {
                if private_output.is_some() {
                    return Err("--private-output은 한 번만 지정할 수 있음".into());
                }
                private_output = Some(PathBuf::from(native_value(raw, &mut index, "--private-output")?));
            }
            Some("--help" | "-h") => return Err(usage()),
            Some(_) => {
                return Err("incomplete-download-destination-plan-unknown-argument".into())
            }
            None => {
                return Err("incomplete-download-destination-plan-invalid-utf8-argument".into())
            }
        }
        index += 1;
    }

    let source_root = source_root.ok_or_else(|| "--source-root가 필요함".to_string())?;
    let cloud_root = cloud_root.ok_or_else(|| "--cloud-root가 필요함".to_string())?;
    let destination_subdirectory = destination_subdirectory
        .ok_or_else(|| "--destination-subdirectory가 필요함".to_string())?;
    if !absolute_without_parent(&source_root) {
        return Err("--source-root는 상위 탐색이 없는 절대 경로여야 함".into());
    }
    if !absolute_without_parent(&cloud_root) {
        return Err("--cloud-root는 상위 탐색이 없는 절대 경로여야 함".into());
    }
    if !safe_relative_directory(&destination_subdirectory) {
        return Err("--destination-subdirectory는 안전한 상대 경로여야 함".into());
    }
    if live_icloud_capacity == capacity_snapshot.is_some() {
        return Err("--live-icloud-capacity와 --capacity-snapshot 중 정확히 하나가 필요함".into());
    }
    if let Some(path) = &capacity_snapshot {
        if !absolute_without_parent(path) {
            return Err("--capacity-snapshot은 상위 탐색이 없는 절대 경로여야 함".into());
        }
    }
    if let Some(path) = &private_output {
        if !absolute_without_parent(path) {
            return Err("--private-output은 상위 탐색이 없는 절대 경로여야 함".into());
        }
    }
    Ok(Args {
        source_root,
        cloud_root,
        destination_subdirectory,
        max_entries,
        stale_after_days,
        reserve_mib,
        live_icloud_capacity,
        capacity_snapshot,
        private_output,
    })
}

#[cfg(test)]
fn parse_args(raw: &[String]) -> Result<Args, String> {
    let native = raw.iter().map(OsString::from).collect::<Vec<_>>();
    parse_args_os(&native)
}

fn system_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn select_discovered_root(home: &Path, requested: &Path) -> Result<CloudRoot, String> {
    let canonical_requested = std::fs::canonicalize(requested)
        .map_err(|_| "materialization-cloud-root-unavailable".to_string())?;
    let mut matches = discover_cloud_roots_report(home)
        .roots
        .into_iter()
        .filter(|root| {
            std::fs::canonicalize(&root.path)
                .map(|path| path == canonical_requested)
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err("materialization-cloud-root-not-uniquely-discovered".into());
    }
    Ok(matches.remove(0))
}

fn read_capacity_snapshot(path: &Path) -> Result<CloudCapacitySnapshot, String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| "materialization-capacity-snapshot-unavailable".to_string())?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() == 0
        || metadata.len() > MAX_CAPACITY_SNAPSHOT_BYTES
    {
        return Err("materialization-capacity-snapshot-unsafe".into());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|_| "materialization-capacity-snapshot-open-failed".to_string())?
        .take(MAX_CAPACITY_SNAPSHOT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "materialization-capacity-snapshot-read-failed".to_string())?;
    if bytes.len() as u64 > MAX_CAPACITY_SNAPSHOT_BYTES {
        return Err("materialization-capacity-snapshot-too-large".into());
    }
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|_| "materialization-capacity-snapshot-json-invalid".to_string())?;
    if let Ok(snapshot) = serde_json::from_value(value.clone()) {
        return Ok(snapshot);
    }
    let nested = value
        .pointer("/capacity/snapshot")
        .cloned()
        .ok_or_else(|| "materialization-capacity-snapshot-json-invalid".to_string())?;
    serde_json::from_value(nested)
        .map_err(|_| "materialization-capacity-snapshot-json-invalid".into())
}

fn run() -> Result<(), String> {
    let raw = std::env::args_os().skip(1).collect::<Vec<_>>();
    if raw.len() == 1 && matches!(raw[0].to_str(), Some("--help" | "-h")) {
        println!("{}", usage());
        return Ok(());
    }
    let args = parse_args_os(&raw)?;
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "home-directory-unavailable".to_string())?;
    let cloud_root = select_discovered_root(&home, &args.cloud_root)?;
    let capacity_observed_at_ms = system_now_ms();
    let capacity = if args.live_icloud_capacity {
        if cloud_root.provider != CloudProvider::Icloud {
            return Err("live-icloud-capacity-requires-icloud-root".into());
        }
        collect_icloud_native_capacity(capacity_observed_at_ms)?
    } else {
        read_capacity_snapshot(
            args.capacity_snapshot
                .as_deref()
                .ok_or_else(|| "materialization-capacity-snapshot-missing".to_string())?,
        )?
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
    let destination = plan_incomplete_download_destination(
        &materialization,
        &cloud_root,
        &args.destination_subdirectory.to_string_lossy(),
        capacity,
        args.reserve_mib.saturating_mul(1024 * 1024),
        system_now_ms(),
    )?;
    let mut summary = serde_json::to_value(summarize_incomplete_download_destination(&destination))
        .map_err(|error| error.to_string())?;
    if let Some(path) = &args.private_output {
        let receipt = write_private_json_create_new(&args.source_root, path, &destination)?;
        summary
            .as_object_mut()
            .ok_or_else(|| {
                "incomplete download destination summary JSON object가 아님".to_string()
            })?
            .insert(
                "private_output".into(),
                serde_json::to_value(receipt).map_err(|error| error.to_string())?,
            );
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&summary).map_err(|error| error.to_string())?
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("DiskSage incomplete download destination plan: {error}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn required_prefix() -> Vec<String> {
        vec![
            "--source-root".into(),
            "/source".into(),
            "--cloud-root".into(),
            "/cloud".into(),
            "--destination-subdirectory".into(),
            "DiskSage/Recovered".into(),
        ]
    }

    #[test]
    fn parses_bounded_live_capacity_arguments() {
        let mut raw = required_prefix();
        raw.extend([
            "--live-icloud-capacity".into(),
            "--max-entries".into(),
            "42".into(),
            "--stale-after-days".into(),
            "60".into(),
            "--capacity-reserve-mib".into(),
            "2048".into(),
            "--private-output".into(),
            "/private/destination.json".into(),
        ]);
        let args = parse_args(&raw).unwrap();
        assert_eq!(args.source_root, PathBuf::from("/source"));
        assert_eq!(args.cloud_root, PathBuf::from("/cloud"));
        assert_eq!(
            args.destination_subdirectory,
            PathBuf::from("DiskSage/Recovered")
        );
        assert!(args.live_icloud_capacity);
        assert_eq!(args.max_entries, 42);
        assert_eq!(args.stale_after_days, 60);
        assert_eq!(args.reserve_mib, 2048);
    }

    #[test]
    fn rejects_missing_ambiguous_relative_and_unbounded_arguments() {
        let mut no_capacity = required_prefix();
        let mut both_capacity = required_prefix();
        both_capacity.extend([
            "--live-icloud-capacity".into(),
            "--capacity-snapshot".into(),
            "/capacity.json".into(),
        ]);
        let mut unsafe_destination = required_prefix();
        unsafe_destination[5] = "../escape".into();
        unsafe_destination.push("--live-icloud-capacity".into());
        let mut relative_source = required_prefix();
        relative_source[1] = "relative".into();
        relative_source.push("--live-icloud-capacity".into());
        let mut unbounded = required_prefix();
        unbounded.extend([
            "--live-icloud-capacity".into(),
            "--max-entries".into(),
            "0".into(),
        ]);
        for raw in [
            Vec::new(),
            std::mem::take(&mut no_capacity),
            both_capacity,
            unsafe_destination,
            relative_source,
            unbounded,
        ] {
            assert!(parse_args(&raw).is_err(), "{raw:?}");
        }
    }
}
