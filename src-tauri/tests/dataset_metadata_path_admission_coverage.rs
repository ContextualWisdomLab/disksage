//! Public path-admission coverage for dataset metadata profiling.
//!
//! These regressions keep sampled values ephemeral while proving that extension handling is
//! case-insensitive for supported formats and fail-closed for host paths that cannot be represented
//! as a supported UTF-8 extension.

#![cfg(not(coverage))]

use disksage_lib::profile_dataset;

#[test]
fn uppercase_delimited_extension_is_normalized_without_retaining_values() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("customer-export.CSV");
    let private_value = "person@example.invalid";
    std::fs::write(&path, format!("email,active\n{private_value},true\n")).unwrap();

    let profile = profile_dataset(&path);

    assert_eq!(profile.format, "csv");
    assert!(profile.profile_complete);
    assert_eq!(profile.sampled_rows, 1);
    assert!(profile.columns[0].sensitive_name);
    assert_eq!(profile.columns[1].inferred_type, "boolean");
    assert!(!serde_json::to_string(&profile).unwrap().contains(private_value));
}

#[cfg(unix)]
#[test]
fn non_utf8_extension_fails_closed_without_reading_or_retaining_payload() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let temp = tempfile::tempdir().unwrap();
    let mut filename = b"customer-export.".to_vec();
    filename.extend_from_slice(&[0xff, 0xfe]);
    let path = temp.path().join(OsString::from_vec(filename));
    let private_value = b"private-payload-that-must-not-be-profiled";
    std::fs::write(&path, private_value).unwrap();

    let profile = profile_dataset(&path);

    assert!(!profile.profile_complete);
    assert_eq!(profile.sampled_rows, 0);
    assert_eq!(profile.quality_warnings, vec!["unsupported-dataset-format"]);
    let serialized = serde_json::to_string(&profile).unwrap();
    assert!(!serialized.contains("private-payload-that-must-not-be-profiled"));
}
