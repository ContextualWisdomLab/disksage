//! Credential-free integration coverage for public command helpers.
//!
//! These regressions exercise filesystem reads only inside temporary directories and do not call
//! Tauri runtime wrappers or grant mutation authority.

use disksage_lib::commands::{list_roots, load_ontology_from, node_view, parse_move_entry};
use disksage_lib::scanner::{ScanResult, ScanStats};
use std::collections::HashMap;
use std::path::PathBuf;

#[test]
fn node_view_rejects_escaping_and_unreadable_paths() {
    let root = tempfile::tempdir().unwrap();
    let result = ScanResult {
        root: root.path().to_path_buf(),
        dir_sizes: HashMap::new(),
        top_files: Vec::new(),
        stats: ScanStats::default(),
        cancelled: false,
    };

    assert_eq!(
        node_view(&result, &root.path().join(".." ).join("escape")).unwrap_err(),
        "path outside scanned root"
    );

    let outside = tempfile::tempdir().unwrap();
    assert_eq!(
        node_view(&result, outside.path()).unwrap_err(),
        "path outside scanned root"
    );

    assert!(node_view(&result, &root.path().join("missing")).is_err());
}

#[test]
fn node_view_reports_regular_files_and_known_directory_sizes() {
    let root = tempfile::tempdir().unwrap();
    let nested = root.path().join("nested");
    std::fs::create_dir(&nested).unwrap();
    let file = root.path().join("payload.bin");
    std::fs::write(&file, b"payload").unwrap();

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&file, root.path().join("payload-link")).unwrap();
    }

    let mut dir_sizes = HashMap::new();
    dir_sizes.insert(root.path().to_path_buf(), 17);
    dir_sizes.insert(nested.clone(), 10);
    let result = ScanResult {
        root: root.path().to_path_buf(),
        dir_sizes,
        top_files: Vec::new(),
        stats: ScanStats::default(),
        cancelled: false,
    };

    let view = node_view(&result, root.path()).unwrap();
    assert_eq!(view.path, root.path().to_string_lossy());
    assert_eq!(view.size, 17);
    assert_eq!(view.entries.len(), 2);
    assert_eq!(view.entries[0].path, nested.to_string_lossy());
    assert!(view.entries[0].is_dir);
    assert_eq!(view.entries[0].size, 10);
    assert_eq!(view.entries[1].path, file.to_string_lossy());
    assert!(!view.entries[1].is_dir);
    assert_eq!(view.entries[1].size, 7);
}

#[test]
fn move_entry_parser_and_ontology_loader_fail_closed() {
    assert_eq!(
        parse_move_entry("/source/a -> /destination/a"),
        Some(("/source/a".to_string(), "/destination/a".to_string()))
    );
    assert_eq!(parse_move_entry("missing delimiter"), None);

    let empty = load_ontology_from("").unwrap();
    assert!(empty.classes.is_empty());
    assert!(load_ontology_from("<not valid turtle").is_err());
}

#[test]
fn list_roots_exposes_only_platform_root_candidates() {
    let roots = list_roots();
    assert!(!roots.is_empty());
    #[cfg(not(windows))]
    assert_eq!(roots.first().map(String::as_str), Some("/"));
    assert!(roots.iter().all(|root| PathBuf::from(root).is_absolute()));
}
