#![cfg(unix)]

use std::fs::{self, OpenOptions};
use std::os::unix::fs::PermissionsExt;

#[test]
fn cache_targets_fail_closed_before_reading_beyond_manifest_byte_budget() {
    const MAX_MANIFEST_BYTES: u64 = 8 * 1024 * 1024 * 1024;

    let temp = tempfile::tempdir().expect("create isolated cache manifest fixture");
    let oversized = temp.path().join("oversized.bin");
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&oversized)
        .expect("create sparse oversized cache target");
    file.set_len(MAX_MANIFEST_BYTES + 1)
        .expect("extend sparse cache target without allocating its logical size");
    drop(file);

    let mut permissions = fs::metadata(&oversized)
        .expect("read cache target metadata")
        .permissions();
    permissions.set_mode(0o000);
    fs::set_permissions(&oversized, permissions).expect("make content unreadable");

    let error = disksage_lib::rules::cache_targets(temp.path())
        .expect_err("oversized manifest input must fail closed before file content is opened");

    assert_eq!(error, "cache-target-manifest-byte-limit-exceeded");
}
