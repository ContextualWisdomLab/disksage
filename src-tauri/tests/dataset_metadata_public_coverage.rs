//! Public-contract coverage for bounded dataset metadata profiling.
//!
//! Fixtures are synthetic and temporary. The profiler must report schema/type/quality evidence
//! without serializing sampled cell values into the returned profile.

#![cfg(not(coverage))]

use disksage_lib::profile_dataset;
use std::io::Write;

fn write_file(path: &std::path::Path, bytes: &[u8]) {
    let mut file = std::fs::File::create(path).unwrap();
    file.write_all(bytes).unwrap();
}

#[test]
fn delimited_profiles_cover_schema_quality_and_type_transitions() {
    let temp = tempfile::tempdir().unwrap();
    let csv = temp.path().join("mixed.csv");
    write_file(
        &csv,
        b"email,number,date,when,payload,empty\nfirst@example.com,1,2026-01-01,2026-01-01T10:00:00Z,text,\nsecond@example.com,1.5,not-a-date,later,2,true\n",
    );

    let profile = profile_dataset(&csv);
    assert_eq!(profile.format, "csv");
    assert_eq!(profile.sampled_rows, 2);
    assert!(profile.profile_complete);
    assert_eq!(profile.columns[0].inferred_type, "text");
    assert!(profile.columns[0].sensitive_name);
    assert_eq!(profile.columns[1].inferred_type, "number");
    assert_eq!(profile.columns[2].inferred_type, "mixed");
    assert_eq!(profile.columns[3].inferred_type, "mixed");
    assert_eq!(profile.columns[4].inferred_type, "mixed");
    assert_eq!(profile.columns[5].inferred_type, "boolean");
    assert_eq!(profile.columns[5].missing_values, 1);
    assert!(profile
        .quality_warnings
        .contains(&"sensitive-column-name-detected".to_string()));

    let serialized = serde_json::to_string(&profile).unwrap();
    assert!(!serialized.contains("first@example.com"));
    assert!(!serialized.contains("second@example.com"));
}

#[test]
fn malformed_and_ambiguous_delimited_inputs_fail_closed() {
    let temp = tempfile::tempdir().unwrap();

    let empty = temp.path().join("empty.csv");
    write_file(&empty, b"");
    let profile = profile_dataset(&empty);
    assert!(!profile.profile_complete);
    assert!(profile
        .quality_warnings
        .iter()
        .any(|warning| warning == "header-parse-error" || warning == "missing-header"));

    let ambiguous = temp.path().join("ambiguous.csv");
    write_file(&ambiguous, b"name,name,\nAlice\n");
    let profile = profile_dataset(&ambiguous);
    assert!(!profile.profile_complete);
    for warning in [
        "duplicate-column-name",
        "empty-column-name",
        "inconsistent-row-width",
    ] {
        assert!(profile.quality_warnings.contains(&warning.to_string()));
    }

    let tsv = temp.path().join("data.tsv");
    write_file(&tsv, b"id\tactive\n1\ttrue\n2\tfalse\n");
    let profile = profile_dataset(&tsv);
    assert!(profile.profile_complete);
    assert_eq!(profile.columns[0].inferred_type, "integer");
    assert_eq!(profile.columns[1].inferred_type, "boolean");
}

#[test]
fn jsonl_profiles_union_keys_and_reject_invalid_records_without_values() {
    let temp = tempfile::tempdir().unwrap();
    let jsonl = temp.path().join("records.jsonl");
    write_file(
        &jsonl,
        b"{\"patient_id\":1,\"flag\":true,\"score\":1.5,\"payload\":{\"nested\":1}}\n{\"patient_id\":null,\"flag\":false,\"items\":[1,2]}\n[]\n\nnot-json\n",
    );

    let profile = profile_dataset(&jsonl);
    assert_eq!(profile.format, "jsonl");
    assert!(!profile.profile_complete);
    assert_eq!(profile.sampled_rows, 3);
    let patient = profile
        .columns
        .iter()
        .find(|column| column.name == "patient_id")
        .unwrap();
    assert!(patient.sensitive_name);
    assert_eq!(patient.inferred_type, "integer");
    assert_eq!(patient.observed_values, 1);
    assert_eq!(patient.missing_values, 2);
    assert_eq!(
        profile
            .columns
            .iter()
            .find(|column| column.name == "payload")
            .unwrap()
            .inferred_type,
        "json"
    );
    assert_eq!(
        profile
            .columns
            .iter()
            .find(|column| column.name == "items")
            .unwrap()
            .inferred_type,
        "json"
    );
    for warning in [
        "blank-jsonl-line",
        "jsonl-record-not-object",
        "record-parse-error",
        "sensitive-column-name-detected",
    ] {
        assert!(profile.quality_warnings.contains(&warning.to_string()));
    }
}

#[test]
fn unsupported_missing_and_bounded_files_report_stable_quality_codes() {
    let temp = tempfile::tempdir().unwrap();

    let unsupported = temp.path().join("dataset.parquet");
    write_file(&unsupported, b"not parsed");
    let profile = profile_dataset(&unsupported);
    assert_eq!(profile.format, "parquet");
    assert_eq!(
        profile.quality_warnings,
        vec!["unsupported-dataset-format".to_string()]
    );

    let missing = profile_dataset(&temp.path().join("missing.csv"));
    assert_eq!(missing.format, "csv");
    assert_eq!(missing.quality_warnings, vec!["dataset-open-error".to_string()]);

    let no_extension = temp.path().join("dataset");
    write_file(&no_extension, b"content");
    let profile = profile_dataset(&no_extension);
    assert_eq!(profile.format, "unknown");
    assert_eq!(
        profile.quality_warnings,
        vec!["unsupported-dataset-format".to_string()]
    );

    let oversized = temp.path().join("oversized.csv");
    let mut file = std::fs::File::create(&oversized).unwrap();
    file.write_all(b"payload\n").unwrap();
    file.write_all(&vec![b'x'; 1024 * 1024 + 1]).unwrap();
    let profile = profile_dataset(&oversized);
    assert!(profile.sample_truncated);
    assert!(!profile.profile_complete);
    assert!(profile
        .quality_warnings
        .contains(&"byte-sample-limit-reached".to_string()));
}

#[test]
fn corrupt_spreadsheet_inputs_fail_closed_at_the_workbook_boundary() {
    let temp = tempfile::tempdir().unwrap();
    for extension in ["xlsx", "xls", "ods"] {
        let path = temp.path().join(format!("corrupt.{extension}"));
        write_file(&path, b"not-a-workbook");
        let profile = profile_dataset(&path);
        assert_eq!(profile.format, extension);
        assert!(!profile.profile_complete);
        assert_eq!(
            profile.quality_warnings,
            vec!["spreadsheet-open-error".to_string()]
        );
    }
}

#[test]
fn public_profile_path_enforces_the_row_sample_limit() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("bounded.csv");
    let mut contents = String::from("id\n");
    for index in 0..101 {
        contents.push_str(&format!("{index}\n"));
    }
    write_file(&path, contents.as_bytes());

    let profile = profile_dataset(&path);
    assert_eq!(profile.sampled_rows, 100);
    assert!(profile.sample_truncated);
    assert!(!profile.profile_complete);
    assert!(profile
        .quality_warnings
        .contains(&"row-sample-limit-reached".to_string()));
}
