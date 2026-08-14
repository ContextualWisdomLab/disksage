#![cfg(not(coverage))]

use disksage_lib::profile_dataset;
use std::io::Write;

fn has_warning(profile: &disksage_lib::DatasetProfile, warning: &str) -> bool {
    profile
        .quality_warnings
        .iter()
        .any(|existing| existing == warning)
}

#[test]
fn dataset_profile_normalizes_extension_and_redacts_sensitive_values() {
    let temp_dir = tempfile::tempdir().expect("create dataset fixture directory");
    let path = temp_dir.path().join("CUSTOMERS.CSV");
    let mut file = std::fs::File::create(&path).expect("create CSV fixture");
    file.write_all(
        "전화 번호,amount\n010-1234-5678,1\n,2.5\n"
            .as_bytes(),
    )
    .expect("write CSV fixture");

    let profile = profile_dataset(&path);

    assert_eq!(profile.format, "csv");
    assert!(profile.profile_complete);
    assert_eq!(profile.sampled_rows, 2);
    assert!(profile.columns[0].sensitive_name);
    assert_eq!(profile.columns[0].missing_values, 1);
    assert_eq!(profile.columns[1].inferred_type, "number");
    assert!(has_warning(&profile, "sensitive-column-name-detected"));

    let serialized = serde_json::to_string(&profile).expect("serialize bounded profile");
    assert!(!serialized.contains("010-1234-5678"));
    assert!(!serialized.contains("2.5"));
}

#[test]
fn dataset_profile_fails_closed_on_invalid_utf8_header() {
    let temp_dir = tempfile::tempdir().expect("create dataset fixture directory");
    let path = temp_dir.path().join("invalid.csv");
    std::fs::write(&path, [0xff, b',', b'n', b'a', b'm', b'e', b'\n'])
        .expect("write malformed CSV fixture");

    let profile = profile_dataset(&path);

    assert!(!profile.profile_complete);
    assert_eq!(profile.sampled_rows, 0);
    assert!(has_warning(&profile, "header-parse-error"));
    assert!(has_warning(&profile, "no-data-rows"));
}

#[test]
fn dataset_profile_reports_ambiguous_headers_width_and_mixed_values() {
    let temp_dir = tempfile::tempdir().expect("create dataset fixture directory");
    let path = temp_dir.path().join("ambiguous.csv");
    std::fs::write(&path, ",Name,name\n,1,alpha\n,word\n")
        .expect("write ambiguous CSV fixture");

    let profile = profile_dataset(&path);

    assert_eq!(profile.format, "csv");
    assert_eq!(profile.sampled_rows, 2);
    assert!(!profile.profile_complete);
    assert!(has_warning(&profile, "empty-column-name"));
    assert!(has_warning(&profile, "duplicate-column-name"));
    assert!(has_warning(&profile, "inconsistent-row-width"));
    assert_eq!(profile.columns[0].name, "column_1");
    assert_eq!(profile.columns[1].inferred_type, "mixed");
    assert_eq!(profile.columns[2].missing_values, 1);
}

#[test]
fn dataset_profile_jsonl_reports_malformed_nonobject_and_column_limit_records() {
    let temp_dir = tempfile::tempdir().expect("create dataset fixture directory");
    let path = temp_dir.path().join("structural.jsonl");
    let mut file = std::fs::File::create(&path).expect("create JSONL fixture");
    writeln!(file).expect("write blank JSONL line");
    writeln!(file, "not-json").expect("write malformed JSONL line");
    writeln!(file, "[]").expect("write non-object JSONL line");

    let object = (0..130)
        .map(|index| (format!("field_{index}"), serde_json::json!(index)))
        .collect::<serde_json::Map<String, serde_json::Value>>();
    writeln!(file, "{}", serde_json::Value::Object(object))
        .expect("write oversized-column JSONL object");

    let profile = profile_dataset(&path);

    assert_eq!(profile.format, "jsonl");
    assert_eq!(profile.sampled_rows, 2);
    assert_eq!(profile.columns.len(), 128);
    assert!(!profile.profile_complete);
    assert!(has_warning(&profile, "blank-jsonl-line"));
    assert!(has_warning(&profile, "record-parse-error"));
    assert!(has_warning(&profile, "jsonl-record-not-object"));
    assert!(has_warning(&profile, "column-limit-exceeded"));
    assert!(profile
        .columns
        .iter()
        .all(|column| column.missing_values == 1 && column.observed_values == 1));
}

#[test]
fn dataset_profile_jsonl_covers_scalar_and_structured_value_kinds() {
    let temp_dir = tempfile::tempdir().expect("create dataset fixture directory");
    let path = temp_dir.path().join("typed.jsonl");
    std::fs::write(
        &path,
        concat!(
            "{\"flag\":true,\"count\":1,\"ratio\":1.5,\"date\":\"2026-08-14\",",
            "\"at\":\"2026-08-14T12:30:00Z\",\"payload\":[1,2]}\n",
            "{\"flag\":false,\"count\":2,\"ratio\":2.5,\"date\":\"2026-08-15\",",
            "\"at\":\"2026-08-15 13:30:00\",\"payload\":{\"ok\":true},\"late\":null}\n"
        ),
    )
    .expect("write JSONL fixture");

    let profile = profile_dataset(&path);

    assert!(profile.profile_complete);
    assert_eq!(profile.sampled_rows, 2);
    for (name, expected_type) in [
        ("flag", "boolean"),
        ("count", "integer"),
        ("ratio", "number"),
        ("date", "date"),
        ("at", "datetime"),
        ("payload", "json"),
    ] {
        let column = profile
            .columns
            .iter()
            .find(|column| column.name == name)
            .expect("expected profiled JSONL column");
        assert_eq!(column.inferred_type, expected_type, "column {name}");
    }
    let late = profile
        .columns
        .iter()
        .find(|column| column.name == "late")
        .expect("late JSONL column");
    assert_eq!(late.observed_values, 0);
    assert_eq!(late.missing_values, 2);
    assert_eq!(late.inferred_type, "unknown");
}

#[test]
fn dataset_profile_jsonl_row_limit_and_unknown_format_fail_closed() {
    let temp_dir = tempfile::tempdir().expect("create dataset fixture directory");
    let jsonl_path = temp_dir.path().join("many.jsonl");
    let mut file = std::fs::File::create(&jsonl_path).expect("create JSONL fixture");
    for index in 0..=100 {
        writeln!(file, "{{\"id\":{index}}}").expect("write JSONL row");
    }

    let limited = profile_dataset(&jsonl_path);
    assert_eq!(limited.sampled_rows, 100);
    assert!(limited.sample_truncated);
    assert!(!limited.profile_complete);
    assert!(has_warning(&limited, "row-sample-limit-reached"));

    let unknown_path = temp_dir.path().join("dataset");
    std::fs::write(&unknown_path, b"content").expect("write extensionless fixture");
    let unknown = profile_dataset(&unknown_path);
    assert_eq!(unknown.format, "unknown");
    assert!(!unknown.profile_complete);
    assert!(has_warning(&unknown, "unsupported-dataset-format"));
}
