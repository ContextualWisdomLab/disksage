//! Edge-contract coverage for bounded dataset metadata profiling.
//!
//! These fixtures exercise reachable production branches that are easy to miss in ordinary
//! examples while keeping all sampled values ephemeral and out of the returned profile.

#![cfg(not(coverage))]

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

#[test]
fn valid_dates_and_digit_invalid_shapes_cover_exact_date_admission() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("exact-date.csv");
    write_file(
        &path,
        b"date,non_digit_month,non_digit_day\n2026-01-31,2026-aa-01,2026-01-xy\n",
    );

    let profile = profile_dataset(&path);
    assert!(profile.profile_complete);
    assert_eq!(profile.columns[0].inferred_type, "date");
    assert_eq!(profile.columns[1].inferred_type, "text");
    assert_eq!(profile.columns[2].inferred_type, "text");
}

#[test]
fn jsonl_row_limit_stops_before_unbounded_input_consumption() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("bounded.jsonl");
    let mut contents = String::new();
    for index in 0..101 {
        contents.push_str(&format!("{{\"id\":{index}}}\n"));
    }
    write_file(&path, contents.as_bytes());

    let profile = profile_dataset(&path);
    assert_eq!(profile.sampled_rows, 100);
    assert!(profile.sample_truncated);
    assert!(!profile.profile_complete);
    assert!(profile
        .quality_warnings
        .contains(&"row-sample-limit-reached".to_string()));
    assert_eq!(profile.columns.len(), 1);
    assert_eq!(profile.columns[0].observed_values, 100);
}

#[test]
fn jsonl_column_limit_and_empty_names_fail_closed_without_value_retention() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("wide.jsonl");
    let mut object = serde_json::Map::new();
    object.insert(String::new(), serde_json::Value::String("private-empty".into()));
    for index in 0..128 {
        object.insert(
            format!("field_{index}"),
            serde_json::Value::String(format!("private-value-{index}")),
        );
    }
    let contents = format!("{}\n", serde_json::Value::Object(object));
    write_file(&path, contents.as_bytes());

    let profile = profile_dataset(&path);
    assert_eq!(profile.sampled_rows, 1);
    assert_eq!(profile.columns.len(), 128);
    assert!(!profile.profile_complete);
    assert!(profile
        .quality_warnings
        .contains(&"column-limit-exceeded".to_string()));
    assert!(profile.columns.iter().any(|column| column.name == "column_1"));

    let serialized = serde_json::to_string(&profile).unwrap();
    assert!(!serialized.contains("private-empty"));
    assert!(!serialized.contains("private-value-0"));
    assert!(!serialized.contains("private-value-127"));
}

#[test]
fn jsonl_late_columns_and_reverse_type_transitions_preserve_bounded_evidence() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("late-columns.jsonl");
    write_file(
        &path,
        b"{\"when\":\"2026-01-01T10:00:00Z\",\"mixed\":true,\"stable\":1}\n{\"when\":\"2026-01-02\",\"mixed\":{},\"late\":false}\n{\"when\":null,\"mixed\":7,\"late\":true}\n",
    );

    let profile = profile_dataset(&path);
    assert!(profile.profile_complete);
    assert_eq!(profile.sampled_rows, 3);

    let when = profile.columns.iter().find(|column| column.name == "when").unwrap();
    assert_eq!(when.inferred_type, "datetime");
    assert_eq!(when.observed_values, 2);
    assert_eq!(when.missing_values, 1);

    let mixed = profile.columns.iter().find(|column| column.name == "mixed").unwrap();
    assert_eq!(mixed.inferred_type, "mixed");
    assert_eq!(mixed.observed_values, 3);

    let stable = profile.columns.iter().find(|column| column.name == "stable").unwrap();
    assert_eq!(stable.observed_values, 1);
    assert_eq!(stable.missing_values, 2);

    let late = profile.columns.iter().find(|column| column.name == "late").unwrap();
    assert_eq!(late.inferred_type, "boolean");
    assert_eq!(late.observed_values, 2);
    assert_eq!(late.missing_values, 1);

    let serialized = serde_json::to_string(&profile).unwrap();
    assert!(!serialized.contains("2026-01-01T10:00:00Z"));
    assert!(!serialized.contains("2026-01-02"));
}

#[test]
fn public_profile_rejects_unsupported_and_missing_inputs_without_reading_them() {
    let temp = tempfile::tempdir().unwrap();

    let unsupported = temp.path().join("private.parquet");
    write_file(&unsupported, b"private-payload-must-not-be-read");
    let profile = profile_dataset(&unsupported);
    assert_eq!(profile.format, "parquet");
    assert!(!profile.profile_complete);
    assert_eq!(profile.sampled_rows, 0);
    assert_eq!(profile.quality_warnings, vec!["unsupported-dataset-format"]);
    assert!(!serde_json::to_string(&profile)
        .unwrap()
        .contains("private-payload-must-not-be-read"));

    let extensionless = temp.path().join("private-dataset");
    let profile = profile_dataset(&extensionless);
    assert_eq!(profile.format, "unknown");
    assert_eq!(profile.quality_warnings, vec!["unsupported-dataset-format"]);

    let missing_csv = temp.path().join("missing.csv");
    let profile = profile_dataset(&missing_csv);
    assert_eq!(profile.format, "csv");
    assert!(!profile.profile_complete);
    assert_eq!(profile.quality_warnings, vec!["dataset-open-error"]);
}

#[test]
fn tsv_and_malformed_csv_headers_exercise_public_delimited_dispatch_fail_closed() {
    let temp = tempfile::tempdir().unwrap();

    let tsv = temp.path().join("sample.tsv");
    write_file(&tsv, b"id\tactive\n1\ttrue\n2\tfalse\n");
    let profile = profile_dataset(&tsv);
    assert_eq!(profile.format, "tsv");
    assert!(profile.profile_complete);
    assert_eq!(profile.sampled_rows, 2);
    assert_eq!(profile.columns[0].inferred_type, "integer");
    assert_eq!(profile.columns[1].inferred_type, "boolean");

    let malformed = temp.path().join("malformed.csv");
    write_file(&malformed, &[0xff, b',', b'a', b'\n']);
    let profile = profile_dataset(&malformed);
    assert!(!profile.profile_complete);
    assert!(profile
        .quality_warnings
        .contains(&"header-parse-error".to_string()));
    assert_eq!(profile.sampled_rows, 0);
}

#[test]
fn byte_sampling_limit_is_explicit_and_never_returns_sample_values() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("large.csv");
    let private_value = "private-value-that-must-never-be-retained";
    let mut contents = String::from("value\n");
    while contents.len() <= 1024 * 1024 + 1024 {
        contents.push_str(private_value);
        contents.push('\n');
    }
    write_file(&path, contents.as_bytes());

    let profile = profile_dataset(&path);
    assert!(!profile.profile_complete);
    assert!(profile.sample_truncated);
    assert!(profile
        .quality_warnings
        .contains(&"byte-sample-limit-reached".to_string()));
    assert!(!serde_json::to_string(&profile).unwrap().contains(private_value));
}
