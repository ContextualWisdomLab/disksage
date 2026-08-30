//! Read-only deleted-open file audit.

fn main() {
    let observed_at_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default();
    match disksage_lib::deleted_open::collect_deleted_open_audit().and_then(|report| {
        let plan = disksage_lib::deleted_open::plan_from_audit(report, observed_at_ms);
        serde_json::to_string_pretty(&plan).map_err(|_| "deleted-open-audit-encode-failed".into())
    }) {
        Ok(report) => println!("{report}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }
}
