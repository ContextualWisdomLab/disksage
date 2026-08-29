#![cfg(all(unix, not(target_os = "macos")))]

use std::fs::{self, OpenOptions};
use std::os::unix::fs::PermissionsExt;

#[test]
fn sparse_large_cache_target_is_inventoried_without_content_reads() {
    const LARGE_LOGICAL_BYTES: u64 = 8 * 1024 * 1024 * 1024 + 1;

    let temp = tempfile::tempdir().expect("create isolated cache manifest fixture");
    let cache_root = temp.path().join("pip");
    fs::create_dir_all(&cache_root).expect("create catalog cache root");
    let oversized = cache_root.join("oversized.bin");
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&oversized)
        .expect("create sparse oversized cache target");
    file.set_len(LARGE_LOGICAL_BYTES)
        .expect("extend sparse cache target without allocating its logical size");
    drop(file);

    let mut permissions = fs::metadata(&oversized)
        .expect("read cache target metadata")
        .permissions();
    permissions.set_mode(0o000);
    fs::set_permissions(&oversized, permissions).expect("make content unreadable");

    let environment_name = "XDG_CACHE_HOME";
    let old_environment = std::env::var_os(environment_name);
    unsafe { std::env::set_var(environment_name, temp.path()) };
    let targets =
        disksage_lib::cache_cleanup::list_cache_targets(cache_root.to_string_lossy().into_owned())
            .expect("metadata-only manifest must not open unreadable sparse file content");
    match old_environment {
        Some(value) => unsafe { std::env::set_var(environment_name, value) },
        None => unsafe { std::env::remove_var(environment_name) },
    }

    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].bytes, LARGE_LOGICAL_BYTES);
}
