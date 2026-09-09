//! Security regressions for scan-tree navigation.
//!
//! These tests exercise the same filesystem boundary as the registered Tauri `get_node` command
//! without introducing GUI/runtime dependencies. A lexical descendant is not sufficient authority:
//! the final directory object must still resolve within the canonical scanned root.

use crate::{node_navigation, scanner};
use std::sync::atomic::AtomicBool;

#[cfg(unix)]
#[test]
fn node_view_rejects_final_directory_symlink_escape() {
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

    let view = node_navigation::node_view(&result, &escape);
    assert!(
        view.is_err(),
        "a final directory symlink below the lexical root must not expose metadata outside the scanned root"
    );
}
