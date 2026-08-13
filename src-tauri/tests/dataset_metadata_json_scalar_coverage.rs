//! Public-edge coverage for JSONL scalar classification and malformed delimited rows.
//!
//! Fixtures remain local and assert only schema-level metadata; sampled values must never be
//! retained in the returned profile.

use disksage_lib::profile_dataset;
use std::io::Write;

fn write_file(path: &std::path::Path, bytes: &[u8]) {
    let mut file = std::fs::File::create(path).unwrap();
    file.write_all(bytes).unwrap();
}

fn column<'a>(
    profile: &'a disksage_lib::DatasetProfile,
    name: &str,
) -> &'a disksage_lib::DatasetColumnProfile {
    profile
        .columns
        .iter()
        .find(|column| column.name == name)
        .unwrap()
}

#[test]
fn jsonl_scalar_families_preserve_types_counts_and_value_privacy() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("scalar-families.jsonl");
    write_file(
        &path,
        br#"{"json_value":["private-array-value"],"float_value":1.25,"bool_value":true,"int_value":7,"date_value":"2026-08-13","text_value":"private-text-value","null_then_bool":null}
{"json_value":{"private-key":"private-object-value"},"float_value":2,"bool_value":false,"int_value":8,"date_value":"2026-08-14T01:02:03Z","text_value":"another-private-text","null_then_bool":true}
"#,
    );

    let profile = profile_dataset(&path);
    assert!(profile.profile_complete);
    assert_eq!(profile.sampled_rows, 2);
    assert_eq!(column(&profile, "json_value").inferred_type, "json");
    assert_eq!(column(&profile, "float_value").inferred_type, "number");
    assert_eq!(column(&profile, "bool_value").inferred_type, "boolean");
    assert_eq!(column(&profile, "int_value").inferred_type, "integer");
    assert_eq!(column(&profile, "date_value").inferred_type, "datetime");
    assert_eq!(column(&profile, "text_value").inferred_type, "text");
    assert_eq!(column(&profile, "null_then_bool").inferred_type, "boolean");
    assert_eq!(column(&profile, "null_then_bool").observed_values, 1);
    assert_eq!(column(&profile, "null_then_bool").missing_values, 1);

    let serialized = serde_json::to_string(&profile).unwrap();
    for private_value in [
        "private-array-value",
        "private-object-value",
        "private-text-value",
        "another-private-text",
    ] {
        assert!(!serialized.contains(private_value));
    }
}

#[test]
fn malformed_and_empty_csv_rows_fail_closed_without_retaining_input_values() {
    let temp = tempfile::tempdir().unwrap();

    let empty = temp.path().join("empty.csv");
    write_file(&empty, b"");
    let empty_profile = profile_dataset(&empty);
    assert!(!empty_profile.profile_complete);
    assert_eq!(empty_profile.sampled_rows, 0);
    assert!(empty_profile
        .quality_warnings
        .contains(&"missing-header".to_string()));

    let short_row = temp.path().join("short-row.csv");
    write_file(&short_row, b"a,b,c\nprivate-a,private-b\n");
    let short_profile = profile_dataset(&short_row);
    assert!(!short_profile.profile_complete);
    assert_eq!(short_profile.sampled_rows, 1);
    assert_eq!(short_profile.columns[2].missing_values, 1);
    assert!(short_profile
        .quality_warnings
        .contains(&"inconsistent-row-width".to_string()));
    let serialized = serde_json::to_string(&short_profile).unwrap();
    assert!(!serialized.contains("private-a"));
    assert!(!serialized.contains("private-b"));

    let invalid_utf8 = temp.path().join("invalid-utf8-row.csv");
    write_file(&invalid_utf8, &[b'a', b',', b'b', b'\n', b'1', b',', 0xff, b'\n']);
    let invalid_profile = profile_dataset(&invalid_utf8);
    assert!(!invalid_profile.profile_complete);
    assert!(invalid_profile
        .quality_warnings
        .contains(&"record-parse-error".to_string()));
}
