//! Real-filesystem regressions for the hardened node-view scanned-root authority.

use crate::node_view_guard::node_view;
use crate::scanner::{ScanResult, ScanStats};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

fn result_for(root: &Path) -> ScanResult {
    ScanResult {
        root: root.to_path_buf(),
        dir_sizes: HashMap::new(),
        top_files: Vec::new(),
        stats: ScanStats::default(),
        cancelled: false,
    }
}

#[test]
fn node_view_preserves_scanned_paths_and_sizes_for_authorized_directories() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    let child = root.join("child");
    fs::create_dir_all(&child).unwrap();
    fs::write(child.join("payload.bin"), b"payload").unwrap();

    let mut scan = result_for(&root);
    scan.dir_sizes.insert(root.clone(), 7);
    scan.dir_sizes.insert(child.clone(), 7);

    let view = node_view(&scan, &root).expect("ordinary scanned directory must remain visible");
    assert_eq!(view.path, root.to_string_lossy());
    assert_eq!(view.size, 7);
    assert_eq!(view.entries.len(), 1);
    assert_eq!(view.entries[0].path, child.to_string_lossy());
    assert_eq!(view.entries[0].size, 7);
    assert!(view.entries[0].is_dir);
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
