use disksage_lib::allocation_map::measure_root;
use std::path::PathBuf;
use std::time::Duration;

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
    let reports = roots
        .iter()
        .map(|root| measure_root(root, max_entries, max_duration))
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_else(|error| {
            eprintln!("disksage-allocation-map: {error}");
            std::process::exit(1);
        });
    println!(
        "{}",
        serde_json::to_string_pretty(&reports).expect("serializable report")
    );
}
