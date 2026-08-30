use disksage_lib::allocation_map::{measure_root, AllocationMapEntry};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn incomplete_report(root: &Path, reason: &'static str) -> AllocationMapEntry {
    AllocationMapEntry {
        root: root.to_string_lossy().into_owned(),
        allocated_bytes: 0,
        visited_entries: 0,
        classification: "unmeasured",
        evidence_complete: false,
        stop_reason: Some(reason),
    }
}

fn main() {
    let mut args = std::env::args_os().skip(1);
    let max_entries = args
        .next()
        .and_then(|value| value.to_str()?.parse::<u64>().ok());
    let max_duration_ms = args
        .next()
        .and_then(|value| value.to_str()?.parse::<u64>().ok());
    let roots = args.map(PathBuf::from).collect::<Vec<_>>();
    if max_entries.is_none()
        || max_duration_ms.is_none()
        || roots.is_empty()
        || roots.iter().any(|root| !root.is_absolute())
    {
        eprintln!("usage: disksage-allocation-map MAX_ENTRIES MAX_DURATION_MS ABSOLUTE_ROOT [ABSOLUTE_ROOT ...]");
        std::process::exit(2);
    }
    let max_entries = max_entries.unwrap();
    let max_duration = Duration::from_millis(max_duration_ms.unwrap());
    if max_entries == 0 || max_duration.is_zero() {
        eprintln!("disksage-allocation-map: allocation-map-options-invalid");
        std::process::exit(2);
    }

    let started = Instant::now();
    let mut remaining_entries = max_entries;
    let mut reports = Vec::with_capacity(roots.len());
    for root in &roots {
        if remaining_entries == 0 {
            reports.push(incomplete_report(root, "entry-limit-reached"));
            continue;
        }
        let remaining_duration = max_duration.saturating_sub(started.elapsed());
        if remaining_duration.is_zero() {
            reports.push(incomplete_report(root, "duration-limit-reached"));
            continue;
        }

        let report = measure_root(root, remaining_entries, remaining_duration).unwrap_or_else(|error| {
            eprintln!("disksage-allocation-map: {error}");
            std::process::exit(1);
        });
        remaining_entries = remaining_entries.saturating_sub(report.visited_entries);
        if report.stop_reason == Some("entry-limit-reached") {
            remaining_entries = 0;
        }
        reports.push(report);
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&reports).expect("serializable report")
    );
}
