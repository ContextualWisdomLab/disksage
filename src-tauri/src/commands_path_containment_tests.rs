//! Real-filesystem regressions for the scanned-root authority enforced by `commands::node_view`.

use crate::commands::node_view;
use crate::scanner::{ScanResult, ScanStats};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

fn result_for(root: &Path) -> ScanResult {
    ScanResult {
        root: root.to_path_buf(),
        dir_sizes: HashMap::new(),
        top_files: Vec::new(),
        stats: ScanStats::default(),
        cancelled: false,
    }
}

#[cfg(unix)]
#[test]
fn node_view_rejects_symlinked_directory_that_escapes_scanned_root() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    let outside = temp.path().join("outside");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join("secret.bin"), b"outside").unwrap();
    std::os::unix::fs::symlink(&outside, root.join("outside-link")).unwrap();

    let result = node_view(&result_for(&root), &root.join("outside-link"));

    assert_eq!(result.err().as_deref(), Some("path outside scanned root"));
}

#[cfg(unix)]
#[test]
fn node_view_rejects_directory_below_symlinked_ancestor_that_escapes_scanned_root() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    let outside = temp.path().join("outside");
    let nested = outside.join("nested");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&nested).unwrap();
    fs::write(nested.join("secret.bin"), b"outside").unwrap();
    std::os::unix::fs::symlink(&outside, root.join("outside-link")).unwrap();

    let result = node_view(
        &result_for(&root),
        &root.join("outside-link").join("nested"),
    );

    assert_eq!(result.err().as_deref(), Some("path outside scanned root"));
}
