#![cfg(windows)]

#[path = "../src/bound_read_root.rs"]
mod bound_read_root;

use bound_read_root::{BoundEntryKind, BoundReadRoot};
use std::path::Path;

#[test]
fn windows_bound_root_rejects_parent_traversal_for_every_child_io_boundary() {
    let parent = tempfile::tempdir().expect("temporary authority parent");
    let selected = parent.path().join("selected");
    let outside = parent.path().join("outside");
    std::fs::create_dir(&selected).expect("create selected root");
    std::fs::create_dir(&outside).expect("create outside sibling");
    std::fs::write(outside.join("secret.txt"), b"outside-authority")
        .expect("write outside marker");

    let guard = BoundReadRoot::open(&selected).expect("real directory must bind");
    let escape = Path::new("../outside");
    let escaped_file = Path::new("../outside/secret.txt");

    assert!(
        guard.read_dir_names(escape).is_err(),
        "directory enumeration must reject parent traversal before joining it to the bound root"
    );
    assert!(
        guard.entry_kind(escaped_file).is_err(),
        "type inspection must reject parent traversal before filesystem access"
    );
    assert!(
        guard.open_file(escaped_file).is_err(),
        "file opening must never escape the selected authority root through '..' components"
    );
}

fn create_directory_reparse(link: &Path, target: &Path) {
    if std::os::windows::fs::symlink_dir(target, link).is_ok() {
        return;
    }
    let status = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .status()
        .expect("launch mklink junction fallback");
    assert!(status.success(), "create directory reparse fixture");
}

#[test]
fn windows_bound_root_rejects_descendant_reparse_redirects() {
    let parent = tempfile::tempdir().expect("temporary authority parent");
    let selected = parent.path().join("selected");
    let outside = parent.path().join("outside");
    let redirect = selected.join("redirect");
    std::fs::create_dir(&selected).expect("create selected root");
    std::fs::create_dir(&outside).expect("create outside sibling");
    std::fs::write(outside.join("secret.txt"), b"outside-authority")
        .expect("write outside marker");
    create_directory_reparse(&redirect, &outside);

    let guard = BoundReadRoot::open(&selected).expect("real directory must bind");
    assert_eq!(
        guard.entry_kind(Path::new("redirect")).expect("inspect descendant reparse"),
        BoundEntryKind::Symlink,
        "directory reparses must never be classified as ordinary directories"
    );
    assert!(
        guard.read_dir_names(Path::new("redirect")).is_err(),
        "directory enumeration must not follow a descendant reparse outside the authority root"
    );
    assert!(
        guard.open_file(Path::new("redirect/secret.txt")).is_err(),
        "file opening must not traverse a descendant reparse outside the authority root"
    );
}
