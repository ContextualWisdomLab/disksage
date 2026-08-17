//! Security regressions for scan-tree navigation.
//!
//! These tests exercise the same filesystem boundary as the registered Tauri `get_node` command
//! and the legacy library helper without introducing GUI/runtime dependencies. A lexical
//! descendant is not sufficient authority: the final directory object must still resolve within
//! the canonical scanned root.

use crate::{commands, node_navigation, scanner};
use std::sync::atomic::AtomicBool;

fn scanned_escape_fixture() -> (tempfile::TempDir, tempfile::TempDir, std::path::PathBuf, scanner::ScanResult) {
    let scanned = tempfile::tempdir().expect("temporary scan root");
    let external = tempfile::tempdir().expect("temporary external root");
    std::fs::write(external.path().join("outside-secret.bin"), b"outside")
        .expect("write external fixture");

    let escape = scanned.path().join("escape");
    std::os::unix::fs::symlink(external.path(), &escape).expect("create directory symlink");

    let result = scanner::scan_dir_with_interval(
        scanned.path(),
        &AtomicBool::new(false),
        1,
        |_| {},
    );
    (scanned, external, escape, result)
}

#[cfg(unix)]
#[test]
fn node_view_rejects_final_directory_symlink_escape() {
    let (_scanned, _external, escape, result) = scanned_escape_fixture();

    let view = node_navigation::node_view(&result, &escape);
    assert!(
        view.is_err(),
        "a final directory symlink below the lexical root must not expose metadata outside the scanned root"
    );
}

#[cfg(unix)]
#[test]
fn legacy_commands_node_view_rejects_final_directory_symlink_escape() {
    let (_scanned, _external, escape, result) = scanned_escape_fixture();

    let view = commands::node_view(&result, &escape);
    assert!(
        view.is_err(),
        "the legacy node_view helper must enforce the same canonical-root authority as the registered Tauri command"
    );
}
