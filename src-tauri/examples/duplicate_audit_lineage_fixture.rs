//! Generate a synthetic, path-free v1 duplicate-audit lineage envelope for contract testing.
//!
//! All source writes stay inside a temporary directory. The JSON emitted to stdout contains only
//! opaque references, checked byte arithmetic, and source-stability metadata.

use disksage_lib::duplicate_audit::{audit_duplicates, DuplicateAuditOptions};
use disksage_lib::naruon_duplicate_audit_lineage::export_naruon_duplicate_audit_lineage;

fn main() -> Result<(), String> {
    let source = tempfile::tempdir().map_err(|error| error.to_string())?;
    let payload = b"synthetic duplicate-audit lineage fixture";
    std::fs::write(source.path().join("private-first.bin"), payload)
        .map_err(|error| error.to_string())?;
    std::fs::write(source.path().join("private-second.bin"), payload)
        .map_err(|error| error.to_string())?;

    let options = DuplicateAuditOptions {
        min_file_bytes: 1,
        prefix_bytes: 8,
        max_entries: 100,
        max_duration_ms: 10_000,
        max_files_to_hash: 100,
        max_size_groups: 100,
        max_hash_bytes: 10_000_000,
    };
    let report = audit_duplicates(source.path(), &options, 100)?;
    let envelope = export_naruon_duplicate_audit_lineage(&report, 101)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&envelope).map_err(|error| error.to_string())?
    );
    Ok(())
}
