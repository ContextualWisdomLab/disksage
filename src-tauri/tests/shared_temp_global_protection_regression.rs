#[cfg(unix)]
#[test]
fn shared_temp_children_remain_globally_protected() {
    use disksage_lib::safety::is_protected;
    use std::fs;

    let shared_root = if cfg!(target_os = "macos") {
        std::path::Path::new("/private/tmp")
    } else {
        std::path::Path::new("/tmp")
    };
    let directory = tempfile::Builder::new()
        .prefix("disksage-global-protection-")
        .tempdir_in(shared_root)
        .expect("create user-owned shared-temp directory");
    fs::write(directory.path().join("owned.txt"), b"owned")
        .expect("populate user-owned shared-temp directory");

    assert!(
        is_protected(directory.path()),
        "the global safety predicate must not grant every caller authority over user-owned shared-temp trees"
    );
}
