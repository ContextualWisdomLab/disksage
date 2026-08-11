//! Edge-contract coverage for bounded dataset metadata profiling.
//!
//! These fixtures exercise reachable production branches that are easy to miss in ordinary
//! examples while keeping all sampled values ephemeral and out of the returned profile.

use disksage_lib::profile_dataset;
use std::io::Write;

fn write_file(path: &std::path::Path, bytes: &[u8]) {
    let mut file = std::fs::File::create(path).unwrap();
    file.write_all(bytes).unwrap();
}

#[test]
fn header_only_and_temporal_columns_preserve_unknown_and_datetime_semantics() {
    let temp = tempfile::tempdir().unwrap();

    let header_only = temp.path().join("header-only.csv");
    write_file(&header_only, b"value\n");
    let profile = profile_dataset(&header_only);
    assert_eq!(profile.sampled_rows, 0);
    assert_eq!(profile.columns.len(), 1);
    assert_eq!(profile.columns[0].inferred_type, "unknown");
    assert!(!profile.profile_complete);
    assert!(profile
        .quality_warnings
        .contains(&"no-data-rows".to_string()));

    let temporal = temp.path().join("temporal.csv");
    write_file(
        &temporal,
        b"when,reverse_number,sticky_mixed\n2026-01-01,1.5,true\n2026-01-01 12:00:00,1,1\n2026-01-02T03:04:05Z,2,text\n",
    );
    let profile = profile_dataset(&temporal);
    assert!(profile.profile_complete);
    assert_eq!(profile.columns[0].inferred_type, "datetime");
    assert_eq!(profile.columns[1].inferred_type, "number");
    assert_eq!(profile.columns[2].inferred_type, "mixed");
}

#[test]
fn delimited_profile_bounds_wide_and_long_schema_without_leaking_values() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("wide.csv");

    let mut headers = Vec::new();
    headers.push(format!("customer.account.{}", "x".repeat(160)));
    for index in 1..129 {
        headers.push(format!("field_{index}"));
    }
    let values = (0..129)
        .map(|index| format!("private-value-{index}"))
        .collect::<Vec<_>>();
    let contents = format!("{}\n{}\n", headers.join(","), values.join(","));
    write_file(&path, contents.as_bytes());

    let profile = profile_dataset(&path);
    assert_eq!(profile.columns.len(), 128);
    assert!(!profile.profile_complete);
    assert!(profile
        .quality_warnings
        .contains(&"column-limit-exceeded".to_string()));
    assert!(profile
        .quality_warnings
        .contains(&"sensitive-column-name-detected".to_string()));
    assert_eq!(profile.columns[0].name.chars().count(), 128);
    assert!(profile.columns[0].sensitive_name);

    let serialized = serde_json::to_string(&profile).unwrap();
    assert!(!serialized.contains("private-value-0"));
    assert!(!serialized.contains("private-value-128"));
}

#[test]
fn whitespace_cells_and_invalid_date_shapes_fail_to_text_without_false_dates() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("date-shapes.csv");
    write_file(
        &path,
        b"blank,bad_month,bad_day,bad_shape\n\"   \",2026-13-01,2026-01-32,2026/01/01\nvalue,2026-12-01,2026-01-31,2026-01-01\n",
    );

    let profile = profile_dataset(&path);
    assert!(profile.profile_complete);
    assert_eq!(profile.columns[0].missing_values, 1);
    assert_eq!(profile.columns[0].inferred_type, "text");
    assert_eq!(profile.columns[1].inferred_type, "mixed");
    assert_eq!(profile.columns[2].inferred_type, "mixed");
    assert_eq!(profile.columns[3].inferred_type, "mixed");
}
