//! Credential-free coverage for small public command-core error boundaries.
//!
//! These cases stay below Tauri/runtime authority and touch only temporary filesystem fixtures.

use disksage_lib::commands::{node_view, parse_move_entry};
use disksage_lib::scanner::{ScanResult, ScanStats};
use std::collections::HashMap;

#[test]
fn node_view_reports_enumeration_failure_inside_the_authorized_scan_root() {
    let temp = tempfile::tempdir().unwrap();
    let scan = ScanResult {
        root: temp.path().to_path_buf(),
        dir_sizes: HashMap::new(),
        top_files: Vec::new(),
        stats: ScanStats::default(),
        cancelled: false,
    };

    let missing_child = temp.path().join("missing-child");
    let error = node_view(&scan, &missing_child)
        .err()
        .expect("missing in-root directory must fail enumeration");
    assert!(!error.is_empty());
}

#[test]
fn move_journal_parser_rejects_entries_without_the_exact_separator() {
    assert_eq!(parse_move_entry("source-destination"), None);
    assert_eq!(parse_move_entry("source -> destination -> tail"), Some((
        "source".to_string(),
        "destination -> tail".to_string(),
    )));
}
