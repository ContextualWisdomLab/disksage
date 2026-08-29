#![cfg(unix)]

use disksage_lib::git_clone_reclaim::{inventory_standalone_clones, CloneInventoryOptions};
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

#[test]
fn unreadable_depth_boundary_marks_inventory_incomplete() {
    let root = tempfile::tempdir().unwrap();
    let boundary = root.path().join("one");
    let hidden_clone = boundary.join("two/repo");
    std::fs::create_dir_all(&hidden_clone).unwrap();
    let git = Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(&hidden_clone)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .unwrap();
    assert!(
        git.status.success(),
        "{}",
        String::from_utf8_lossy(&git.stderr)
    );

    let original_permissions = std::fs::symlink_metadata(&boundary).unwrap().permissions();
    let mut unreadable_permissions = original_permissions.clone();
    unreadable_permissions.set_mode(0o000);
    std::fs::set_permissions(&boundary, unreadable_permissions).unwrap();

    let report = inventory_standalone_clones(
        &[root.path().to_path_buf()],
        CloneInventoryOptions {
            max_depth: 1,
            ..CloneInventoryOptions::default()
        },
    );

    std::fs::set_permissions(&boundary, original_permissions).unwrap();
    let report = report.unwrap();
    assert!(report.clone_roots.is_empty());
    assert!(
        !report.evidence_complete,
        "unreadable depth-boundary traversal must not authorize complete evidence: {:?}",
        report.issues
    );
    assert!(
        report
            .issues
            .contains(&"git-clone-inventory-directory-unreadable".to_string()),
        "unreadable boundary must remain observable: {:?}",
        report.issues
    );
}
