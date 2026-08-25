#[path = "../src/bound_read_root.rs"]
mod bound_read_root;

#[cfg(windows)]
#[test]
fn windows_junction_child_never_gains_directory_traversal_authority() {
    use bound_read_root::{BoundEntryKind, BoundReadRoot};
    use std::path::Path;
    use std::process::Command;

    let fixture = tempfile::tempdir().expect("temporary fixture");
    let selected_root = fixture.path().join("selected");
    let outside = fixture.path().join("outside");
    let junction = selected_root.join("junction");
    std::fs::create_dir(&selected_root).expect("selected root");
    std::fs::create_dir(&outside).expect("outside directory");
    std::fs::write(outside.join("outside.txt"), b"outside").expect("outside marker");

    let status = Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&junction)
        .arg(&outside)
        .status()
        .expect("create Windows junction fixture");
    assert!(status.success(), "junction fixture must be created");

    let guard = BoundReadRoot::open(&selected_root).expect("selected root must bind");
    assert_eq!(
        guard.entry_kind(Path::new("junction")).expect("junction kind"),
        BoundEntryKind::Symlink,
        "a directory reparse point must never be admitted as a traversable directory"
    );
    assert!(
        guard.read_dir_names(Path::new("junction")).is_err(),
        "the traversal primitive itself must reject a directory junction instead of following it outside the bound root"
    );
    assert!(
        guard.entry_kind(Path::new("junction/outside.txt")).is_err(),
        "entry inspection must reject a junction in an intermediate component"
    );
    assert!(
        guard.open_file(Path::new("junction/outside.txt")).is_err(),
        "file reads must reject a junction in an intermediate component rather than opening the external target"
    );
}
