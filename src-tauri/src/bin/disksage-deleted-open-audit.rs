//! Read-only deleted-open file audit.

fn main() {
    match disksage_lib::deleted_open::collect_deleted_open_audit().and_then(|report| {
        serde_json::to_string_pretty(&report).map_err(|_| "deleted-open-audit-encode-failed".into())
    }) {
        Ok(report) => println!("{report}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }
}
