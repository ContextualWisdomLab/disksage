//! macOS regression tests for the strict current-user temporary-directory exception.
//!
//! These tests remain outside `safety.rs` so the platform contract is explicit without exposing
//! the private safety module as a public library API.

#![cfg(target_os = "macos")]

use crate::rules::{cache_candidates, BaseDirs};
use crate::safety::is_protected;
use std::path::{Path, PathBuf};

/// Returns the canonical process-specific temporary root used by the guard.
fn current_temp_root() -> PathBuf {
    std::fs::canonicalize(std::env::temp_dir())
        .expect("the macOS process temporary directory must be canonicalizable")
}

#[test]
fn protects_temp_root_and_allows_only_a_strict_descendant() {
    let platform_root = Path::new("/private/var/folders");
    let temp_root = current_temp_root();
    assert!(temp_root.starts_with(platform_root));
    assert!(is_protected(platform_root));
    assert!(is_protected(&temp_root));

    let owned_descendant = temp_root.join("disksage-current-user-owned-fixture");
    assert!(!is_protected(&owned_descendant));
}

#[test]
fn rejects_a_sibling_user_temporary_tree() {
    let sibling = Path::new("/private/var/folders/disksage-not-current-user/T/candidate");
    assert!(!sibling.starts_with(current_temp_root()));
    assert!(is_protected(sibling));
}

#[test]
fn rejects_a_canonicalized_symlink_to_a_protected_target() {
    use std::os::unix::fs::symlink;

    let fixture = tempfile::tempdir().expect("create a current-user temporary fixture");
    let link = fixture.path().join("protected-system-link");
    symlink("/System", &link).expect("create a symlink to the protected system root");

    let canonical_target = std::fs::canonicalize(&link).expect("canonicalize the protected target");
    assert_eq!(canonical_target, Path::new("/System"));
    assert!(is_protected(&canonical_target));
}

#[test]
fn cache_totals_exclude_disksage_trash_staging() {
    let fixture = tempfile::tempdir().expect("create cache accounting fixture");
    let cache_root = fixture.path().join("cache");
    let staging = cache_root.join(".disksage-trash-fixture");
    std::fs::create_dir_all(&staging).expect("create staging fixture");
    std::fs::write(cache_root.join("live.bin"), b"live").expect("write live cache fixture");
    std::fs::write(staging.join("staged.bin"), vec![0_u8; 100])
        .expect("write staged cache fixture");

    let bases = BaseDirs {
        temp: cache_root,
        local_data: fixture.path().join("local"),
        home: fixture.path().join("home"),
    };
    let candidate = cache_candidates(&bases)
        .into_iter()
        .find(|candidate| candidate.id == "os-temp")
        .expect("OS temp candidate must be catalogued");

    assert_eq!(candidate.bytes, 4);
}
