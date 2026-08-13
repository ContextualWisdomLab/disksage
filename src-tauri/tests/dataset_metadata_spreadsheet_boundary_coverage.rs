//! Public-contract coverage for spreadsheet admission boundaries.
//!
//! These tests exercise metadata and workbook admission without reading user cell values.

use disksage_lib::profile_dataset;
use std::io::Write;

#[test]
fn missing_spreadsheet_fails_closed_before_workbook_open() {
    let temp = tempfile::tempdir().unwrap();
    let profile = profile_dataset(&temp.path().join("missing.XLSX"));

    assert_eq!(profile.format, "xlsx");
    assert!(!profile.profile_complete);
    assert!(!profile.sample_truncated);
    assert_eq!(
        profile.quality_warnings,
        vec!["dataset-open-error".to_string()]
    );
}

#[test]
fn oversized_spreadsheet_fails_closed_before_parser_allocation() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("oversized.xlsx");
    let mut file = std::fs::File::create(&path).unwrap();
    file.write_all(b"not-a-workbook").unwrap();
    file.set_len(64 * 1024 * 1024 + 1).unwrap();
    drop(file);

    let profile = profile_dataset(&path);

    assert_eq!(profile.format, "xlsx");
    assert!(!profile.profile_complete);
    assert!(profile.sample_truncated);
    assert_eq!(
        profile.quality_warnings,
        vec!["spreadsheet-size-limit-exceeded".to_string()]
    );
}

#[test]
fn supported_spreadsheet_extensions_are_normalized_before_fail_closed_open() {
    let temp = tempfile::tempdir().unwrap();

    for extension in ["XLS", "XLSM", "XLSB", "ODS"] {
        let path = temp.path().join(format!("corrupt.{extension}"));
        std::fs::write(&path, b"not-a-workbook").unwrap();

        let profile = profile_dataset(&path);

        assert_eq!(profile.format, extension.to_ascii_lowercase());
        assert!(!profile.profile_complete);
        assert!(!profile.sample_truncated);
        assert_eq!(
            profile.quality_warnings,
            vec!["spreadsheet-open-error".to_string()]
        );
    }
}
